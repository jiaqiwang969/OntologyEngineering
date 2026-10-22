//! Regular archive byte algorithm. Native execution remains the existing callback owner's job.
use anyhow::{Context, Result, ensure};
use oe_core::sha256;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Member {
    pub ordinal: usize,
    pub name: String,
    pub date: String,
    pub uid: String,
    pub gid: String,
    pub mode: String,
    pub sha256: String,
    pub bytes: usize,
    #[serde(skip)]
    pub payload: Vec<u8>,
    #[serde(skip)]
    pub header: Vec<u8>,
    #[serde(skip)]
    pub name_bytes: Vec<u8>,
    #[serde(skip)]
    pub header_ordinal: usize,
    #[serde(skip)]
    pub header_offset: usize,
}
#[derive(Debug, Clone)]
pub(crate) struct Structure {
    pub ordinal: usize,
    pub name: String,
    pub header: Vec<u8>,
    pub name_bytes: Vec<u8>,
    pub payload: Vec<u8>,
}
pub(crate) struct Archive {
    pub members: Vec<Member>,
    pub structure: Vec<Structure>,
}
pub(crate) fn parse(raw: &[u8]) -> Result<Archive> {
    ensure!(
        raw.starts_with(b"!<arch>\n"),
        "archive_regular_magic_unsupported"
    );
    let mut pos = 8usize;
    let mut ordinal = 0;
    let mut members = Vec::new();
    let mut structure = Vec::new();
    let mut table = Vec::new();
    while pos < raw.len() {
        let endhead = pos.checked_add(60).context("archive_offset_overflow")?;
        let h = raw.get(pos..endhead).context("archive_header_truncated")?;
        ensure!(&h[58..60] == b"`\n", "archive_header_terminator");
        let field =
            |a, b| -> Result<String> { Ok(std::str::from_utf8(&h[a..b])?.trim().to_owned()) };
        let encoded = field(0, 16)?;
        let size: usize = field(48, 58)?.parse()?;
        ensure!(size <= 64 * 1024 * 1024, "archive_member_limit");
        let end = endhead
            .checked_add(size)
            .context("archive_offset_overflow")?;
        let data = raw.get(endhead..end).context("archive_payload_truncated")?;
        let (name, name_bytes, payload) = if let Some(n) = encoded.strip_prefix("#1/") {
            let n: usize = n.parse()?;
            let name_bytes = data.get(..n).context("archive_extended_name_truncated")?;
            let name = std::str::from_utf8(name_bytes)?
                .trim_end_matches('\0')
                .to_owned();
            (
                name,
                name_bytes.to_vec(),
                data.get(n..).context("archive_payload_truncated")?.to_vec(),
            )
        } else if encoded == "//" {
            table = data.to_vec();
            ("//".to_owned(), Vec::new(), data.to_vec())
        } else if encoded.starts_with('/')
            && encoded.len() > 1
            && encoded[1..].bytes().all(|c| c.is_ascii_digit())
        {
            let offset: usize = encoded[1..].parse()?;
            let bytes = table.get(offset..).context("archive_gnu_name_range")?;
            let n = bytes
                .windows(2)
                .position(|b| b == b"/\n")
                .context("archive_gnu_name_end")?;
            (
                std::str::from_utf8(&bytes[..n])?.to_owned(),
                bytes[..n + 2].to_vec(),
                data.to_vec(),
            )
        } else {
            (
                if encoded == "/" || encoded == "/SYM64/" {
                    encoded
                } else {
                    encoded.trim_end_matches('/').to_owned()
                },
                Vec::new(),
                data.to_vec(),
            )
        };
        if matches!(
            name.as_str(),
            "/" | "/SYM64/"
                | "//"
                | "__.SYMDEF"
                | "__.SYMDEF SORTED"
                | "__.SYMDEF_64"
                | "__.SYMDEF_64 SORTED"
        ) {
            structure.push(Structure {
                ordinal,
                name,
                header: h.to_vec(),
                name_bytes,
                payload,
            });
        } else {
            ensure!(
                !name.is_empty() && !name.contains(['\0', '\n']),
                "archive_member_name_invalid"
            );
            members.push(Member {
                ordinal: members.len(),
                name,
                date: field(16, 28)?,
                uid: field(28, 34)?,
                gid: field(34, 40)?,
                mode: field(40, 48)?,
                sha256: sha256(&payload),
                bytes: payload.len(),
                payload,
                header: h.to_vec(),
                name_bytes,
                header_ordinal: ordinal,
                header_offset: pos,
            });
        }
        pos = end
            .checked_add(size % 2)
            .context("archive_offset_overflow")?;
        ensure!(pos <= raw.len(), "archive_padding_truncated");
        ordinal += 1;
        ensure!(ordinal <= 10000, "archive_member_count_limit");
    }
    Ok(Archive { members, structure })
}
pub(crate) fn is_macho(data: &[u8]) -> bool {
    data.get(..4).is_some_and(|x| {
        matches!(
            x,
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xfe, 0xed, 0xfa, 0xce]
        )
    })
}
pub(crate) fn stable(original: &[Member]) -> Vec<Member> {
    original
        .iter()
        .map(|m| {
            let mut s = m.clone();
            if is_macho(&m.payload) && m.name != "lib.rmeta" {
                s.name = format!("m{:06}-{}.o", m.ordinal, m.sha256)
            }
            s
        })
        .collect()
}
/// Exactly the original eight ordered Serialize fields, not public provenance.
pub(crate) fn canonical_digest(stable: &[Member]) -> Result<String> {
    Ok(sha256(serde_json::to_vec(stable)?))
}
fn field(out: &mut Vec<u8>, value: &str, n: usize) -> Result<()> {
    ensure!(value.len() <= n, "archive_header_field_overflow");
    out.extend(value.as_bytes());
    out.resize(out.len() + n - value.len(), b' ');
    Ok(())
}
pub(crate) fn render(members: &[Member]) -> Result<Vec<u8>> {
    let mut out = b"!<arch>\n".to_vec();
    for m in members {
        let namelen = (m.name.len() + 7) & !7;
        let size = namelen + m.payload.len();
        field(&mut out, &format!("#1/{namelen}"), 16)?;
        field(&mut out, &m.date, 12)?;
        field(&mut out, &m.uid, 6)?;
        field(&mut out, &m.gid, 6)?;
        field(&mut out, &m.mode, 8)?;
        field(&mut out, &size.to_string(), 10)?;
        out.extend(b"`\n");
        out.extend(m.name.as_bytes());
        out.resize(out.len() + namelen - m.name.len(), 0);
        out.extend(&m.payload);
        if size % 2 == 1 {
            out.push(b'\n')
        }
    }
    Ok(out)
}
pub(crate) fn equivalent(a: &[Member], b: &[Member]) -> Result<()> {
    ensure!(a.len() == b.len(), "archive_member_count_changed");
    for (x, y) in a.iter().zip(b) {
        ensure!(
            x.ordinal == y.ordinal
                && x.name == y.name
                && x.date == y.date
                && x.uid == y.uid
                && x.gid == y.gid
                && x.mode == y.mode
                && x.payload == y.payload,
            "archive_member_order_name_attrs_payload_changed"
        );
    }
    Ok(())
}
fn u32le(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .context("archive_symbol_u32_truncated")?
            .try_into()?,
    ))
}
fn u64le(b: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(p..p + 8)
            .context("archive_symbol_u64_truncated")?
            .try_into()?,
    ))
}
fn cstring(b: &[u8], p: usize) -> Result<Vec<u8>> {
    let s = b.get(p..).context("archive_symbol_name_offset")?;
    let n = s
        .iter()
        .position(|c| *c == 0)
        .context("archive_symbol_name_unterminated")?;
    Ok(s[..n].to_vec())
}
/// Finite actual arm64 MachO64 little-endian symbol source. Unknown object source is never attested.
fn defined_symbols(raw: &[u8]) -> Result<Vec<Vec<u8>>> {
    if !is_macho(raw) {
        return Ok(Vec::new());
    }
    ensure!(
        raw.get(..4) == Some(&[0xcf, 0xfa, 0xed, 0xfe]),
        "archive_symbol_object_format_uncovered"
    );
    let ncmds = u32le(raw, 16)? as usize;
    ensure!(ncmds <= 4096, "archive_load_command_limit");
    let sizeofcmds = u32le(raw, 20)? as usize;
    let cmds_end = 32usize
        .checked_add(sizeofcmds)
        .context("archive_load_overflow")?;
    ensure!(cmds_end <= raw.len(), "archive_load_commands_truncated");
    let mut pos = 32;
    let mut result = Vec::new();
    for _ in 0..ncmds {
        let cmd = u32le(raw, pos)?;
        let size = u32le(raw, pos + 4)? as usize;
        ensure!(
            size >= 8 && pos.checked_add(size).is_some_and(|end| end <= cmds_end),
            "archive_load_command_bounds"
        );
        if cmd == 2 {
            ensure!(size >= 24, "archive_symtab_command_truncated");
            let symoff = u32le(raw, pos + 8)? as usize;
            let nsyms = u32le(raw, pos + 12)? as usize;
            let stroff = u32le(raw, pos + 16)? as usize;
            let strsize = u32le(raw, pos + 20)? as usize;
            let strings = raw
                .get(
                    stroff
                        ..stroff
                            .checked_add(strsize)
                            .context("archive_strings_overflow")?,
                )
                .context("archive_strings_truncated")?;
            ensure!(nsyms <= 1000000, "archive_symbol_count_limit");
            for i in 0..nsyms {
                let off = symoff
                    .checked_add(i.checked_mul(16).context("archive_nlist_overflow")?)
                    .context("archive_nlist_overflow")?;
                let n = raw.get(off..off + 16).context("archive_nlist_truncated")?;
                let ty = n[4];
                let value = u64le(n, 8)?;
                if ty & 0xe0 != 0 || ty & 1 == 0 || (ty & 0x0e == 0 && value == 0) {
                    continue;
                }
                let name = cstring(strings, u32le(n, 0)? as usize)?;
                if !name.is_empty() {
                    result.push(name)
                }
            }
        }
        pos += size;
    }
    Ok(result)
}
/// Compare the rebuilt native index to actual member symbol names/offsets and same-symbol occurrence order.
pub(crate) fn verify_index(a: &Archive) -> Result<()> {
    let indexes = a
        .structure
        .iter()
        .filter(|s| s.name.starts_with("__.SYMDEF"))
        .collect::<Vec<_>>();
    ensure!(indexes.len() == 1, "archive_rebuilt_index_inventory");
    let index = indexes[0];
    let wide = index.name.contains("_64");
    let width = if wide { 8 } else { 4 };
    let get = |p| -> Result<usize> {
        Ok(if wide {
            usize::try_from(u64le(&index.payload, p)?)?
        } else {
            u32le(&index.payload, p)? as usize
        })
    };
    let bytes = get(0)?;
    ensure!(
        bytes % (width * 2) == 0 && bytes <= 32 * 1024 * 1024,
        "archive_index_records_bounds"
    );
    let end = width + bytes;
    let strings_size = get(end)?;
    let start = end
        .checked_add(width)
        .context("archive_index_strings_overflow")?;
    let finish = start
        .checked_add(strings_size)
        .context("archive_index_strings_overflow")?;
    let strings = index
        .payload
        .get(start..finish)
        .context("archive_index_strings_truncated")?;
    let mut actual = Vec::new();
    for pos in (width..end).step_by(width * 2) {
        actual.push((cstring(strings, get(pos)?)?, get(pos + width)?));
    }
    let mut expected = Vec::new();
    for m in &a.members {
        for name in defined_symbols(&m.payload)? {
            expected.push((name, m.header_offset));
        }
    }
    verify_symbol_index(&actual, &expected)
}

