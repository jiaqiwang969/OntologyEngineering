//! Decompressors the ingest port needs and the allowed dependencies do not expose:
//! DEFLATE (RFC 1951) with its gzip (RFC 1952) and zlib (RFC 1950) framings, Snappy raw blocks
//! (Parquet's `SNAPPY` codec) and LZ4 frames (Arrow IPC / Feather `LZ4_FRAME` buffers).

/// Inflates a raw DEFLATE stream.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut inflater = Inflater::new(data);
    inflater.run()?;
    Ok(inflater.out)
}

/// Inflates a zlib stream, falling back to raw DEFLATE (what browsers and requests accept
/// for `Content-Encoding: deflate`).
pub fn inflate_zlib_or_raw(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() >= 2 && (data[0] & 0x0f) == 8 && (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 == 0 {
        let mut inflater = Inflater::new(&data[2..]);
        if inflater.run().is_ok() {
            return Ok(inflater.out);
        }
    }
    inflate(data)
}

/// Decompresses a gzip file/stream (concatenated members are joined).
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        if data.len() - pos < 18 || data[pos] != 0x1f || data[pos + 1] != 0x8b {
            if pos == 0 {
                return Err("Not a gzipped file".to_string());
            }
            break;
        }
        if data[pos + 2] != 8 {
            return Err("Unknown compression method".to_string());
        }
        let flags = data[pos + 3];
        let mut p = pos + 10;
        if flags & 0x04 != 0 {
            if p + 2 > data.len() {
                return Err("truncated gzip header".to_string());
            }
            let xlen = usize::from(data[p]) | usize::from(data[p + 1]) << 8;
            p += 2 + xlen;
        }
        if flags & 0x08 != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
        if flags & 0x10 != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
        if flags & 0x02 != 0 {
            p += 2;
        }
        if p > data.len() {
            return Err("truncated gzip header".to_string());
        }
        let mut inflater = Inflater::new(&data[p..]);
        inflater.run()?;
        let consumed = inflater.consumed_bytes();
        out.extend_from_slice(&inflater.out);
        pos = p + consumed + 8;
    }
    Ok(out)
}

struct Inflater<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u64,
    bit_count: u32,
    out: Vec<u8>,
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman, String> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

impl<'a> Inflater<'a> {
    fn new(data: &'a [u8]) -> Self {
        Inflater {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
            out: Vec::new(),
        }
    }

