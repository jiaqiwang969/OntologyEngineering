//! Lossless, bounded operation-local material. No persistent format or source identity changes.
use anyhow::{Context, Result, ensure};
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::{borrow::Cow, io::Write};

pub(crate) const FILE_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) const CACHE_LIMIT: usize = 64 * 1024 * 1024;
// Explicit workspace /4 and task-view /5 only; legacy budgets remain unchanged.
pub(crate) const EXTENDED_CACHE_LIMIT: usize = 128 * 1024 * 1024;
pub(crate) const EXTENDED_PATH_LIMIT: usize = 2048;
// Explicit workspace /5 and task-view /6 only; /1-/4 and task-view /1-/5 keep their budgets.
pub(crate) const LARGE_EXTENDED_CACHE_LIMIT: usize = 256 * 1024 * 1024;
pub(crate) const LARGE_EXTENDED_PATH_LIMIT: usize = 4096;

pub(crate) struct Material {
    payload: Box<[u8]>,
    logical_len: usize,
    compressed: bool,
}

impl Material {
    pub(crate) fn encode(raw: &[u8], compress: bool) -> Result<Self> {
        ensure!(raw.len() <= FILE_LIMIT, "source_cache_file_limit");
        let payload = if compress && !raw.is_empty() {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(raw).context("source_cache_encode")?;
            encoder.finish().context("source_cache_encode")?
        } else {
            Vec::new()
        };
        let compressed = compress && !raw.is_empty() && payload.len() < raw.len();
        Ok(Self {
            payload: if compressed {
                payload.into_boxed_slice()
            } else {
                raw.into()
            },
            logical_len: raw.len(),
            compressed,
        })
    }

    pub(crate) fn retained_len(&self) -> usize {
        self.payload.len()
    }

    pub(crate) fn decode(&self) -> Result<Cow<'_, [u8]>> {
        ensure!(self.logical_len <= FILE_LIMIT, "source_cache_file_limit");
        if !self.compressed {
            ensure!(
                self.payload.len() == self.logical_len,
                "source_cache_length"
            );
            return Ok(Cow::Borrowed(&self.payload));
        }
        let mut decoder = Decompress::new(true);
        // The extra byte detects expansion beyond the exact claimed length.
        // StreamEnd is required even if all expected output was recovered.
        let mut raw = vec![0; self.logical_len + 1];
        let status = decoder
            .decompress(&self.payload, &mut raw, FlushDecompress::Finish)
            .context("source_cache_decode")?;
        ensure!(
            status == Status::StreamEnd,
            "source_cache_incomplete_stream"
        );
        ensure!(
            decoder.total_out() == self.logical_len as u64,
            "source_cache_length"
        );
        ensure!(
            decoder.total_in() == self.payload.len() as u64,
            "source_cache_trailing_data"
        );
        raw.truncate(self.logical_len);
        Ok(Cow::Owned(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_preserves_bytes_and_uses_raw_when_compression_is_larger() {
        for raw in [
            Vec::new(),
            vec![1],
            b"random small bytes".to_vec(),
            vec![42; FILE_LIMIT],
        ] {
            let m = Material::encode(&raw, true).unwrap();
            assert!(m.retained_len() <= raw.len());
            assert_eq!(m.decode().unwrap().as_ref(), raw);
            let legacy = Material::encode(&raw, false).unwrap();
            assert_eq!(legacy.retained_len(), raw.len());
            assert!(!legacy.compressed);
        }
        assert!(Material::encode(&vec![0; FILE_LIMIT + 1], true).is_err());
    }

    #[test]
    fn corrupt_truncated_trailing_and_forged_lengths_reject() {
        let raw = vec![17; 100_000];
        for kind in 0..6 {
            let mut m = Material::encode(&raw, true).unwrap();
            match kind {
                0 => m.payload[4] ^= 127,
                1 => m.payload = m.payload[..m.payload.len() / 2].into(),
                2 => {
                    let mut p = m.payload.to_vec();
                    p.push(0);
                    m.payload = p.into_boxed_slice();
                }
                3 => m.logical_len = 1,
                4 => m.logical_len = FILE_LIMIT + 1,
                _ => m.payload = m.payload[..m.payload.len() - 1].into(),
            }
            assert!(m.decode().is_err(), "corruption kind {kind}");
        }
    }
}
