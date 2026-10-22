//! pdfminer's `TrueTypeFont.create_unicode_map`: invert an embedded TrueType font's Unicode
//! `cmap` subtables (formats 0, 2 and 4) into a glyph id -> character map, used for
//! `Adobe-Identity` CID fonts without a ToUnicode CMap.

use std::collections::HashMap;

fn u16_at(data: &[u8], pos: usize) -> Option<u16> {
    data.get(pos..pos + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn i16_at(data: &[u8], pos: usize) -> Option<i16> {
    data.get(pos..pos + 2).map(|b| i16::from_be_bytes([b[0], b[1]]))
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Glyph id -> Unicode text, or `None` when the font has no usable `cmap` table
/// (pdfminer's `CMapNotFound`). Unsupported subtable formats are skipped (pdfminer
/// asserts on them).
pub fn unicode_map(data: &[u8]) -> Option<HashMap<u32, String>> {
    let ntables = u16_at(data, 4)? as usize;
    let mut cmap: Option<(usize, usize)> = None;
    for i in 0..ntables {
        let rec = 12 + 16 * i;
        let Some(tag) = data.get(rec..rec + 4) else { break };
        let (Some(offset), Some(length)) = (u32_at(data, rec + 8), u32_at(data, rec + 12)) else {
            break;
        };
        if tag == b"cmap" {
            cmap = Some((offset as usize, length as usize));
        }
    }
    let (base, _len) = cmap?;
    let nsub = u16_at(data, base + 2)? as usize;
    let mut char2gid: Vec<(u32, u32)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    let mut put = |c: u32, g: u32, v: &mut Vec<(u32, u32)>| match seen.get(&c) {
        Some(&i) => v[i].1 = g,
        None => {
            seen.insert(c, v.len());
            v.push((c, g));
        }
    };
    for i in 0..nsub {
        let rec = base + 4 + 8 * i;
        let (Some(platform), Some(encoding), Some(st_offset)) =
            (u16_at(data, rec), u16_at(data, rec + 2), u32_at(data, rec + 4))
        else {
            break;
        };
        if !(platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10))) {
            continue;
        }
        let st = base + st_offset as usize;
        let Some(fmt) = u16_at(data, st) else { continue };
        let body = st + 6;
        match fmt {
            0 => {
                for c in 0..256usize {
                    if let Some(&g) = data.get(body + c) {
                        put(c as u32, g as u32, &mut char2gid);
                    }
                }
            }
            2 => {
                let keys: Vec<u16> = (0..256).filter_map(|k| u16_at(data, body + 2 * k)).collect();
                if keys.len() != 256 {
                    continue;
                }
                let mut firstbytes = vec![0usize; 8192];
                for (k, &key) in keys.iter().enumerate() {
                    firstbytes[(key / 8) as usize] = k;
                }
                let nhdrs = keys.iter().copied().max().unwrap_or(0) as usize / 8 + 1;
                let hdr_base = body + 512;
                for h in 0..nhdrs {
                    let pos = hdr_base + 8 * h;
                    let (Some(firstcode), Some(entcount), Some(delta), Some(offset)) = (
                        u16_at(data, pos),
                        u16_at(data, pos + 2),
                        i16_at(data, pos + 4),
                        u16_at(data, pos + 6),
                    ) else {
                        break;
                    };
                    if entcount == 0 {
                        continue;
                    }
                    let first = firstcode as u32 + ((firstbytes[h] as u32) << 8);
                    let glyphs = pos + 8 - 2 + offset as usize;
                    for c in 0..entcount as usize {
                        let Some(mut gid) = u16_at(data, glyphs + 2 * c).map(|g| g as i64) else { break };
                        if gid != 0 {
                            gid += delta as i64;
                        }
                        put(first + c as u32, gid as u32, &mut char2gid);
                    }
                }
            }
            4 => {
                let Some(segcount2) = u16_at(data, body) else { continue };
                let segcount = segcount2 as usize / 2;
                let ecs_at = body + 8;
                let scs_at = ecs_at + 2 * segcount + 2;
                let idd_at = scs_at + 2 * segcount;
                let idr_at = idd_at + 2 * segcount;
                for s in 0..segcount {
                    let (Some(ec), Some(sc), Some(idd), Some(idr)) = (
                        u16_at(data, ecs_at + 2 * s),
                        u16_at(data, scs_at + 2 * s),
                        i16_at(data, idd_at + 2 * s),
                        u16_at(data, idr_at + 2 * s),
                    ) else {
                        break;
                    };
                    if ec < sc {
                        continue;
                    }
                    if idr != 0 {
                        // pdfminer seeks to `pos + idr` (without the segment's own offset).
                        let mut p = idr_at + idr as usize;
                        for c in sc as u32..=ec as u32 {
                            let Some(b) = u16_at(data, p) else { break };
                            p += 2;
                            put(c, ((b as i32 + idd as i32) & 0xFFFF) as u32, &mut char2gid);
                        }
                    } else {
                        for c in sc as u32..=ec as u32 {
                            put(c, ((c as i32 + idd as i32) & 0xFFFF) as u32, &mut char2gid);
                        }
                    }
                }
            }
            _ => continue,
        }
    }
    if char2gid.is_empty() {
        return None;
    }
    let mut out: HashMap<u32, String> = HashMap::new();
    for (c, gid) in char2gid {
        if let Some(ch) = char::from_u32(c) {
            let text = ch.to_string();
            if text == "\u{a0}" && out.get(&gid).map(String::as_str) == Some(" ") {
                continue;
            }
            out.insert(gid, text);
        }
    }
    Some(out)
}
