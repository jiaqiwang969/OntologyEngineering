//! The part of numpy's legacy `np.random.RandomState` the embeddings module uses.
//!
//! `TextEmbedder._embed_fallback` seeds `RandomState(seed)` with a 32-bit integer and draws
//! `rand(dim)`; `LlamaStore.embed` draws `np.random.normal(0, 0.1, 768)` from the global
//! state. This is numpy's MT19937 with its legacy integer seeding (`mt19937_seed`), the 53-bit
//! `random_sample` double and the legacy polar Box-Muller Gaussian, so the same seed gives the
//! same numbers bit for bit.

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// `np.random.RandomState` (MT19937 bit generator, legacy distributions).
#[derive(Clone)]
pub(crate) struct RandomState {
    key: [u32; N],
    pos: usize,
    gauss: Option<f64>,
}

impl RandomState {
    /// `RandomState(seed)` for an integer seed in `0..2**32` (`mt19937_seed`).
    pub(crate) fn new(seed: u32) -> RandomState {
        let mut key = [0u32; N];
        let mut s = seed;
        for (pos, slot) in key.iter_mut().enumerate() {
            *slot = s;
            s = 1_812_433_253u32
                .wrapping_mul(s ^ (s >> 30))
                .wrapping_add(pos as u32 + 1);
        }
        RandomState {
            key,
            pos: N,
            gauss: None,
        }
    }

    /// A state seeded from the clock and the process id (numpy seeds its global state from
    /// OS entropy).
    pub(crate) fn from_entropy() -> RandomState {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mixed = (nanos as u64) ^ ((nanos >> 64) as u64) ^ ((std::process::id() as u64) << 32);
        RandomState::new((mixed ^ (mixed >> 32)) as u32)
    }

    fn generate(&mut self) {
        let key = &mut self.key;
        let mut i = 0;
        while i < N - M {
            let y = (key[i] & UPPER_MASK) | (key[i + 1] & LOWER_MASK);
            key[i] = key[i + M] ^ (y >> 1) ^ (0u32.wrapping_sub(y & 1) & MATRIX_A);
            i += 1;
        }
        while i < N - 1 {
            let y = (key[i] & UPPER_MASK) | (key[i + 1] & LOWER_MASK);
            key[i] = key[i + M - N] ^ (y >> 1) ^ (0u32.wrapping_sub(y & 1) & MATRIX_A);
            i += 1;
        }
        let y = (key[N - 1] & UPPER_MASK) | (key[0] & LOWER_MASK);
        key[N - 1] = key[M - 1] ^ (y >> 1) ^ (0u32.wrapping_sub(y & 1) & MATRIX_A);
        self.pos = 0;
    }

    fn next_u32(&mut self) -> u32 {
        if self.pos == N {
            self.generate();
        }
        let mut y = self.key[self.pos];
        self.pos += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `random_sample()`: a double in `[0, 1)` with 53 random bits.
    pub(crate) fn random_sample(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67_108_864.0 + b) / 9_007_199_254_740_992.0
    }

    /// `rand(n)`.
    pub(crate) fn rand(&mut self, n: usize) -> Vec<f64> {
        (0..n).map(|_| self.random_sample()).collect()
    }

    /// `legacy_gauss`: the polar Box-Muller method, caching the second value.
    pub(crate) fn gauss(&mut self) -> f64 {
        if let Some(value) = self.gauss.take() {
            return value;
        }
        loop {
            let x1 = 2.0 * self.random_sample() - 1.0;
            let x2 = 2.0 * self.random_sample() - 1.0;
            let r2 = x1 * x1 + x2 * x2;
            if r2 < 1.0 && r2 != 0.0 {
                let f = (-2.0 * r2.ln() / r2).sqrt();
                self.gauss = Some(f * x1);
                return f * x2;
            }
        }
    }

    /// `normal(loc, scale, size)`.
    pub(crate) fn normal(&mut self, loc: f64, scale: f64, size: usize) -> Vec<f64> {
        (0..size).map(|_| loc + scale * self.gauss()).collect()
    }
}
