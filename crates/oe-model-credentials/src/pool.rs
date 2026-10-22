//! Shared private-file credentials for API adapters. No HTTP or hidden retries.
use crate::{CredentialFileError, credential_file::Credential};
use std::{fmt, path::Path, sync::Mutex};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PoolError {
    #[error("credential_pool_exhausted")]
    Exhausted,
    #[error("credential_pool_state_unavailable")]
    StateUnavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStatus {
    pub total: usize,
    pub available: usize,
    pub unavailable: usize,
}

struct State {
    cursor: usize,
    unavailable: Vec<bool>,
}
pub struct CredentialPool {
    credential: Credential,
    state: Mutex<State>,
}
impl fmt::Debug for CredentialPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialPool([REDACTED])")
    }
}
impl CredentialPool {
    pub fn is_openai_single(&self) -> bool {
        self.credential.is_openai_single()
    }
    /// Exactly one OpenAI credential. Reuse the same secure file/lease owner;
    /// do not rotate accounts or inspect legacy gateway sidecars.
    pub fn read_openai_bearer(path: impl AsRef<Path>) -> Result<Self, CredentialFileError> {
        let credential = Credential::read_openai_bearer(path.as_ref())?;
        Ok(Self {
            credential,
            state: Mutex::new(State {
                cursor: 0,
                unavailable: vec![false],
            }),
        })
    }
    /// Owner-selected single bearer file. No automatic alternate credentials.
    pub fn read_single_http_bearer(path: impl AsRef<Path>) -> Result<Self, CredentialFileError> {
        let credential = Credential::read_single_http_bearer(path.as_ref())?;
        let state = Mutex::new(State {
            cursor: 0,
            unavailable: vec![false; credential.len()],
        });
        Ok(Self { credential, state })
    }
    /// Opaque generation for in-process reuse. Never key material.
    pub fn generation(&self) -> &str {
        self.credential.generation()
    }
    pub fn contains_in(&self, bytes: &[u8]) -> bool {
        self.credential.contains_in(bytes)
    }
    pub fn lease(&self) -> Result<CredentialLease<'_>, PoolError> {
        let mut state = self.state.lock().map_err(|_| PoolError::StateUnavailable)?;
        for step in 0..self.credential.len() {
            let index = (state.cursor + step) % self.credential.len();
            if !state.unavailable[index] {
                state.cursor = (index + 1) % self.credential.len();
                return Ok(CredentialLease { pool: self, index });
            }
        }
        Err(PoolError::Exhausted)
    }
    pub fn unavailable(&self) -> bool {
        self.state
            .lock()
            .map_or(true, |s| s.unavailable.iter().all(|v| *v))
    }
    pub fn status(&self) -> Result<PoolStatus, PoolError> {
        let state = self.state.lock().map_err(|_| PoolError::StateUnavailable)?;
        let unavailable = state.unavailable.iter().filter(|v| **v).count();
        Ok(PoolStatus {
            total: self.credential.len(),
            unavailable,
            available: self.credential.len() - unavailable,
        })
    }
}
pub struct CredentialLease<'a> {
    pool: &'a CredentialPool,
    index: usize,
}
impl fmt::Debug for CredentialLease<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialLease([REDACTED])")
    }
}
impl CredentialLease<'_> {
    /// Confidential HTTP authorization value; never log or persist this string.
    pub fn bearer(&self) -> String {
        self.pool.credential.bearer(self.index)
    }
    /// Confirmed account failure only. Another attempt remains caller-budgeted.
    pub fn retire(&self) -> Result<bool, PoolError> {
        let mut state = self
            .pool
            .state
            .lock()
            .map_err(|_| PoolError::StateUnavailable)?;
        state.unavailable[self.index] = true;
        Ok(state.unavailable.iter().any(|v| !v))
    }
}