fn verify_symbol_index(actual: &[(Vec<u8>, usize)], expected: &[(Vec<u8>, usize)]) -> Result<()> {
    let mut aset = actual.to_vec();
    let mut eset = expected.to_vec();
    aset.sort();
    eset.sort();
    ensure!(aset == eset, "archive_index_symbol_member_offset_mismatch");
    fn groups(items: &[(Vec<u8>, usize)]) -> std::collections::BTreeMap<&[u8], Vec<usize>> {
        let mut grouped = std::collections::BTreeMap::<&[u8], Vec<usize>>::new();
        for (name, offset) in items {
            grouped.entry(name.as_slice()).or_default().push(*offset);
        }
        grouped
    }
    // Preserve encounter order inside each symbol, including repeated offsets.
    // The original multiset check above still has precedence over order failure.
    ensure!(
        groups(actual) == groups(expected),
        "archive_index_duplicate_symbol_order_changed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexed_symbol_order_preserves_original_outcomes() {
        fn original(actual: &[(Vec<u8>, usize)], expected: &[(Vec<u8>, usize)]) -> Result<()> {
            let mut aset = actual.to_vec();
            let mut eset = expected.to_vec();
            aset.sort();
            eset.sort();
            ensure!(aset == eset, "archive_index_symbol_member_offset_mismatch");
            let names = expected
                .iter()
                .map(|x| x.0.clone())
                .collect::<std::collections::BTreeSet<_>>();
            for name in names {
                let aa = actual
                    .iter()
                    .filter(|x| x.0 == name)
                    .map(|x| x.1)
                    .collect::<Vec<_>>();
                let ee = expected
                    .iter()
                    .filter(|x| x.0 == name)
                    .map(|x| x.1)
                    .collect::<Vec<_>>();
                ensure!(aa == ee, "archive_index_duplicate_symbol_order_changed");
            }
            Ok(())
        }
        let alphabet = [(vec![0], 0), (vec![0], 8), (vec![255], 0), (vec![255], 8)];
        let mut sequences = vec![vec![]];
        let mut level = vec![vec![]];
        for _ in 0..3 {
            let mut next = Vec::new();
            for prefix in &level {
                for item in &alphabet {
                    let mut value = prefix.clone();
                    value.push(item.clone());
                    next.push(value);
                }
            }
            sequences.extend(next.clone());
            level = next;
        }
        assert_eq!(sequences.len(), 85);
        for actual in &sequences {
            for expected in &sequences {
                assert_eq!(
                    verify_symbol_index(actual, expected).map_err(|e| e.to_string()),
                    original(actual, expected).map_err(|e| e.to_string())
                );
            }
        }
    }

    #[test]
    fn indexed_symbol_order_retains_duplicate_order_and_multiset_error_precedence() {
        let expected = vec![
            (b"a".to_vec(), 1),
            (b"b".to_vec(), 5),
            (b"a".to_vec(), 2),
            (b"a".to_vec(), 1),
        ];
        let regrouped = vec![
            (b"b".to_vec(), 5),
            (b"a".to_vec(), 1),
            (b"a".to_vec(), 2),
            (b"a".to_vec(), 1),
        ];
        verify_symbol_index(&regrouped, &expected).unwrap();
        let mut reversed = expected.clone();
        reversed.swap(0, 2);
        assert_eq!(
            verify_symbol_index(&reversed, &expected)
                .unwrap_err()
                .to_string(),
            "archive_index_duplicate_symbol_order_changed"
        );
        reversed[1].1 = 6;
        assert_eq!(
            verify_symbol_index(&reversed, &expected)
                .unwrap_err()
                .to_string(),
            "archive_index_symbol_member_offset_mismatch"
        );
    }

    #[test]
    fn indexed_symbol_order_reads_each_current_input() {
        let expected = (0..20000)
            .map(|n| (format!("symbol_{n}").into_bytes(), n))
            .collect::<Vec<_>>();
        verify_symbol_index(&expected, &expected).unwrap();
        let mut changed = expected.clone();
        changed[19000].1 += 1;
        assert!(verify_symbol_index(&changed, &expected).is_err());
        verify_symbol_index(&expected, &expected).unwrap();
    }
    #[test]
    fn canonical_identity_retains_original_eight_field_order_excludes_provenance() {
        let payload = vec![0xcf, 0xfa, 0xed, 0xfe];
        let digest = sha256(&payload);
        let original = Member {
            ordinal: 0,
            name: "first-random.rcgu.o".into(),
            date: "123".into(),
            uid: "".into(),
            gid: "".into(),
            mode: "644".into(),
            sha256: digest.clone(),
            bytes: 4,
            payload,
            header: vec![1; 60],
            name_bytes: b"original padded name".to_vec(),
            header_ordinal: 3,
            header_offset: 999,
        };
        let s = stable(&[original.clone()]);
        let expected = format!(
            "[{{\"ordinal\":0,\"name\":\"m000000-{digest}.o\",\"date\":\"123\",\"uid\":\"\",\"gid\":\"\",\"mode\":\"644\",\"sha256\":\"{digest}\",\"bytes\":4}}]"
        );
        assert_eq!(serde_json::to_vec(&s).unwrap(), expected.as_bytes());
        let mut changed = original;
        changed.name = "second-random.rcgu.o".into();
        changed.header_ordinal = 0;
        changed.header_offset = 8;
        changed.header = vec![2; 60];
        changed.name_bytes = b"different physical name".to_vec();
        assert_eq!(
            canonical_digest(&s).unwrap(),
            canonical_digest(&stable(&[changed])).unwrap()
        );
    }
    #[test]
    fn truncated_wide_index_size_refuses_without_overflow() {
        let archive = Archive {
            members: vec![],
            structure: vec![Structure {
                ordinal: 0,
                name: "__.SYMDEF_64".into(),
                header: vec![],
                name_bytes: vec![],
                payload: [0u64.to_le_bytes(), u64::MAX.to_le_bytes()].concat(),
            }],
        };
        assert!(verify_index(&archive).is_err());
    }
}
