//! The package export surface (`semantica/parse/__init__.py`): `__all__` matches the
//! original (fixtures/core, section "exports") and every exported name resolves.

mod common;

#[allow(unused_imports)]
use sem_parse::{
    CSVData, CSVParser, CodeComment, CodeParser, CodeStructure, CommentExtractor, DOCXParser,
    DependencyAnalyzer, DoclingMetadata, DoclingParser, DocumentParser, DocxMetadata,
    DocxSection, EmailBody, EmailData, EmailHeaders, EmailParser, EmailThreadAnalyzer,
    ExcelData, ExcelParser, ExcelSheet, HTMLContentParser, HTMLElement, HTMLMetadata,
    HTMLParser, ImageMetadata, ImageParser, JSONData, JSONParser, JavaScriptRenderer,
    MCPParser, MIMEParser, MediaParser, MethodRegistry, OCRResult, PDFMetadata, PDFPage,
    PDFParser, PPTXData, PPTXParser, ParseConfig, SlideContent, StructuredDataParser,
    SyntaxTreeParser, WebParser, XMLData, XMLElement, XMLParser, get_parse_method,
    list_available_methods, method_registry, parse_code, parse_config, parse_csv,
    parse_document, parse_docx, parse_email, parse_image, parse_json, parse_media, parse_pdf,
    parse_structured_data, parse_web_content, parse_xml,
};

#[test]
fn test_all_matches_original_exports() {
    let oracle = common::load_oracle("core");
    let expected: Vec<String> = oracle["exports"]["all"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let actual: Vec<String> = sem_parse::__all__.iter().map(|s| s.to_string()).collect();
    assert_eq!(actual, expected);
    assert!(sem_parse::DOCLING_AVAILABLE);
    assert_eq!(sem_parse::DOCLING_IMPORT_ERROR, None);
}
