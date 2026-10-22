//! Image parsing (`semantica/parse/image_parser.py`).
//!
//! [`ImageParser`] reads image metadata the way Pillow 12 reports it (`format`, `mode`,
//! `size`, the per-format `info` dict and `_getexif()` tags, see [`pil`]) and runs OCR
//! through an [`OcrEngine`] (pytesseract's `image_to_string`/`image_to_data`), plus
//! `preprocess_image` with Pillow's pixel arithmetic (ITU-R 601-2 grayscale, LANCZOS
//! resampling, `ImageEnhance.Contrast`).
//!
//! As in the original:
//! - `parse` with `extract_metadata=True` (the default) calls a `_extract_metadata` method the
//!   class does not have, so it always fails with
//!   `ProcessingError("Failed to parse image: 'ImageParser' object has no attribute
//!   '_extract_metadata'")` for valid images;
//! - OCR needs an engine: pytesseract is not installed in the reference environment, so a
//!   parser without one reports `Tesseract OCR is not available. Install pytesseract and
//!   tesseract.` A `tesseract_path` configuration (or [`ImageParser::with_ocr_engine`])
//!   provides one ([`TesseractCli`]); the original instead fails with
//!   `AttributeError: 'NoneType' object has no attribute 'pytesseract'` when pytesseract is
//!   missing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    Options, PResult, opt_string, opt_truthy, py_exists, py_name, py_parent, py_path_str,
    py_stem, py_suffix,
};

/// Whether pytesseract can be imported. It cannot in the reference environment; an
/// [`ImageParser`] runs OCR only with an explicitly configured [`OcrEngine`].
pub const TESSERACT_AVAILABLE: bool = false;

fn size_as_list<S: Serializer>(size: &(u32, u32), s: S) -> Result<S::Ok, S::Error> {
    [size.0, size.1].serialize(s)
}

/// `ImageMetadata`: image metadata representation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImageMetadata {
    /// `img.format` (`PNG`, `JPEG`, ...).
    pub format: Option<String>,
    /// `img.mode` (`RGB`, `RGBA`, `L`, `P`, `1`, `I;16`, `CMYK`, ...).
    pub mode: Option<String>,
    /// `img.size` (serialised as `[width, height]`).
    #[serde(serialize_with = "size_as_list")]
    pub size: (u32, u32),
    /// `img.width`.
    pub width: u32,
    /// `img.height`.
    pub height: u32,
    /// `{str(ExifTags.TAGS.get(tag, tag)): str(value)}` from `img._getexif()`.
    pub exif: Map<String, Value>,
    /// `img.info`: bytes values as their Python `repr`, tuples as arrays.
    pub info: Map<String, Value>,
}

/// `OCRResult`: OCR result representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OCRResult {
    /// The recognised text (stripped).
    pub text: String,
    /// Mean word confidence divided by 100.
    pub confidence: f64,
    /// The OCR language code.
    pub language: String,
    /// Word boxes: `text`, `confidence`, `left`, `top`, `width`, `height`.
    pub boxes: Vec<Map<String, Value>>,
}

impl Default for OCRResult {
    fn default() -> Self {
        OCRResult {
            text: String::new(),
            confidence: 0.0,
            language: "eng".to_string(),
            boxes: Vec::new(),
        }
    }
}

/// pytesseract's `image_to_data(..., output_type=Output.DICT)`: parallel columns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcrData {
    pub level: Vec<i64>,
    pub left: Vec<i64>,
    pub top: Vec<i64>,
    pub width: Vec<i64>,
    pub height: Vec<i64>,
    /// Word confidences (`-1` for non-word rows).
    pub conf: Vec<f64>,
    pub text: Vec<String>,
}

/// The OCR backend (pytesseract in the original).
pub trait OcrEngine: Send + Sync {
    /// `pytesseract.image_to_string(path, lang=language)`.
    fn image_to_string(&self, path: &Path, language: &str) -> PResult<String>;
    /// `pytesseract.image_to_data(path, lang=language, output_type=Output.DICT)`.
    fn image_to_data(&self, path: &Path, language: &str) -> PResult<OcrData>;
}

/// [`OcrEngine`] running the `tesseract` command (what pytesseract does).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TesseractCli {
    /// `pytesseract.pytesseract.tesseract_cmd`.
    pub cmd: String,
}

impl Default for TesseractCli {
    fn default() -> Self {
        TesseractCli {
            cmd: "tesseract".to_string(),
        }
    }
}

impl TesseractCli {
    /// A runner for the given executable.
    pub fn new(cmd: impl Into<String>) -> Self {
        TesseractCli { cmd: cmd.into() }
    }

    fn run(&self, path: &Path, language: &str, extra: &[&str]) -> PResult<String> {
        let output = Command::new(&self.cmd)
            .arg(path)
            .arg("stdout")
            .arg("-l")
            .arg(language)
            .args(extra)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    SemanticaError::Other {
                        module: "pytesseract.pytesseract".to_string(),
                        type_name: "TesseractNotFoundError".to_string(),
                        message: format!(
                            "{} is not installed or it's not in your PATH. See README file for more information.",
                            self.cmd
                        ),
                    }
                } else {
                    SemanticaError::from(e)
                }
            })?;
        if !output.status.success() {
            return Err(SemanticaError::Other {
                module: "pytesseract.pytesseract".to_string(),
                type_name: "TesseractError".to_string(),
                message: format!(
                    "({}, '{}')",
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl OcrEngine for TesseractCli {
    fn image_to_string(&self, path: &Path, language: &str) -> PResult<String> {
        self.run(path, language, &[])
    }

    fn image_to_data(&self, path: &Path, language: &str) -> PResult<OcrData> {
        let tsv = self.run(path, language, &["tsv"])?;
        let mut data = OcrData::default();
        for line in tsv.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 {
                continue;
            }
            let int = |s: &str| s.trim().parse::<i64>().unwrap_or(0);
            data.level.push(int(cols[0]));
            data.left.push(int(cols[6]));
            data.top.push(int(cols[7]));
            data.width.push(int(cols[8]));
            data.height.push(int(cols[9]));
            data.conf.push(cols[10].trim().parse::<f64>().unwrap_or(-1.0));
            data.text.push(cols[11].to_string());
        }
        Ok(data)
    }
}

/// `ImageParser(**config)`: image parser for OCR and metadata extraction.
#[derive(Clone)]
pub struct ImageParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
    /// `self.tesseract_path` (`config["tesseract_path"]`).
    pub tesseract_path: Option<String>,
    /// The OCR backend; `None` when OCR is unavailable.
    pub ocr_engine: Option<Arc<dyn OcrEngine>>,
}

impl std::fmt::Debug for ImageParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageParser")
            .field("config", &self.config)
            .field("tesseract_path", &self.tesseract_path)
            .field("ocr_engine", &self.ocr_engine.is_some())
            .finish()
    }
}

