//! Python standard-library behaviour the port relies on to keep Semantica's observable output:
//! `str()`/`repr()` formatting ([`fmt`]), `json.dumps` ([`json`]), PyYAML's `yaml.dump`
//! ([`yaml`]), `datetime` parsing and formatting ([`time`]) and the `warnings` module
//! ([`warnings`]).
//!
//! These are shared building blocks for every Semantica crate; they are not part of the
//! original `semantica.utils` API.

pub mod fmt;
pub mod json;
pub mod time;
pub mod warnings;
pub mod yaml;
