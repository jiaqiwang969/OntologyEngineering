//! Source-bound engineering knowledge under the shared project and semantic contracts.
//! Retrieval and model judgments never establish project facts or semantic authority.

pub mod citations;
pub mod context;
pub mod continuity;
pub mod conversation;
pub mod feedback;
pub mod incad;
pub mod index;
pub mod library_audit;
pub mod packet;
pub mod policy;
pub mod poppler;
pub mod premise_advisor;
pub(crate) mod publication;
pub mod query;
pub mod registration;
pub mod registration_freshness;
pub mod retrieval;
pub mod source_library;
pub mod source_pack;
pub mod sources;
pub mod supplier_index;
pub mod supplier_library;
pub mod text;
pub mod windows;

/// Selected source content and explicit observed local rendering;noOCR/model/adoption.
pub mod page_content4;

/// Same-owner opaque actual OCR source observation; no model or Project adoption.
pub mod ocr_page5;
pub mod registered_ocr6;
pub mod typed_source8;