    /// Bytes of input consumed (partial bytes count as consumed).
    fn consumed_bytes(&self) -> usize {
        self.pos - (self.bit_count / 8) as usize
    }

    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.bit_count < n {
            if self.pos >= self.data.len() {
                return Err("Error -3 while decompressing data: incomplete or truncated stream".to_string());
            }
            self.bit_buf |= u64::from(self.data[self.pos]) << self.bit_count;
            self.pos += 1;
            self.bit_count += 8;
        }
        Ok(())
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = (self.bit_buf & ((1u64 << n) - 1)) as u32;
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(v)
    }

    fn decode(&mut self, h: &Huffman) -> Result<u16, String> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= self.bits(1)? as i32;
            let count = i32::from(h.counts[len]);
            if code - count < first {
                return Ok(h.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("Error -3 while decompressing data: invalid code".to_string())
    }

    fn run(&mut self) -> Result<(), String> {
        loop {
            let last = self.bits(1)?;
            let kind = self.bits(2)?;
            match kind {
                0 => self.stored()?,
                1 => {
                    let mut lengths = [0u8; 288];
                    for (i, l) in lengths.iter_mut().enumerate() {
                        *l = match i {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    let lit = Huffman::new(&lengths)?;
                    let dist = Huffman::new(&[5u8; 30])?;
                    self.codes(&lit, &dist)?;
                }
                2 => {
                    let (lit, dist) = self.dynamic_tables()?;
                    self.codes(&lit, &dist)?;
                }
                _ => return Err("Error -3 while decompressing data: invalid block type".to_string()),
            }
            if last == 1 {
                return Ok(());
            }
        }
    }

    fn stored(&mut self) -> Result<(), String> {
        // Discard to byte boundary.
        let drop = self.bit_count % 8;
        self.bits(drop)?;
        let len = self.bits(16)? as usize;
        let nlen = self.bits(16)? as usize;
        if len != (!nlen & 0xffff) {
            return Err("Error -3 while decompressing data: invalid stored block lengths".to_string());
        }
        // Whole bytes may remain in the bit buffer.
        for _ in 0..len {
            let b = self.bits(8)? as u8;
            self.out.push(b);
        }
        Ok(())
    }

    fn dynamic_tables(&mut self) -> Result<(Huffman, Huffman), String> {
        let nlen = self.bits(5)? as usize + 257;
        let ndist = self.bits(5)? as usize + 1;
        let ncode = self.bits(4)? as usize + 4;
        const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
        let mut lengths = [0u8; 19];
        for &idx in ORDER.iter().take(ncode) {
            lengths[idx] = self.bits(3)? as u8;
        }
        let lencode = Huffman::new(&lengths)?;
        let mut all = vec![0u8; nlen + ndist];
        let mut index = 0;
        while index < nlen + ndist {
            let symbol = self.decode(&lencode)?;
            if symbol < 16 {
                all[index] = symbol as u8;
                index += 1;
            } else {
                let (value, repeat) = match symbol {
                    16 => {
                        if index == 0 {
                            return Err("Error -3 while decompressing data: invalid bit length repeat".to_string());
                        }
                        (all[index - 1], 3 + self.bits(2)? as usize)
                    }
                    17 => (0, 3 + self.bits(3)? as usize),
                    _ => (0, 11 + self.bits(7)? as usize),
                };
                if index + repeat > nlen + ndist {
                    return Err("Error -3 while decompressing data: invalid bit length repeat".to_string());
                }
                for _ in 0..repeat {
                    all[index] = value;
                    index += 1;
                }
            }
        }
        let lit = Huffman::new(&all[..nlen])?;
        let dist = Huffman::new(&all[nlen..])?;
        Ok((lit, dist))
    }

    fn codes(&mut self, lit: &Huffman, dist: &Huffman) -> Result<(), String> {
        loop {
            let symbol = self.decode(lit)?;
            if symbol < 256 {
                self.out.push(symbol as u8);
            } else if symbol == 256 {
                return Ok(());
            } else {
                let s = (symbol - 257) as usize;
                if s >= 29 {
                    return Err("Error -3 while decompressing data: invalid literal/length code".to_string());
                }
                let len = LEN_BASE[s] as usize + self.bits(u32::from(LEN_EXTRA[s]))? as usize;
                let ds = self.decode(dist)? as usize;
                if ds >= 30 {
                    return Err("Error -3 while decompressing data: invalid distance code".to_string());
                }
                let d = DIST_BASE[ds] as usize + self.bits(u32::from(DIST_EXTRA[ds]))? as usize;
                if d > self.out.len() {
                    return Err("Error -3 while decompressing data: invalid distance too far back".to_string());
                }
                let start = self.out.len() - d;
                for i in 0..len {
                    let b = self.out[start + i];
                    self.out.push(b);
                }
            }
        }
    }
}

/// Decompresses a raw Snappy block (the format Parquet's `SNAPPY` codec uses).
pub fn snappy_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut pos = 0usize;
    // Preamble: uncompressed length as a varint.
    let mut len: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *data.get(pos).ok_or("snappy: truncated length")?;
        pos += 1;
        len |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 63 {
            return Err("snappy: invalid length".to_string());
        }
    }
    let mut out: Vec<u8> = Vec::with_capacity(len as usize);
    while pos < data.len() {
        let tag = data[pos];
        pos += 1;
        match tag & 0x03 {
            0 => {
                let mut l = (tag >> 2) as usize;
                if l >= 60 {
                    let extra = l - 59;
                    if pos + extra > data.len() {
                        return Err("snappy: truncated literal".to_string());
                    }
                    l = 0;
                    for i in 0..extra {
                        l |= (data[pos + i] as usize) << (8 * i);
                    }
                    pos += extra;
                }
                l += 1;
                if pos + l > data.len() {
                    return Err("snappy: truncated literal".to_string());
                }
                out.extend_from_slice(&data[pos..pos + l]);
                pos += l;
            }
            kind => {
                let (length, offset) = match kind {
                    1 => {
                        let b = *data.get(pos).ok_or("snappy: truncated copy")?;
                        pos += 1;
                        (
                            (((tag >> 2) & 0x07) as usize) + 4,
                            ((tag as usize >> 5) << 8) | b as usize,
                        )
                    }
                    2 => {
                        if pos + 2 > data.len() {
                            return Err("snappy: truncated copy".to_string());
                        }
                        let o = data[pos] as usize | (data[pos + 1] as usize) << 8;
                        pos += 2;
                        (((tag >> 2) as usize) + 1, o)
                    }
                    _ => {
                        if pos + 4 > data.len() {
                            return Err("snappy: truncated copy".to_string());
                        }
                        let o = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                        pos += 4;
                        (((tag >> 2) as usize) + 1, o)
                    }
                };
                if offset == 0 || offset > out.len() {
                    return Err("snappy: invalid copy offset".to_string());
                }
                let start = out.len() - offset;
                for i in 0..length {
                    let b = out[start + i];
                    out.push(b);
                }
            }
        }
    }
    if out.len() as u64 != len {
        return Err("snappy: length mismatch".to_string());
    }
    Ok(out)
}

