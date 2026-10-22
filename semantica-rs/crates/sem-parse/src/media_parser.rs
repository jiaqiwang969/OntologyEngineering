//! Media parsing (`semantica/parse/media_parser.py`). STUB — replaced by the full port.
//!
//! API CONTRACT used by other modules:
//! - `MediaParser::new(config: Option<Options>, kwargs: Options) -> MediaParser`
//! - `MediaParser::parse_media(&self, file_path: impl AsRef<Path>, media_type: Option<&str>, options: &Options) -> PResult<Map<String, Value>>`

use std::path::Path;

use serde_json::{Map, Value};

use crate::common::{Options, PResult};

/// `MediaParser`.
#[derive(Debug, Clone)]
pub struct MediaParser;

impl MediaParser {
    pub fn new(_config: Option<Options>, _kwargs: Options) -> Self {
        MediaParser
    }

    pub fn parse_media(
        &self,
        _file_path: impl AsRef<Path>,
        _media_type: Option<&str>,
        _options: &Options,
    ) -> PResult<Map<String, Value>> {
        todo!("media_parser port")
    }
}
