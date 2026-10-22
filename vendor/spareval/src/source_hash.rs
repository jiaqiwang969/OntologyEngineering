//! CPython 3.13.11 64-bit little-endian source identity hashing.
//!
//! Python Software Foundation copyright and license: ../LICENSE-CPYTHON.
//! SipHash adaptation copyright (c) 2013 Marek Majkowski; ../LICENSE-SIPHASH.
//! See ../SOURCE-HASH-SET-PROVENANCE.md. This is independent of Rust's Hash
//! trait, numeric value comparison, and the choice of session identity policy.

/// Process/session-owned secret. Deliberately has no Debug or serialization.
#[derive(Clone)]
pub(crate) struct SourceHashSecret([u8; 24]);

impl SourceHashSecret {
    /// The owner supplies entropy; this module never reads an interpreter's
    /// memory or substitutes a fixed seed for a failed entropy request.
    pub(crate) const fn from_bytes(bytes: [u8; 24]) -> Self {
        Self(bytes)
    }
}

/// CPython's PYTHONHASHSEED expansion, including its special zero seed.
pub(crate) fn secret_from_seed(seed: u32) -> SourceHashSecret {
    let mut bytes = [0; 24];
    if seed != 0 {
        let mut state = seed;
        for byte in &mut bytes {
            state = state.wrapping_mul(214013).wrapping_add(2531011);
            *byte = (state >> 16) as u8;
        }
    }
    SourceHashSecret::from_bytes(bytes)
}

pub(crate) const fn normalize_hash_word(raw: i64) -> i64 {
    if raw == -1 { -2 } else { raw }
}

/// XOR is intentionally unnormalized. FrozenDict caches this raw value and
/// applies the slot normalization on each outer hash return. Its incremental
/// partial cache after a component failure belongs to the row owner.
pub(crate) fn xor_hash_words(words: impl IntoIterator<Item = i64>) -> i64 {
    words.into_iter().fold(0, |acc, word| acc ^ word)
}

pub(crate) const fn hash_source_none() -> i64 {
    0xFCA8_6420
}

/// CPython 3.13.11's 64-bit tuplehash over already computed component hashes.
/// The caller owns component conversion/error order. Unlike other hash slots,
/// tuplehash substitutes 1546275796 when its final accumulator equals -1.
pub(crate) fn hash_source_tuple_words(words: &[i64]) -> i64 {
    const PRIME1: u64 = 11_400_714_785_074_694_791;
    const PRIME2: u64 = 14_029_467_366_897_019_727;
    const PRIME5: u64 = 2_870_177_450_012_600_261;
    let mut acc = PRIME5;
    for &word in words {
        acc = acc.wrapping_add((word as u64).wrapping_mul(PRIME2));
        acc = acc.rotate_left(31).wrapping_mul(PRIME1);
    }
    acc = acc.wrapping_add(words.len() as u64 ^ (PRIME5 ^ 3_527_539));
    if acc == u64::MAX { 1_546_275_796 } else { acc as i64 }
}

/// The address must belong to the caller's declared identity policy. A new
/// Rust address is not a recovered address of a historical Python object.
pub(crate) const fn hash_source_pointer_word(address: u64) -> i64 {
    normalize_hash_word(address.rotate_right(4) as i64)
}

/// Hash the CPython canonical Unicode representation, not UTF-8. A non-Latin1
/// BMP scalar promotes the whole string to two bytes per code point; an astral
/// scalar promotes it to four. Rust strings cannot contain lone surrogates.
pub(crate) fn hash_python_string(text: &str, secret: &SourceHashSecret) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let max = text.chars().map(u32::from).max().unwrap_or(0);
    let kind = if max <= 0xff {
        1
    } else if max <= 0xffff {
        2
    } else {
        4
    };
    let mut state = SipHash13::new(secret);
    for character in text.chars() {
        let bytes = u32::from(character).to_le_bytes();
        for &byte in &bytes[..kind] {
            state.write_byte(byte);
        }
    }
    normalize_hash_word(state.finish() as i64)
}

/// `language_lower` is already source-lowercased metadata. The owner must not
/// use the RDF datatype of a plain literal or include any cached numeric/error
/// value. Each component and the final Literal slot normalize independently.
pub(crate) fn hash_source_literal(
    lexical: &str,
    language_lower: Option<&str>,
    datatype: Option<&str>,
    secret: &SourceHashSecret,
) -> i64 {
    let mut raw = hash_python_string(lexical, secret);
    if let Some(language) = language_lower.filter(|language| !language.is_empty()) {
        raw ^= hash_python_string(language, secret);
    }
    if let Some(datatype) = datatype {
        raw ^= hash_python_string(datatype, secret);
    }
    normalize_hash_word(raw)
}

struct SipHash13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    tail: u64,
    tail_bytes: u32,
    byte_count: u64,
}

impl SipHash13 {
    fn new(secret: &SourceHashSecret) -> Self {
        let k0 = u64::from_le_bytes(secret.0[..8].try_into().unwrap());
        let k1 = u64::from_le_bytes(secret.0[8..16].try_into().unwrap());
        Self {
            v0: k0 ^ 0x736f_6d65_7073_6575,
            v1: k1 ^ 0x646f_7261_6e64_6f6d,
            v2: k0 ^ 0x6c79_6765_6e65_7261,
            v3: k1 ^ 0x7465_6462_7974_6573,
            tail: 0,
            tail_bytes: 0,
            byte_count: 0,
        }
    }

    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(13) ^ self.v0;
        self.v3 = self.v3.rotate_left(16) ^ self.v2;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(17) ^ self.v2;
        self.v3 = self.v3.rotate_left(21) ^ self.v0;
        self.v2 = self.v2.rotate_left(32);
    }

    fn compress(&mut self, word: u64) {
        self.v3 ^= word;
        self.round();
        self.v0 ^= word;
    }

    fn write_byte(&mut self, byte: u8) {
        self.tail |= u64::from(byte) << (8 * self.tail_bytes);
        self.tail_bytes += 1;
        // SipHash encodes only the low byte of total length in its final word.
        self.byte_count = self.byte_count.wrapping_add(1);
        if self.tail_bytes == 8 {
            self.compress(self.tail);
            self.tail = 0;
            self.tail_bytes = 0;
        }
    }

    fn finish(mut self) -> u64 {
        self.compress(self.tail | self.byte_count.wrapping_shl(56));
        self.v2 ^= 0xff;
        self.round();
        self.round();
        self.round();
        (self.v0 ^ self.v1) ^ (self.v2 ^ self.v3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_hash_keeps_raw_row_xor_separate_from_slot_normalization() {
        let cached = xor_hash_words([1, -2]);
        assert_eq!(cached, -1);
        assert_eq!(normalize_hash_word(cached), -2);
        assert_eq!(cached, -1);
        assert_eq!(normalize_hash_word(i64::MIN), i64::MIN);
    }

    #[test]
    fn source_hash_native_pointer_uses_own_identity_without_python_mapping() {
        let owned = Box::new(7_u8);
        let address = (&*owned as *const u8) as usize as u64;
        assert_eq!(
            hash_source_pointer_word(address),
            normalize_hash_word(address.rotate_right(4) as i64)
        );
        assert_eq!(hash_source_pointer_word(u64::MAX), -2);
        assert_eq!(hash_source_none(), 0xFCA8_6420);
    }
}