impl Default for ImageParser {
    fn default() -> Self {
        ImageParser::new(Options::new())
    }
}

impl ImageParser {
    /// `ImageParser(**config)`. A `tesseract_path` configures a [`TesseractCli`] OCR engine
    /// with that executable.
    pub fn new(config: Options) -> Self {
        let tesseract_path = crate::common::opt_opt_string(&config, "tesseract_path")
            .filter(|p| !p.is_empty());
        let ocr_engine = tesseract_path
            .as_ref()
            .map(|p| Arc::new(TesseractCli::new(p.clone())) as Arc<dyn OcrEngine>);
        ImageParser {
            logger: get_logger("image_parser"),
            config,
            progress_tracker: get_progress_tracker(),
            tesseract_path,
            ocr_engine,
        }
    }

    /// `ImageParser(**config)` with an explicit OCR engine.
    pub fn with_ocr_engine(config: Options, engine: Arc<dyn OcrEngine>) -> Self {
        let mut parser = ImageParser::new(config);
        parser.ocr_engine = Some(engine);
        parser
    }

    /// `parse(file_path, **options)`: `{"metadata", "ocr", "text"}`. Options:
    /// `extract_metadata` (default `True`, which always fails as in the original),
    /// `extract_text` (default `False`; needs an OCR engine), `ocr_language` (`eng`).
    pub fn parse(&self, file_path: impl AsRef<Path>, options: &Options) -> PResult<Map<String, Value>> {
        let file_path = file_path.as_ref();
        let shown = py_path_str(file_path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&shown),
            Some("parse"),
            Some("ImageParser"),
            &format!("Image: {}", py_name(file_path)),
            None,
        );
        let result = (|| -> PResult<Map<String, Value>> {
            if !py_exists(file_path)? {
                return Err(ValidationError::new(format!("Image file not found: {shown}")).into());
            }
            pil::open(file_path)
                .and_then(|img| img.verify(file_path).map(|_| img))
                .map_err(|e| SemanticaError::from(ValidationError::new(format!("Invalid image file: {e}"))))?;
            let inner = (|| -> PResult<Map<String, Value>> {
                let _img = pil::open(file_path)?;
                if opt_truthy(options, "extract_metadata", true) {
                    self.progress_tracker
                        .update_tracking(&tracking_id, None, "Extracting metadata...");
                    // `self._extract_metadata(img, file_path)`: not defined on the class.
                    return Err(crate::common::attribute_error("ImageParser", "_extract_metadata"));
                }
                let mut ocr: Option<OCRResult> = None;
                if opt_truthy(options, "extract_text", false) && self.ocr_engine.is_some() {
                    self.progress_tracker
                        .update_tracking(&tracking_id, None, "Running OCR...");
                    let language = opt_string(options, "ocr_language", "eng");
                    ocr = Some(self.extract_text(file_path, Some(&language), &Options::new())?);
                }
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    "Image parsed successfully",
                    None,
                );
                let mut out = Map::new();
                out.insert("metadata".to_string(), Value::Null);
                out.insert(
                    "ocr".to_string(),
                    ocr.as_ref()
                        .map(|o| serde_json::to_value(o).unwrap_or(Value::Null))
                        .unwrap_or(Value::Null),
                );
                out.insert(
                    "text".to_string(),
                    ocr.map(|o| Value::String(o.text)).unwrap_or(Value::Null),
                );
                Ok(out)
            })();
            inner.map_err(|e| {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                self.logger
                    .error(format!("Failed to parse image {shown}: {e}"));
                ProcessingError::new(format!("Failed to parse image: {e}")).into()
            })
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `extract_text(file_path, language="eng", **options)`: OCR with the configured engine.
    pub fn extract_text(
        &self,
        file_path: impl AsRef<Path>,
        language: Option<&str>,
        _options: &Options,
    ) -> PResult<OCRResult> {
        let Some(engine) = self.ocr_engine.as_ref() else {
            return Err(ProcessingError::new(
                "Tesseract OCR is not available. Install pytesseract and tesseract.",
            )
            .into());
        };
        let language = language.unwrap_or("eng");
        let file_path = file_path.as_ref();
        if !py_exists(file_path)? {
            return Err(ValidationError::new(format!(
                "Image file not found: {}",
                py_path_str(file_path)
            ))
            .into());
        }
        let result = (|| -> PResult<OCRResult> {
            let text = engine.image_to_string(&crate::common::py_fs_path(file_path), language)?;
            let data = engine.image_to_data(&crate::common::py_fs_path(file_path), language)?;
            let confidences: Vec<f64> = data.conf.iter().copied().filter(|c| *c != -1.0).collect();
            let avg = if confidences.is_empty() {
                0.0
            } else {
                confidences.iter().sum::<f64>() / confidences.len() as f64
            };
            let mut boxes = Vec::new();
            for i in 0..data.level.len() {
                let word = data.text.get(i).cloned().unwrap_or_default();
                if !sem_utils::pycompat::fmt::strip(&word).is_empty() {
                    let mut b = Map::new();
                    b.insert("text".to_string(), Value::String(word));
                    b.insert("confidence".to_string(), num(data.conf.get(i).copied().unwrap_or(-1.0)));
                    b.insert("left".to_string(), Value::from(data.left.get(i).copied().unwrap_or(0)));
                    b.insert("top".to_string(), Value::from(data.top.get(i).copied().unwrap_or(0)));
                    b.insert("width".to_string(), Value::from(data.width.get(i).copied().unwrap_or(0)));
                    b.insert("height".to_string(), Value::from(data.height.get(i).copied().unwrap_or(0)));
                    boxes.push(b);
                }
            }
            Ok(OCRResult {
                text: sem_utils::pycompat::fmt::strip(&text).to_string(),
                confidence: avg / 100.0,
                language: language.to_string(),
                boxes,
            })
        })();
        result.map_err(|e| {
            self.logger.error(format!(
                "Failed to extract text from image {}: {e}",
                py_path_str(file_path)
            ));
            ProcessingError::new(format!("Failed to extract text from image: {e}")).into()
        })
    }

    /// `extract_metadata(file_path)`: format, mode, size, EXIF tags and `info`.
    pub fn extract_metadata(&self, file_path: impl AsRef<Path>) -> PResult<ImageMetadata> {
        let file_path = file_path.as_ref();
        if !py_exists(file_path)? {
            return Err(ValidationError::new(format!(
                "Image file not found: {}",
                py_path_str(file_path)
            ))
            .into());
        }
        let result = (|| -> PResult<ImageMetadata> {
            let img = pil::open(file_path)?;
            let mut exif = Map::new();
            if let Some(tags) = img.getexif() {
                for (tag, value) in tags {
                    let name = crate::exif::tag_name(tag as u16)
                        .map(str::to_string)
                        .unwrap_or_else(|| tag.to_string());
                    exif.insert(name, Value::String(value.py_str()));
                }
            }
            Ok(ImageMetadata {
                format: Some(img.format.to_string()),
                mode: Some(img.mode.clone()),
                size: (img.width, img.height),
                width: img.width,
                height: img.height,
                exif,
                info: img
                    .info
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            })
        })();
        result.map_err(|e| {
            self.logger.error(format!(
                "Failed to extract metadata from image {}: {e}",
                py_path_str(file_path)
            ));
            ProcessingError::new(format!("Failed to extract metadata from image: {e}")).into()
        })
    }

    /// `preprocess_image(file_path, **options)`: grayscale (`grayscale`, default `True`),
    /// LANCZOS resize (`resize` factor) and contrast 1.5 (`enhance_contrast`, default
    /// `True`), saved as `<stem>_preprocessed<suffix>` next to the input. Pixels are decoded
    /// for PNG, JPEG and WebP inputs.
    pub fn preprocess_image(&self, file_path: impl AsRef<Path>, options: &Options) -> PResult<PathBuf> {
        let file_path = file_path.as_ref();
        let header = pil::open(file_path)?;
        let mut img = pixels::load(file_path, &header)?;
        if opt_truthy(options, "grayscale", true) {
            img = img.convert_l();
        }
        if let Some(factor) = options.get("resize").filter(|v| sem_utils::pycompat::fmt::truthy(v)) {
            let f = factor.as_f64().unwrap_or(0.0);
            let w = (f64::from(img.width) * f) as i64;
            let h = (f64::from(img.height) * f) as i64;
            if w <= 0 || h <= 0 {
                return Err(SemanticaError::Value("height and width must be > 0".to_string()));
            }
            img = img.resize_lanczos(w as u32, h as u32);
        }
        if opt_truthy(options, "enhance_contrast", true) {
            img = img.enhance_contrast(1.5)?;
        }
        let output_path = py_parent(file_path).join(format!(
            "{}_preprocessed{}",
            py_stem(file_path),
            py_suffix(file_path)
        ));
        img.save(&output_path)?;
        Ok(output_path)
    }
}

fn num(f: f64) -> Value {
    serde_json::Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Pillow's image identification and header reading (`Image.open` / `verify` / `info` /
/// `_getexif`) for PNG, JPEG, GIF, BMP, TIFF and WebP.
pub(crate) mod pil {
    use std::path::Path;

    use indexmap::IndexMap;

    use sem_utils::SemanticaError;

    use crate::common::py_path_str;
    use crate::exif::{Exif, PyVal};

    /// An opened image: what `Image.open` reads from the header.
    #[derive(Debug, Clone)]
    pub(crate) struct PilImage {
        pub format: &'static str,
        pub mode: String,
        pub width: u32,
        pub height: u32,
        pub info: IndexMap<String, PyVal>,
        /// Whether the format's image class has `_getexif` (JPEG, PNG, WebP).
        pub has_getexif: bool,
        /// PNG: the offset of the first IDAT chunk (for `verify`).
        pub png_idat: Option<usize>,
        /// The palette of a `P` image (RGB triples), when known.
        pub palette: Option<Vec<[u8; 3]>>,
    }

    impl PilImage {
        /// `img._getexif()`: the merged EXIF dict, `None` without EXIF data.
        pub(crate) fn getexif(&self) -> Option<Vec<(i64, PyVal)>> {
            if !self.has_getexif {
                return None;
            }
            let raw = match self.info.get("exif") {
                Some(PyVal::Bytes(b)) => b.clone(),
                _ => return None,
            };
            let mut exif = Exif::load(&raw)?;
            let merged = exif.merged_dict();
            if merged.is_empty() { None } else { Some(merged) }
        }

        /// `img.verify()`: PNG checks the chunk CRCs up to IEND; the other formats accept
        /// what `open` accepted.
        pub(crate) fn verify(&self, path: &Path) -> Result<(), SemanticaError> {
            if self.format == "PNG"
                && let Some(start) = self.png_idat
            {
                let data = crate::common::py_read_bytes(path)?;
                let mut pos = start;
                loop {
                    if pos + 8 > data.len() {
                        return Err(syntax_error("broken PNG file"));
                    }
                    let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                    let ctype = &data[pos + 4..pos + 8];
                    if pos + 12 + len > data.len() {
                        return Err(SemanticaError::Os(sem_utils::exceptions::OsError::new(
                            "OSError",
                            format!(
                                "Truncated File Read"
                            ),
                        )));
                    }
                    let body = &data[pos + 8..pos + 8 + len];
                    let crc = u32::from_be_bytes([
                        data[pos + 8 + len],
                        data[pos + 9 + len],
                        data[pos + 10 + len],
                        data[pos + 11 + len],
                    ]);
                    if crc32(ctype, body) != crc {
                        return Err(syntax_error(&format!(
                            "broken PNG file (bad header checksum in {})",
                            crate::exif::bytes_repr(ctype)
                        )));
                    }
                    if ctype == b"IEND" {
                        return Ok(());
                    }
                    pos += 12 + len;
                }
            }
            Ok(())
        }
    }

    fn syntax_error(message: &str) -> SemanticaError {
        SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "SyntaxError".to_string(),
            message: message.to_string(),
        }
    }

    /// `PIL.UnidentifiedImageError: cannot identify image file '<path>'`.
    fn unidentified(path: &Path) -> SemanticaError {
        SemanticaError::Other {
            module: "PIL".to_string(),
            type_name: "UnidentifiedImageError".to_string(),
            message: format!(
                "cannot identify image file {}",
                sem_utils::pycompat::fmt::str_repr(&py_path_str(path))
            ),
        }
    }

    /// `Image.open(path)`.
    pub(crate) fn open(path: &Path) -> Result<PilImage, SemanticaError> {
        let data = crate::common::py_read_bytes(path)?;
        let opened = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            open_png(&data)
        } else if data.starts_with(b"\xff\xd8\xff") {
            open_jpeg(&data)
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            open_gif(&data)
        } else if data.starts_with(b"BM") {
            open_bmp(&data)
        } else if data.starts_with(b"II*\x00") || data.starts_with(b"MM\x00*") {
            open_tiff(&data)
        } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            open_webp(&data)
        } else {
            None
        };
        opened.ok_or_else(|| unidentified(path))
    }

    fn be32(d: &[u8], at: usize) -> Option<u32> {
        d.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn be16(d: &[u8], at: usize) -> Option<u16> {
        d.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn le16(d: &[u8], at: usize) -> Option<u16> {
        d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn le32(d: &[u8], at: usize) -> Option<u32> {
        d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn le24(d: &[u8], at: usize) -> Option<u32> {
        d.get(at..at + 3).map(|b| u32::from_le_bytes([b[0], b[1], b[2], 0]))
    }

    /// CRC-32 (PNG) of a chunk type and body.
    pub(crate) fn crc32(ctype: &[u8], body: &[u8]) -> u32 {
        let mut crc: u32 = 0xffff_ffff;
        for &b in ctype.iter().chain(body.iter()) {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    fn latin1(b: &[u8]) -> String {
        b.iter().map(|&c| c as char).collect()
    }

    /// zlib-inflate (PNG zTXt/iTXt/iCCP) through lopdf's FlateDecode.
    fn inflate(data: &[u8]) -> Option<Vec<u8>> {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Filter", lopdf::Object::Name(b"FlateDecode".to_vec()));
        let stream = lopdf::Stream::new(dict, data.to_vec());
        stream.decompressed_content().ok()
    }

    fn open_png(d: &[u8]) -> Option<PilImage> {
        let mut pos = 8;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        let mut header: Option<(u32, u32, u8, u8)> = None;
        let mut palette = None;
        loop {
            let len = be32(d, pos)? as usize;
            let ctype = d.get(pos + 4..pos + 8)?;
            let body = d.get(pos + 8..pos + 8 + len)?;
            let crc = be32(d, pos + 8 + len)?;
            if crc32(ctype, body) != crc {
                return None;
            }
            match ctype {
                b"IHDR" => {
                    if len < 13 {
                        return None;
                    }
                    header = Some((be32(body, 0)?, be32(body, 4)?, body[8], body[9]));
                }
                b"PLTE" => {
                    palette = Some(body.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect::<Vec<_>>());
                }
                b"IDAT" | b"fdAT" => {
                    let (w, h, depth, color) = header?;
                    let mode = match (depth, color) {
                        (1, 0) => "1",
                        (2, 0) | (4, 0) | (8, 0) => "L",
                        (16, 0) => "I;16",
                        (8, 2) | (16, 2) => "RGB",
                        (1, 3) | (2, 3) | (4, 3) | (8, 3) => "P",
                        (8, 4) | (16, 4) => "LA",
                        (8, 6) | (16, 6) => "RGBA",
                        _ => return None,
                    };
                    return Some(PilImage {
                        format: "PNG",
                        mode: mode.to_string(),
                        width: w,
                        height: h,
                        info,
                        has_getexif: true,
                        png_idat: Some(pos),
                        palette,
                    });
                }
                b"pHYs" if len >= 9 => {
                    let px = be32(body, 0)? as f64;
                    let py = be32(body, 4)? as f64;
                    if body[8] == 1 {
                        info.insert(
                            "dpi".to_string(),
                            PyVal::Tuple(vec![PyVal::Float(px * 0.0254), PyVal::Float(py * 0.0254)]),
                        );
                    } else {
                        info.insert(
                            "aspect".to_string(),
                            PyVal::Tuple(vec![PyVal::Int(px as i64), PyVal::Int(py as i64)]),
                        );
                    }
                }
                b"gAMA" if len >= 4 => {
                    info.insert("gamma".to_string(), PyVal::Float(f64::from(be32(body, 0)?) / 100_000.0));
                }
                b"sRGB" if len >= 1 => {
                    info.insert("srgb".to_string(), PyVal::Int(i64::from(body[0])));
                }
                b"tEXt" => {
                    if let Some(nul) = body.iter().position(|&c| c == 0) {
                        info.insert(latin1(&body[..nul]), PyVal::Str(latin1(&body[nul + 1..])));
                    }
                }
                b"zTXt" => {
                    if let Some(nul) = body.iter().position(|&c| c == 0)
                        && let Some(text) = body.get(nul + 2..).and_then(inflate)
                    {
                        info.insert(latin1(&body[..nul]), PyVal::Str(latin1(&text)));
                    }
                }
                b"iTXt" => {
                    if let Some(nul) = body.iter().position(|&c| c == 0) {
                        let key = latin1(&body[..nul]);
                        let flag = body.get(nul + 1).copied().unwrap_or(0);
                        let rest = body.get(nul + 3..).unwrap_or_default();
                        let lang_end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
                        let rest2 = rest.get(lang_end + 1..).unwrap_or_default();
                        let tk_end = rest2.iter().position(|&c| c == 0).unwrap_or(rest2.len());
                        let raw = rest2.get(tk_end + 1..).unwrap_or_default();
                        let text = if flag == 1 { inflate(raw).unwrap_or_default() } else { raw.to_vec() };
                        info.insert(key, PyVal::Str(String::from_utf8_lossy(&text).into_owned()));
                    }
                }
                b"eXIf" => {
                    let mut v = b"Exif\x00\x00".to_vec();
                    v.extend_from_slice(body);
                    info.insert("exif".to_string(), PyVal::Bytes(v));
                }
                b"iCCP" => {
                    if let Some(nul) = body.iter().position(|&c| c == 0)
                        && let Some(icc) = body.get(nul + 2..).and_then(inflate)
                    {
                        info.insert("icc_profile".to_string(), PyVal::Bytes(icc));
                    }
                }
                b"tRNS" => {
                    if let Some((_, _, _, color)) = header {
                        match color {
                            3 => match body.iter().position(|&a| a == 0) {
                                Some(i) if body[i + 1..].iter().all(|&a| a == 255) => {
                                    info.insert("transparency".to_string(), PyVal::Int(i as i64));
                                }
                                _ => {
                                    info.insert("transparency".to_string(), PyVal::Bytes(body.to_vec()));
                                }
                            },
                            0 if len >= 2 => {
                                info.insert("transparency".to_string(), PyVal::Int(i64::from(be16(body, 0)?)));
                            }
                            2 if len >= 6 => {
                                info.insert(
                                    "transparency".to_string(),
                                    PyVal::Tuple(vec![
                                        PyVal::Int(i64::from(be16(body, 0)?)),
                                        PyVal::Int(i64::from(be16(body, 2)?)),
                                        PyVal::Int(i64::from(be16(body, 4)?)),
                                    ]),
                                );
                            }
                            _ => {}
                        }
                    }
                }
                b"IEND" => return None,
                _ => {}
            }
            pos += 12 + len;
        }
    }

    fn open_jpeg(d: &[u8]) -> Option<PilImage> {
        let mut pos = 2;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        let mut frame: Option<(u32, u32, u8)> = None;
        loop {
            // Skip fill bytes before a marker.
            while d.get(pos) == Some(&0xff) && d.get(pos + 1) == Some(&0xff) {
                pos += 1;
            }
            if d.get(pos) != Some(&0xff) {
                return None;
            }
            let marker = *d.get(pos + 1)?;
            if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
                pos += 2;
                continue;
            }
            if marker == 0xd9 {
                break;
            }
            let len = be16(d, pos + 2)? as usize;
            if len < 2 {
                return None;
            }
            let body = d.get(pos + 4..pos + 2 + len)?;
            match marker {
                0xe0 if body.starts_with(b"JFIF\x00") && body.len() >= 12 => {
                    let version = i64::from(be16(body, 5)?);
                    info.insert("jfif".to_string(), PyVal::Int(version));
                    info.insert(
                        "jfif_version".to_string(),
                        PyVal::Tuple(vec![PyVal::Int(version / 256), PyVal::Int(version % 256)]),
                    );
                    let unit = i64::from(body[7]);
                    let density = (i64::from(be16(body, 8)?), i64::from(be16(body, 10)?));
                    if unit == 1 {
                        info.insert(
                            "dpi".to_string(),
                            PyVal::Tuple(vec![PyVal::Int(density.0), PyVal::Int(density.1)]),
                        );
                    }
                    info.insert("jfif_unit".to_string(), PyVal::Int(unit));
                    info.insert(
                        "jfif_density".to_string(),
                        PyVal::Tuple(vec![PyVal::Int(density.0), PyVal::Int(density.1)]),
                    );
                }
                0xe1 if body.starts_with(b"Exif\x00\x00") && !info.contains_key("exif") => {
                    info.insert("exif".to_string(), PyVal::Bytes(body.to_vec()));
                }
                0xe2 if body.starts_with(b"ICC_PROFILE\x00") && body.len() > 14 => {
                    let chunk = body[14..].to_vec();
                    match info.get_mut("icc_profile") {
                        Some(PyVal::Bytes(existing)) => existing.extend_from_slice(&chunk),
                        _ => {
                            info.insert("icc_profile".to_string(), PyVal::Bytes(chunk));
                        }
                    }
                }
                0xee if body.starts_with(b"Adobe") && body.len() >= 12 => {
                    info.insert("adobe".to_string(), PyVal::Int(i64::from(be16(body, 5)?)));
                    info.insert("adobe_transform".to_string(), PyVal::Int(i64::from(body[11])));
                }
                0xfe => {
                    info.insert("comment".to_string(), PyVal::Bytes(body.to_vec()));
                }
                0xc0..=0xcf if marker != 0xc4 && marker != 0xc8 && marker != 0xcc => {
                    if body.len() < 6 {
                        return None;
                    }
                    let height = u32::from(be16(body, 1)?);
                    let width = u32::from(be16(body, 3)?);
                    let comps = body[5];
                    frame = Some((width, height, comps));
                    if matches!(marker, 0xc2 | 0xc6 | 0xca | 0xce) {
                        info.insert("progressive".to_string(), PyVal::Int(1));
                        info.insert("progression".to_string(), PyVal::Int(1));
                    }
                }
                0xda => break,
                _ => {}
            }
            pos += 2 + len;
        }
        let (width, height, comps) = frame?;
        let mode = match comps {
            1 => "L",
            3 => "RGB",
            4 => "CMYK",
            _ => return None,
        };
        // `dpi` from the EXIF resolution when JFIF gave none.
        if !info.contains_key("dpi")
            && let Some(PyVal::Bytes(raw)) = info.get("exif").cloned()
            && let Some(mut exif) = Exif::load(&raw)
        {
            let unit = exif.get(296).and_then(|v| v.as_int()).unwrap_or(2);
            if let Some(x) = exif.get(282)
                && let Some(xf) = x.as_f64()
            {
                let dpi = if unit == 3 { xf * 2.54 } else { xf };
                let dpi = if dpi.is_nan() { -1.0 } else { dpi };
                info.insert("dpi".to_string(), PyVal::Tuple(vec![PyVal::Float(dpi), PyVal::Float(dpi)]));
            }
        }
        Some(PilImage {
            format: "JPEG",
            mode: mode.to_string(),
            width,
            height,
            info,
            has_getexif: true,
            png_idat: None,
            palette: None,
        })
    }

    fn open_gif(d: &[u8]) -> Option<PilImage> {
        let width = u32::from(le16(d, 6)?);
        let height = u32::from(le16(d, 8)?);
        let flags = *d.get(10)?;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        info.insert("version".to_string(), PyVal::Bytes(d[0..6].to_vec()));
        let mut mode = "L";
        let mut palette = None;
        let mut pos = 13;
        if flags & 0x80 != 0 {
            let bits = (flags & 7) + 1;
            let size = 3usize << bits;
            let p = d.get(13..13 + size)?;
            info.insert("background".to_string(), PyVal::Int(i64::from(d[11])));
            let identity = p.chunks(3).enumerate().all(|(i, c)| usize::from(c[0]) == i && c[0] == c[1] && c[1] == c[2]);
            if !identity {
                mode = "P";
            }
            palette = Some(p.chunks(3).map(|c| [c[0], c[1], c[2]]).collect());
            pos += size;
        }
        // Extension blocks before the first image descriptor.
        loop {
            match d.get(pos)? {
                0x21 => {
                    let label = *d.get(pos + 1)?;
                    let mut p = pos + 2;
                    let mut first = true;
                    loop {
                        let n = usize::from(*d.get(p)?);
                        if n == 0 {
                            p += 1;
                            break;
                        }
                        let block = d.get(p + 1..p + 1 + n)?;
                        if first {
                            match label {
                                0xf9 if block.len() >= 4 => {
                                    let fl = block[0];
                                    let duration = i64::from(le16(block, 1)?) * 10;
                                    info.insert("duration".to_string(), PyVal::Int(duration));
                                    if fl & 1 != 0 {
                                        info.insert("transparency".to_string(), PyVal::Int(i64::from(block[3])));
                                    }
                                }
                                0xff if block.starts_with(b"NETSCAPE2.0") => {}
                                _ => {}
                            }
                        } else if label == 0xff && block.len() >= 3 && block[0] == 1 {
                            info.insert("loop".to_string(), PyVal::Int(i64::from(le16(block, 1)?)));
                        }
                        first = false;
                        p += 1 + n;
                    }
                    pos = p;
                }
                0x2c => {
                    let local = *d.get(pos + 9)?;
                    if local & 0x80 != 0 {
                        mode = "P";
                    }
                    break;
                }
                _ => return None,
            }
        }
        Some(PilImage {
            format: "GIF",
            mode: mode.to_string(),
            width,
            height,
            info,
            has_getexif: false,
            png_idat: None,
            palette,
        })
    }

    fn open_bmp(d: &[u8]) -> Option<PilImage> {
        let header_size = le32(d, 14)?;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        let (width, height, bits, compression, ppm) = if header_size >= 40 {
            let w = le32(d, 18)? as i32;
            let h = le32(d, 22)? as i32;
            let bits = le16(d, 28)?;
            let comp = le32(d, 30)?;
            let px = le32(d, 38)? as i32;
            let py = le32(d, 42)? as i32;
            (w.unsigned_abs(), h.unsigned_abs(), bits, comp, Some((px, py)))
        } else if header_size == 12 {
            (u32::from(le16(d, 18)?), u32::from(le16(d, 20)?), le16(d, 24)?, 0, None)
        } else {
            return None;
        };
        if let Some((px, py)) = ppm {
            info.insert(
                "dpi".to_string(),
                PyVal::Tuple(vec![PyVal::Float(f64::from(px) / 39.3701), PyVal::Float(f64::from(py) / 39.3701)]),
            );
            info.insert("compression".to_string(), PyVal::Int(i64::from(compression)));
        }
        let mode = match bits {
            1 => "1",
            4 | 8 => "P",
            16 | 24 => "RGB",
            32 => {
                if compression == 3 { "RGBA" } else { "RGB" }
            }
            _ => return None,
        };
        Some(PilImage {
            format: "BMP",
            mode: mode.to_string(),
            width,
            height,
            info,
            has_getexif: false,
            png_idat: None,
            palette: None,
        })
    }

    fn open_tiff(d: &[u8]) -> Option<PilImage> {
        let (endian, first) = crate::exif::tiff_header(d)?;
        let ifd = crate::exif::load_ifd(d, first as usize, endian, None);
        let get = |tag: u16| ifd.get(tag);
        let width = get(256)?.as_int()? as u32;
        let height = get(257)?.as_int()? as u32;
        let ints = |v: Option<PyVal>, default: Vec<i64>| -> Vec<i64> {
            match v {
                Some(v) => v.items().iter().filter_map(PyVal::as_int).collect(),
                None => default,
            }
        };
        let photo = get(262).and_then(|v| v.as_int()).unwrap_or(0);
        let compression = get(259).and_then(|v| v.as_int()).unwrap_or(1);
        let bps = ints(get(258), vec![1]);
        let samples = get(277).and_then(|v| v.as_int()).unwrap_or(1);
        let bps: Vec<i64> = if bps.len() == 1 && samples > 1 { vec![bps[0]; samples as usize] } else { bps };
        let extra = ints(get(338), vec![]);
        let sample_format = ints(get(339), vec![1]);
        let fillorder = get(266).and_then(|v| v.as_int()).unwrap_or(1);
        let byteorder = if d.starts_with(b"II") { "II" } else { "MM" };
        let mode = crate::exif::tiff_open_info(byteorder, photo, &sample_format, fillorder, &bps, &extra)?;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        info.insert(
            "compression".to_string(),
            PyVal::Str(crate::exif::tiff_compression(compression).unwrap_or_else(|| "raw".to_string())),
        );
        let xres = get(282).unwrap_or(PyVal::Int(1));
        let yres = get(283).unwrap_or(PyVal::Int(1));
        let nonzero = |v: &PyVal| v.as_f64().is_some_and(|f| f != 0.0);
        if nonzero(&xres) && nonzero(&yres) {
            match get(296).and_then(|v| v.as_int()) {
                Some(2) => {
                    info.insert("dpi".to_string(), PyVal::Tuple(vec![xres.clone(), yres.clone()]));
                }
                Some(3) => {
                    let f = |v: &PyVal| PyVal::Float(v.as_f64().unwrap_or(0.0) * 2.54);
                    info.insert("dpi".to_string(), PyVal::Tuple(vec![f(&xres), f(&yres)]));
                }
                None => {
                    info.insert("dpi".to_string(), PyVal::Tuple(vec![xres.clone(), yres.clone()]));
                    info.insert("resolution".to_string(), PyVal::Tuple(vec![xres, yres]));
                }
                Some(_) => {
                    info.insert("resolution".to_string(), PyVal::Tuple(vec![xres, yres]));
                }
            }
        }
        Some(PilImage {
            format: "TIFF",
            mode,
            width,
            height,
            info,
            has_getexif: false,
            png_idat: None,
            palette: None,
        })
    }

    fn open_webp(d: &[u8]) -> Option<PilImage> {
        let mut pos = 12;
        let mut info: IndexMap<String, PyVal> = IndexMap::new();
        let mut size: Option<(u32, u32)> = None;
        let mut alpha = false;
        let mut loop_count = 1i64;
        let mut background = [255i64, 255, 255, 255];
        while pos + 8 <= d.len() {
            let fourcc = &d[pos..pos + 4];
            let len = le32(d, pos + 4)? as usize;
            let body = d.get(pos + 8..pos + 8 + len)?;
            match fourcc {
                b"VP8X" if len >= 10 => {
                    alpha = body[0] & 0x10 != 0;
                    size = Some((le24(body, 4)? + 1, le24(body, 7)? + 1));
                }
                b"VP8 " if len >= 10 => {
                    if size.is_none() {
                        let w = u32::from(le16(body, 6)? & 0x3fff);
                        let h = u32::from(le16(body, 8)? & 0x3fff);
                        size = Some((w, h));
                    }
                }
                b"VP8L" if len >= 5 => {
                    let bits = le32(body, 1)?;
                    if size.is_none() {
                        size = Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
                    }
                    if bits >> 28 & 1 == 1 {
                        alpha = true;
                    }
                }
                b"ALPH" => alpha = true,
                b"ANIM" if len >= 6 => {
                    background = [
                        i64::from(body[2]),
                        i64::from(body[1]),
                        i64::from(body[0]),
                        i64::from(body[3]),
                    ];
                    loop_count = i64::from(le16(body, 4)?);
                }
                b"EXIF" => {
                    info.insert("exif".to_string(), PyVal::Bytes(body.to_vec()));
                }
                b"ICCP" => {
                    info.insert("icc_profile".to_string(), PyVal::Bytes(body.to_vec()));
                }
                b"XMP " => {
                    info.insert("xmp".to_string(), PyVal::Bytes(body.to_vec()));
                }
                _ => {}
            }
            pos += 8 + len + (len & 1);
        }
        let (width, height) = size?;
        let mut ordered: IndexMap<String, PyVal> = IndexMap::new();
        ordered.insert("loop".to_string(), PyVal::Int(loop_count));
        ordered.insert(
            "background".to_string(),
            PyVal::Tuple(background.iter().map(|&v| PyVal::Int(v)).collect()),
        );
        for (k, v) in info {
            ordered.insert(k, v);
        }
        Some(PilImage {
            format: "WEBP",
            mode: if alpha { "RGBA" } else { "RGB" }.to_string(),
            width,
            height,
            info: ordered,
            has_getexif: true,
            png_idat: None,
            palette: None,
        })
    }
}

/// Pillow's pixel operations for `preprocess_image`.
mod pixels {
    use std::path::Path;

    use sem_utils::SemanticaError;

    use super::pil::PilImage;
    use crate::common::py_suffix;

    /// An 8-bit image in one of Pillow's modes (`L`, `LA`, `RGB`, `RGBA`, `P`), one byte per
    /// band.
    #[derive(Debug, Clone)]
    pub(super) struct Pixels {
        pub mode: String,
        pub width: u32,
        pub height: u32,
        pub bands: usize,
        pub data: Vec<u8>,
        /// The source was a `P` image (Pillow keeps palette indices; blending them fails).
        pub palette_source: bool,
    }

    fn value_error(message: &str) -> SemanticaError {
        SemanticaError::Value(message.to_string())
    }

    /// Decode the pixels (PNG, JPEG and WebP through the `image` crate).
    pub(super) fn load(path: &Path, header: &PilImage) -> Result<Pixels, SemanticaError> {
        let bytes = crate::common::py_read_bytes(path)?;
        let format = match header.format {
            "PNG" => image::ImageFormat::Png,
            "JPEG" => image::ImageFormat::Jpeg,
            "WEBP" => image::ImageFormat::WebP,
            other => {
                return Err(SemanticaError::Os(sem_utils::exceptions::OsError::new(
                    "OSError",
                    format!("decoder for {other} images is not available"),
                )));
            }
        };
        let img = image::load_from_memory_with_format(&bytes, format)
            .map_err(|e| SemanticaError::Os(sem_utils::exceptions::OsError::new("OSError", e.to_string())))?;
        let (width, height) = (img.width(), img.height());
        let palette_source = header.mode == "P";
        let (mode, bands, data) = match header.mode.as_str() {
            "1" | "L" => ("L", 1, img.to_luma8().into_raw()),
            "I;16" => {
                // `I;16` -> `L` clips at 255 (Pillow's I16_L).
                let raw = img.to_luma16().into_raw();
                ("L", 1, raw.into_iter().map(|v| v.min(255) as u8).collect())
            }
            "LA" => ("LA", 2, img.to_luma_alpha8().into_raw()),
            "RGBA" => ("RGBA", 4, img.to_rgba8().into_raw()),
            "P" => {
                if img.color().has_alpha() {
                    ("RGBA", 4, img.to_rgba8().into_raw())
                } else {
                    ("RGB", 3, img.to_rgb8().into_raw())
                }
            }
            _ => ("RGB", 3, img.to_rgb8().into_raw()),
        };
        Ok(Pixels {
            mode: if palette_source { "P".to_string() } else { mode.to_string() },
            width,
            height,
            bands,
            data,
            palette_source,
        })
    }

    impl Pixels {
        /// `img.convert("L")` (ITU-R 601-2 luma, Pillow's fixed-point `L24`).
        pub(super) fn convert_l(self) -> Pixels {
            if self.bands == 1 {
                return Pixels {
                    mode: "L".to_string(),
                    palette_source: false,
                    ..self
                };
            }
            let mut out = Vec::with_capacity((self.width * self.height) as usize);
            for px in self.data.chunks(self.bands) {
                let v = if self.bands >= 3 {
                    (u32::from(px[0]) * 19595 + u32::from(px[1]) * 38470 + u32::from(px[2]) * 7471 + 0x8000) >> 16
                } else {
                    u32::from(px[0])
                };
                out.push(v as u8);
            }
            Pixels {
                mode: "L".to_string(),
                width: self.width,
                height: self.height,
                bands: 1,
                data: out,
                palette_source: false,
            }
        }

        /// `img.resize(size, Image.Resampling.LANCZOS)`: Pillow's two-pass convolution
        /// (`ImagingResample`), fixed-point coefficients with 22 fractional bits.
        pub(super) fn resize_lanczos(self, width: u32, height: u32) -> Pixels {
            if self.palette_source {
                // `P` images resample with NEAREST.
                return self.resize_nearest(width, height);
            }
            let premultiply = self.bands == 2 || self.bands == 4;
            let mut img = if premultiply { self.premultiplied() } else { self };
            let need_h = width != img.width;
            let need_v = height != img.height;
            let (kh, bh, ksize_h) = precompute_coeffs(img.width, 0.0, img.width as f64, width);
            let (kv, mut bv, ksize_v) = precompute_coeffs(img.height, 0.0, img.height as f64, height);
            let ybox_first = bv[0];
            let ybox_last = bv[(height as usize) * 2 - 2] + bv[(height as usize) * 2 - 1];
            if need_h {
                for i in 0..height as usize {
                    bv[i * 2] -= ybox_first;
                }
                img = img.horizontal(width, ybox_first, ybox_last - ybox_first, &kh, &bh, ksize_h);
            }
            if need_v {
                img = img.vertical(height, &kv, &bv, ksize_v);
            }
            if premultiply { img.unpremultiplied() } else { img }
        }

        fn resize_nearest(self, width: u32, height: u32) -> Pixels {
            let mut data = Vec::with_capacity((width * height) as usize * self.bands);
            for y in 0..height {
                let sy = ((f64::from(y) + 0.5) * f64::from(self.height) / f64::from(height)) as u32;
                for x in 0..width {
                    let sx = ((f64::from(x) + 0.5) * f64::from(self.width) / f64::from(width)) as u32;
                    let i = ((sy.min(self.height - 1) * self.width + sx.min(self.width - 1)) as usize) * self.bands;
                    data.extend_from_slice(&self.data[i..i + self.bands]);
                }
            }
            Pixels { width, height, data, ..self }
        }

        fn premultiplied(self) -> Pixels {
            let bands = self.bands;
            let mut data = self.data.clone();
            for px in data.chunks_mut(bands) {
                let a = u32::from(px[bands - 1]);
                for c in px.iter_mut().take(bands - 1) {
                    let tmp = u32::from(*c) * a + 128;
                    *c = (((tmp >> 8) + tmp) >> 8) as u8;
                }
            }
            Pixels { data, ..self }
        }

        fn unpremultiplied(self) -> Pixels {
            let bands = self.bands;
            let mut data = self.data.clone();
            for px in data.chunks_mut(bands) {
                let a = u32::from(px[bands - 1]);
                for c in px.iter_mut().take(bands - 1) {
                    *c = if a == 0 { 0 } else { ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8 };
                }
            }
            Pixels { data, ..self }
        }

        #[allow(clippy::too_many_arguments)]
        fn horizontal(&self, width: u32, offset: usize, rows: usize, kk: &[i32], bounds: &[usize], ksize: usize) -> Pixels {
            let b = self.bands;
            let mut data = vec![0u8; width as usize * rows * b];
            for yy in 0..rows {
                let src_row = &self.data[(yy + offset) * self.width as usize * b..];
                for xx in 0..width as usize {
                    let xmin = bounds[xx * 2];
                    let xmax = bounds[xx * 2 + 1];
                    let k = &kk[xx * ksize..];
                    for c in 0..b {
                        let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                        for x in 0..xmax {
                            ss += i64::from(src_row[(x + xmin) * b + c]) * i64::from(k[x]);
                        }
                        data[(yy * width as usize + xx) * b + c] = clip8(ss);
                    }
                }
            }
            Pixels {
                mode: self.mode.clone(),
                width,
                height: rows as u32,
                bands: b,
                data,
                palette_source: false,
            }
        }

        fn vertical(&self, height: u32, kk: &[i32], bounds: &[usize], ksize: usize) -> Pixels {
            let b = self.bands;
            let w = self.width as usize;
            let mut data = vec![0u8; w * height as usize * b];
            for yy in 0..height as usize {
                let ymin = bounds[yy * 2];
                let ymax = bounds[yy * 2 + 1];
                let k = &kk[yy * ksize..];
                for xx in 0..w {
                    for c in 0..b {
                        let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                        for y in 0..ymax {
                            ss += i64::from(self.data[((y + ymin) * w + xx) * b + c]) * i64::from(k[y]);
                        }
                        data[(yy * w + xx) * b + c] = clip8(ss);
                    }
                }
            }
            Pixels {
                mode: self.mode.clone(),
                width: self.width,
                height,
                bands: b,
                data,
                palette_source: false,
            }
        }

        /// `ImageEnhance.Contrast(img).enhance(factor)`: blend with a flat image of the
        /// rounded mean luminance (`Image.blend`, extrapolating with clipping).
        pub(super) fn enhance_contrast(self, factor: f64) -> Result<Pixels, SemanticaError> {
            if self.palette_source {
                return Err(value_error("image has wrong mode"));
            }
            let gray = self.clone().convert_l();
            let sum: u64 = gray.data.iter().map(|&v| u64::from(v)).sum();
            let n = gray.data.len().max(1) as f64;
            let mean = (sum as f64 / n + 0.5) as i64;
            let mean = mean.clamp(0, 255) as u8;
            let b = self.bands;
            let mut data = self.data.clone();
            for (i, v) in data.iter_mut().enumerate() {
                let is_alpha = (b == 2 || b == 4) && i % b == b - 1;
                if is_alpha {
                    continue; // the degenerate image carries the original alpha
                }
                let in1 = f64::from(mean);
                let in2 = f64::from(*v);
                let temp = (in1 + factor * (in2 - in1)) as f32;
                *v = if temp <= 0.0 {
                    0
                } else if temp >= 255.0 {
                    255
                } else {
                    temp as u8
                };
            }
            Ok(Pixels { data, ..self })
        }

        /// `img.save(path)`: format from the suffix (PNG, JPEG quality 75, WebP lossless).
        pub(super) fn save(&self, path: &Path) -> Result<(), SemanticaError> {
            let color = match self.bands {
                1 => image::ExtendedColorType::L8,
                2 => image::ExtendedColorType::La8,
                3 => image::ExtendedColorType::Rgb8,
                _ => image::ExtendedColorType::Rgba8,
            };
            let suffix = py_suffix(path).to_lowercase();
            let format = match suffix.as_str() {
                ".png" => image::ImageFormat::Png,
                ".jpg" | ".jpeg" | ".jpe" | ".jfif" => image::ImageFormat::Jpeg,
                ".webp" => image::ImageFormat::WebP,
                other => {
                    return Err(SemanticaError::Value(format!("unknown file extension: {other}")));
                }
            };
            let os_err = |e: image::ImageError| SemanticaError::Os(sem_utils::exceptions::OsError::new("OSError", e.to_string()));
            if format == image::ImageFormat::Jpeg && (self.bands == 2 || self.bands == 4) {
                return Err(SemanticaError::Os(sem_utils::exceptions::OsError::new(
                    "OSError",
                    format!("cannot write mode {} as JPEG", self.mode),
                )));
            }
            image::save_buffer_with_format(
                crate::common::py_fs_path(path),
                &self.data,
                self.width,
                self.height,
                color,
                format,
            )
            .map_err(os_err)
        }
    }

    const PRECISION_BITS: u32 = 32 - 8 - 2;

    fn clip8(v: i64) -> u8 {
        if v >= (1i64 << PRECISION_BITS << 8) {
            255
        } else if v <= 0 {
            0
        } else {
            (v >> PRECISION_BITS) as u8
        }
    }

    fn sinc(x: f64) -> f64 {
        if x == 0.0 {
            1.0
        } else {
            let x = x * std::f64::consts::PI;
            x.sin() / x
        }
    }

    fn lanczos(x: f64) -> f64 {
        if (-3.0..3.0).contains(&x) {
            sinc(x) * sinc(x / 3.0)
        } else {
            0.0
        }
    }

    /// Pillow's `precompute_coeffs` + `normalize_coeffs_8bpc`: (coefficients, bounds, ksize).
    fn precompute_coeffs(in_size: u32, in0: f64, in1: f64, out_size: u32) -> (Vec<i32>, Vec<usize>, usize) {
        let scale = (in1 - in0) / f64::from(out_size);
        let filterscale = scale.max(1.0);
        let support = 3.0 * filterscale;
        let ksize = (support.ceil() as usize) * 2 + 1;
        let mut kk = vec![0.0f64; out_size as usize * ksize];
        let mut bounds = vec![0usize; out_size as usize * 2];
        for xx in 0..out_size as usize {
            let center = in0 + (xx as f64 + 0.5) * scale;
            let ss = 1.0 / filterscale;
            let mut xmin = (center - support + 0.5) as i64;
            if xmin < 0 {
                xmin = 0;
            }
            let mut xmax = (center + support + 0.5) as i64;
            if xmax > i64::from(in_size) {
                xmax = i64::from(in_size);
            }
            xmax -= xmin;
            let k = &mut kk[xx * ksize..(xx + 1) * ksize];
            let mut ww = 0.0;
            for x in 0..xmax.max(0) as usize {
                let w = lanczos((x as f64 + xmin as f64 - center + 0.5) * ss);
                k[x] = w;
                ww += w;
            }
            for x in 0..xmax.max(0) as usize {
                if ww != 0.0 {
                    k[x] /= ww;
                }
            }
            bounds[xx * 2] = xmin as usize;
            bounds[xx * 2 + 1] = xmax.max(0) as usize;
        }
        let fixed = kk
            .iter()
            .map(|&v| {
                if v < 0.0 {
                    (-0.5 + v * f64::from(1u32 << PRECISION_BITS)) as i32
                } else {
                    (0.5 + v * f64::from(1u32 << PRECISION_BITS)) as i32
                }
            })
            .collect();
        (fixed, bounds, ksize)
    }
}
