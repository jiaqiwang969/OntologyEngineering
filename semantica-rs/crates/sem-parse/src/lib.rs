//! Rust port of Semantica's `semantica.parse` package (see `semantica-rs/scope/sem-parse.json`).
//!
//! Data parsing for documents (PDF, DOCX, PPTX, Excel, HTML, text), web content (HTML, XML,
//! JavaScript-rendered pages), structured data (JSON, CSV, XML, YAML), e-mail (headers,
//! MIME bodies, attachments, threads), source code (structure, comments, dependencies),
//! media (image metadata and OCR, audio/video metadata) and MCP server responses, plus the
//! method registry and configuration of the original module.
//!
//! Module files mirror the original (`pdf_parser.rs` is `pdf_parser.py`, ...). The crate root
//! re-exports the names `semantica.parse` exports (`__init__.py`'s `__all__`), so
//! `from semantica.parse import X` becomes `use sem_parse::X`.
//!
//! Python's `**options`/`**config` keyword dictionaries are [`Options`]
//! (`serde_json::Map`); arguments the original accepts as "a path or the content itself" are
//! [`Source`]s. Errors are [`sem_utils::SemanticaError`] with the original exception kinds
//! and messages. See `PORT-NOTES.md` for the behaviour that differs from the original.

pub mod code_parser;
pub mod common;
pub mod config;
pub mod csv_parser;
pub mod docling_parser;
pub mod document_parser;
pub mod docx_parser;
pub mod email_parser;
pub mod excel_parser;
pub mod html_parser;
pub mod image_parser;
pub mod json_parser;
pub mod mcp_parser;
pub mod media_parser;
pub mod methods;
pub mod parse_provenance;
pub mod pdf_parser;
pub mod pptx_parser;
pub mod registry;
pub mod structured_data_parser;
pub mod web_parser;
pub mod xml_parser;

// Private helpers: ports of the Python standard-library and third-party behaviour the
// original modules rely on (one per area).
mod exif; // PIL EXIF/info reading (image_parser)
mod mime; // the `email` package (email_parser)
mod ooxml; // python-docx / python-pptx / openpyxl+pandas models (office parsers)
mod pdfdoc; // pdfplumber-style PDF model on lopdf (pdf_parser)
mod pyast; // Python `ast` subset (code_parser)
mod pycsv; // the `csv` module (csv_parser, structured_data_parser)
mod pyjson; // the `json` decoder (json_parser, config)
mod soup; // BeautifulSoup + `html.parser` (html_parser, web_parser)
mod xpath; // lxml XPath subset (xml_parser)

pub use common::{Options, Source};

// Main parsers
pub use code_parser::{
    CodeComment, CodeParser, CodeStructure, CommentExtractor, DependencyAnalyzer,
    SyntaxTreeParser,
};
pub use config::{ParseConfig, parse_config};
pub use csv_parser::{CSVData, CSVParser};
pub use document_parser::DocumentParser;
pub use docx_parser::{DOCXParser, DocxMetadata, DocxSection};
pub use email_parser::{
    EmailBody, EmailData, EmailHeaders, EmailParser, EmailThreadAnalyzer, MIMEParser,
};
pub use excel_parser::{ExcelData, ExcelParser, ExcelSheet};
pub use html_parser::{HTMLElement, HTMLMetadata, HTMLParser};
pub use image_parser::{ImageMetadata, ImageParser, OCRResult};
pub use json_parser::{JSONData, JSONParser};
pub use mcp_parser::MCPParser;
pub use media_parser::MediaParser;
pub use methods::{
    get_parse_method, list_available_methods, parse_code, parse_csv, parse_document,
    parse_docx, parse_email, parse_image, parse_json, parse_media, parse_pdf,
    parse_structured_data, parse_web_content, parse_xml,
};
pub use pdf_parser::{PDFMetadata, PDFPage, PDFParser};
pub use pptx_parser::{PPTXData, PPTXParser, SlideContent};

// Optional Docling parser (`__all__.extend(["DoclingParser", "DoclingMetadata"])`).
pub use docling_parser::{
    DOCLING_AVAILABLE, DOCLING_IMPORT_ERROR, DoclingMetadata, DoclingParser,
};

// Registry and config
pub use registry::{MethodRegistry, method_registry};
pub use structured_data_parser::StructuredDataParser;
pub use web_parser::{HTMLContentParser, JavaScriptRenderer, WebParser};
pub use xml_parser::{XMLData, XMLElement, XMLParser};

/// `semantica.parse.__all__`: the exported names, in the original order (the two Docling
/// names are appended by `__all__.extend`).
#[allow(non_upper_case_globals)]
pub const __all__: &[&str] = &[
    "DocumentParser",
    "WebParser",
    "HTMLContentParser",
    "JavaScriptRenderer",
    "StructuredDataParser",
    "EmailParser",
    "EmailHeaders",
    "EmailBody",
    "EmailData",
    "MIMEParser",
    "EmailThreadAnalyzer",
    "CodeParser",
    "CodeStructure",
    "CodeComment",
    "SyntaxTreeParser",
    "CommentExtractor",
    "DependencyAnalyzer",
    "MediaParser",
    "PDFParser",
    "PDFPage",
    "PDFMetadata",
    "DOCXParser",
    "DocxSection",
    "DocxMetadata",
    "PPTXParser",
    "SlideContent",
    "PPTXData",
    "ExcelParser",
    "ExcelSheet",
    "ExcelData",
    "HTMLParser",
    "HTMLMetadata",
    "HTMLElement",
    "JSONParser",
    "JSONData",
    "CSVParser",
    "CSVData",
    "XMLParser",
    "XMLElement",
    "XMLData",
    "ImageParser",
    "ImageMetadata",
    "OCRResult",
    "MCPParser",
    "MethodRegistry",
    "method_registry",
    "ParseConfig",
    "parse_config",
    "parse_document",
    "parse_web_content",
    "parse_structured_data",
    "parse_email",
    "parse_code",
    "parse_media",
    "parse_pdf",
    "parse_docx",
    "parse_json",
    "parse_csv",
    "parse_xml",
    "parse_image",
    "get_parse_method",
    "list_available_methods",
    "DoclingParser",
    "DoclingMetadata",
];
