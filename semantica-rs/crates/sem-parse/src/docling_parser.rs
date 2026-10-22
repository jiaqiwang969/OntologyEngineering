//! Docling-based parsing (`semantica/parse/docling_parser.py`). STUB.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::common::{Options, PResult};

/// `DOCLING_AVAILABLE`.
pub const DOCLING_AVAILABLE: bool = true;
/// `DOCLING_IMPORT_ERROR`.
pub const DOCLING_IMPORT_ERROR: Option<&str> = None;

/// `DoclingMetadata` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DoclingMetadata;

/// `DoclingParser`.
#[derive(Debug)]
pub struct DoclingParser;

impl DoclingParser {
    pub fn new(_config: Options) -> PResult<Self> {
        Ok(DoclingParser)
    }

    pub fn parse(&mut self, _file_path: impl AsRef<Path>, _options: &Options) -> PResult<Map<String, Value>> {
        todo!()
    }
}
