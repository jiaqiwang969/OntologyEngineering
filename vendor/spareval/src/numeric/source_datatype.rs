//! Fixed original constructor catalogue. Unmapped datatypes have a None
//! cache; recognized but unreceived converters remain an explicit boundary.
//! Dynamic rdflib.term.bind()/custom converter mutation is a separate protocol.
use super::*;

pub(super) fn has_registered_converter(datatype: &NamedNode) -> bool {
    matches!(
        datatype.as_str(),
        "http://www.w3.org/2001/XMLSchema#time"
            | "http://www.w3.org/2001/XMLSchema#date"
            | "http://www.w3.org/2001/XMLSchema#dateTime"
            | "http://www.w3.org/2001/XMLSchema#duration"
            | "http://www.w3.org/2001/XMLSchema#dayTimeDuration"
            | "http://www.w3.org/2001/XMLSchema#yearMonthDuration"
            | "http://www.w3.org/2001/XMLSchema#hexBinary"
            | "http://www.w3.org/2001/XMLSchema#string"
            | "http://www.w3.org/2001/XMLSchema#normalizedString"
            | "http://www.w3.org/2001/XMLSchema#token"
            | "http://www.w3.org/2001/XMLSchema#language"
            | "http://www.w3.org/2001/XMLSchema#boolean"
            | "http://www.w3.org/2001/XMLSchema#decimal"
            | "http://www.w3.org/2001/XMLSchema#integer"
            | "http://www.w3.org/2001/XMLSchema#nonPositiveInteger"
            | "http://www.w3.org/2001/XMLSchema#long"
            | "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
            | "http://www.w3.org/2001/XMLSchema#negativeInteger"
            | "http://www.w3.org/2001/XMLSchema#int"
            | "http://www.w3.org/2001/XMLSchema#unsignedLong"
            | "http://www.w3.org/2001/XMLSchema#positiveInteger"
            | "http://www.w3.org/2001/XMLSchema#short"
            | "http://www.w3.org/2001/XMLSchema#unsignedInt"
            | "http://www.w3.org/2001/XMLSchema#byte"
            | "http://www.w3.org/2001/XMLSchema#unsignedShort"
            | "http://www.w3.org/2001/XMLSchema#unsignedByte"
            | "http://www.w3.org/2001/XMLSchema#float"
            | "http://www.w3.org/2001/XMLSchema#double"
            | "http://www.w3.org/2001/XMLSchema#base64Binary"
            | "http://www.w3.org/2001/XMLSchema#anyURI"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML"
    )
}

pub(super) fn is_relational_xsd(datatype: &NamedNode) -> bool {
    numeric_kind(datatype).is_some()
        || matches!(
            datatype.as_str(),
            "http://www.w3.org/2001/XMLSchema#string"
                | "http://www.w3.org/2001/XMLSchema#boolean"
                | "http://www.w3.org/2001/XMLSchema#dateTime"
                | "http://www.w3.org/2001/XMLSchema#date"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn source_relational_value_catalogue() {
        let bytes = include_str!("../eval/fixtures/source_datatype_catalogue_01.json");
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes.as_bytes())),
            "94bdeb9ed6256761b48248d2ccefbe07b8a7e06d2bda1608a789479d36f66ff7"
        );
        let original: serde_json::Value = serde_json::from_str(bytes).unwrap();
        assert_eq!(original["candidate_results_used"], false);
        let allowed = original["xsd_datatypes"].as_array().unwrap();
        assert_eq!(allowed.len(), 20);
        for entry in original["mappings"].as_array().unwrap() {
            let Some(iri) = entry["datatype"].as_str() else {
                continue;
            };
            let dt = NamedNode::new(iri).unwrap();
            assert!(has_registered_converter(&dt), "{iri}");
            assert_eq!(
                is_relational_xsd(&dt),
                allowed.iter().any(|v| v == iri),
                "{iri}"
            );
        }
        for iri in ["urn:d", "http://www.w3.org/2001/XMLSchema#unregistered"] {
            let dt = NamedNode::new(iri).unwrap();
            assert!(!has_registered_converter(&dt));
            assert!(!is_relational_xsd(&dt));
        }
    }
}
