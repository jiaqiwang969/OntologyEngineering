//! Streaming ZIP32/ZIP64 metadata checks before the ZIP library can conceal
//! duplicate central names in its IndexMap.
use super::{MAX_ARCHIVE_BYTES, MAX_FILE_BYTES, MAX_FILES, MAX_TOTAL_BYTES};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
};
pub(super) struct Member {
    pub name: String,
    pub bytes: u64,
    pub offset: u64,
    pub data_start: u64,
}
fn slice(bytes: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    bytes
        .get(at..at.checked_add(size).context("zip_metadata_overflow")?)
        .context("zip_metadata_truncated")
}
fn u16at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(bytes, at, 2)?.try_into()?))
}
fn u32at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(bytes, at, 4)?.try_into()?))
}
fn u64at(bytes: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(slice(bytes, at, 8)?.try_into()?))
}
fn read_at(file: &mut File, at: u64, count: usize) -> Result<Vec<u8>> {
    let length = file.metadata()?.len();
    ensure!(
        at.checked_add(count as u64)
            .is_some_and(|end| end <= length),
        "zip_metadata_outside_container"
    );
    file.seek(SeekFrom::Start(at))?;
    let mut bytes = vec![0; count];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub(super) fn inventory(file: &mut File) -> Result<Vec<Member>> {
    let length = file.metadata()?.len();
    ensure!(
        (22..=MAX_ARCHIVE_BYTES).contains(&length),
        "zip_container_size_limit"
    );
    let tail_size = length.min(22 + u16::MAX as u64) as usize;
    let tail_start = length - tail_size as u64;
    let tail = read_at(file, tail_start, tail_size)?;
    let end = (0..=tail.len() - 22)
        .rev()
        .find(|&at| {
            tail[at..at + 4] == *b"PK\x05\x06"
                && at + 22 + u16::from_le_bytes([tail[at + 20], tail[at + 21]]) as usize
                    == tail.len()
        })
        .context("zip_exact_end_record_missing")?;
    ensure!(
        u16at(&tail, end + 4)? == 0
            && u16at(&tail, end + 6)? == 0
            && u16at(&tail, end + 8)? == u16at(&tail, end + 10)?,
        "zip_split_container"
    );
    let end_offset = tail_start + end as u64;
    let mut count = u16at(&tail, end + 10)? as u64;
    let mut size = u32at(&tail, end + 12)? as u64;
    let mut start = u32at(&tail, end + 16)? as u64;
    let mut central_end = end_offset;
    let locator = if end_offset >= 20 {
        read_at(file, end_offset - 20, 20)?
    } else {
        Vec::new()
    };
    if locator.starts_with(b"PK\x06\x07") {
        ensure!(
            u32at(&locator, 4)? == 0 && u32at(&locator, 16)? == 1,
            "zip64_split_container"
        );
        let offset = u64at(&locator, 8)?;
        ensure!(
            offset.checked_add(56) == Some(end_offset - 20),
            "zip64_end_extent"
        );
        let record = read_at(file, offset, 56)?;
        ensure!(
            record.starts_with(b"PK\x06\x06")
                && u64at(&record, 4)? == 44
                && u32at(&record, 16)? == 0
                && u32at(&record, 20)? == 0
                && u64at(&record, 24)? == u64at(&record, 32)?,
            "zip64_end_contract"
        );
        let actual_count = u64at(&record, 32)?;
        let actual_size = u64at(&record, 40)?;
        let actual_start = u64at(&record, 48)?;
        ensure!(
            (count == u16::MAX as u64 || count == actual_count)
                && (size == u32::MAX as u64 || size == actual_size)
                && (start == u32::MAX as u64 || start == actual_start),
            "zip64_end_values_differ"
        );
        count = actual_count;
        size = actual_size;
        start = actual_start;
        central_end = offset;
    } else {
        ensure!(
            count != u16::MAX as u64 && size != u32::MAX as u64 && start != u32::MAX as u64,
            "zip64_locator_missing"
        );
    }
    ensure!(
        count > 0
            && count <= MAX_FILES as u64 + 1
            && size <= 16 * 1024 * 1024
            && start.checked_add(size) == Some(central_end),
        "zip_central_extent_or_count_limit"
    );
    let central = read_at(file, start, size as usize)?;
    let mut cursor = 0usize;
    let mut names = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut expanded = 0u64;
    let mut result = Vec::new();
    for _ in 0..count {
        ensure!(u32at(&central, cursor)? == 0x02014b50, "zip_central_header");
        let flags = u16at(&central, cursor + 8)?;
        let method = u16at(&central, cursor + 10)?;
        ensure!(
            flags & !0x0808 == 0 && matches!(method, 0 | 8),
            "zip_flags_or_compression_rejected"
        );
        let mut compressed = u32at(&central, cursor + 20)? as u64;
        let mut bytes = u32at(&central, cursor + 24)? as u64;
        let name_len = u16at(&central, cursor + 28)? as usize;
        let extra_len = u16at(&central, cursor + 30)? as usize;
        let comment_len = u16at(&central, cursor + 32)? as usize;
        ensure!(
            name_len > 0 && name_len <= 4096 && u16at(&central, cursor + 34)? == 0,
            "zip_name_or_disk_limit"
        );
        let kind = (u32at(&central, cursor + 38)? >> 16) & 0o170000;
        ensure!(matches!(kind, 0 | 0o100000), "zip_nonregular_member");
        let mut offset = u32at(&central, cursor + 42)? as u64;
        let name_bytes = slice(&central, cursor + 46, name_len)?;
        let name = std::str::from_utf8(name_bytes)?.to_owned();
        oe_semantic::safe_relative(&name)?;
        ensure!(names.insert(name.clone()), "zip_duplicate_member");
        let extra = slice(&central, cursor + 46 + name_len, extra_len)?;
        let mut at = 0usize;
        let mut zip64 = None;
        while at < extra.len() {
            let tag = u16at(extra, at)?;
            let length = u16at(extra, at + 2)? as usize;
            let field = slice(extra, at + 4, length)?;
            if tag == 1 {
                ensure!(zip64.is_none(), "zip64_duplicate_extra");
                zip64 = Some(field);
            }
            at = at.checked_add(4 + length).context("zip_extra_overflow")?;
        }
        let mut slot = 0usize;
        for value in [&mut bytes, &mut compressed, &mut offset] {
            if *value == u32::MAX as u64 {
                *value = u64at(zip64.context("zip64_member_extra_missing")?, slot)?;
                slot += 8;
            }
        }
        ensure!(bytes <= MAX_FILE_BYTES, "zip_expanded_member_limit");
        expanded = expanded
            .checked_add(bytes)
            .context("zip_expanded_size_overflow")?;
        ensure!(
            expanded <= MAX_TOTAL_BYTES + 4 * 1024 * 1024,
            "zip_expanded_total_limit"
        );
        let local = read_at(file, offset, 30)?;
        ensure!(
            local.starts_with(b"PK\x03\x04")
                && u16at(&local, 6)? == flags
                && u16at(&local, 8)? == method,
            "zip_local_metadata_differs"
        );
        let local_name = u16at(&local, 26)? as usize;
        let local_extra = u16at(&local, 28)? as u64;
        ensure!(
            local_name == name_len
                && read_at(
                    file,
                    offset.checked_add(30).context("zip_offset_overflow")?,
                    local_name
                )? == name_bytes,
            "zip_local_name_differs"
        );
        let data_start = offset
            .checked_add(30 + local_name as u64 + local_extra)
            .context("zip_offset_overflow")?;
        let data_end = data_start
            .checked_add(compressed)
            .context("zip_offset_overflow")?;
        ensure!(data_end <= start, "zip_member_overlaps_directory");
        ranges.push((offset, data_end));
        result.push(Member {
            name,
            bytes,
            offset,
            data_start,
        });
        cursor = cursor
            .checked_add(46 + name_len + extra_len + comment_len)
            .context("zip_central_overflow")?;
        ensure!(cursor <= central.len(), "zip_central_member_extent");
    }
    ensure!(
        cursor == central.len(),
        "zip_central_inventory_size_differs"
    );
    ranges.sort_unstable();
    ensure!(
        ranges.first().is_some_and(|v| v.0 == 0)
            && ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0),
        "zip_preamble_or_overlapping_members"
    );
    Ok(result)
}