/// Decompresses an LZ4 block into `out` (appending).
pub fn lz4_block_decompress(data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let mut pos = 0usize;
    loop {
        let token = *data.get(pos).ok_or("lz4: truncated block")?;
        pos += 1;
        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                let b = *data.get(pos).ok_or("lz4: truncated literal length")?;
                pos += 1;
                lit_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        if pos + lit_len > data.len() {
            return Err("lz4: truncated literals".to_string());
        }
        out.extend_from_slice(&data[pos..pos + lit_len]);
        pos += lit_len;
        if pos >= data.len() {
            return Ok(());
        }
        if pos + 2 > data.len() {
            return Err("lz4: truncated offset".to_string());
        }
        let offset = data[pos] as usize | (data[pos + 1] as usize) << 8;
        pos += 2;
        let mut match_len = (token & 0x0f) as usize;
        if match_len == 15 {
            loop {
                let b = *data.get(pos).ok_or("lz4: truncated match length")?;
                pos += 1;
                match_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        match_len += 4;
        if offset == 0 || offset > out.len() {
            return Err("lz4: invalid offset".to_string());
        }
        let start = out.len() - offset;
        for i in 0..match_len {
            let b = out[start + i];
            out.push(b);
        }
    }
}

/// Decompresses an LZ4 frame (magic `04 22 4D 18`), possibly several concatenated frames.
pub fn lz4_frame_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        if data.len() - pos < 7 {
            return Err("lz4: truncated frame header".to_string());
        }
        let magic = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        if (0x184D2A50..=0x184D2A5F).contains(&magic) {
            // Skippable frame.
            let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
            pos += 8 + size;
            continue;
        }
        if magic != 0x184D2204 {
            return Err("lz4: bad frame magic".to_string());
        }
        pos += 4;
        let flg = data[pos];
        let _bd = data[pos + 1];
        pos += 2;
        let block_checksum = flg & 0x10 != 0;
        let content_size = flg & 0x08 != 0;
        let content_checksum = flg & 0x04 != 0;
        let dict_id = flg & 0x01 != 0;
        if content_size {
            pos += 8;
        }
        if dict_id {
            pos += 4;
        }
        pos += 1; // header checksum
        loop {
            if pos + 4 > data.len() {
                return Err("lz4: truncated block size".to_string());
            }
            let raw = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;
            if raw == 0 {
                break;
            }
            let uncompressed = raw & 0x8000_0000 != 0;
            let size = (raw & 0x7fff_ffff) as usize;
            if pos + size > data.len() {
                return Err("lz4: truncated block".to_string());
            }
            let block = &data[pos..pos + size];
            pos += size;
            if uncompressed {
                out.extend_from_slice(block);
            } else {
                lz4_block_decompress(block, &mut out)?;
            }
            if block_checksum {
                pos += 4;
            }
        }
        if content_checksum {
            pos += 4;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_fixed_blocks() {
        // zlib.compress(b"hello hello hello", 9)[2:-4] (raw deflate, fixed Huffman).
        let raw = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00];
        assert_eq!(inflate(&raw).unwrap(), b"hello hello hello");
        // Stored block: BFINAL=1, BTYPE=00, LEN=3, NLEN=~3, "abc".
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&stored).unwrap(), b"abc");
    }

    #[test]
    fn snappy_literal_and_copy() {
        // Uncompressed length 8, literal "ab" (tag 0x04), copy len 6 offset 2 (tag 0x09, off 2).
        let data = [0x08, 0x04, b'a', b'b', 0x09, 0x02];
        assert_eq!(snappy_decompress(&data).unwrap(), b"abababab");
    }
}
