//! Office Open XML documents as the Python libraries the original parsers use expose them:
//! python-docx 1.2.0 ([`docx`]), python-pptx 1.0.2 ([`pptx`]) and openpyxl 3.1.5 with
//! pandas 3.0.6 `read_excel` ([`xlsx`]), on a shared zip/OPC reader ([`opc`]) and XML tree
//! ([`dom`]).

pub(crate) mod dom;
pub mod docx;
pub(crate) mod opc;
pub(crate) mod pandas;
pub(crate) mod pptx;
pub(crate) mod w3cdtf;
pub(crate) mod xlsx;
