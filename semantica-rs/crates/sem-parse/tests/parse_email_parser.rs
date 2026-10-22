//! Characterisation tests for `email_parser` (`semantica/parse/email_parser.py`) and the
//! port of Python's `email` package behind it. Every expected value comes from running the
//! original in the oracle environment (`tests/fixtures/email/generate_oracle.py`).

mod common;

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use common::{err_json, fixture, load_oracle, outcome, quiet_progress};
use sem_parse::email_parser::{
    DecodedPart, EmailAttachment, EmailBody, EmailData, EmailHeaderValue, EmailHeaders,
    EmailMessage, EmailParser, EmailSource, EmailThreadAnalyzer, MIMEParser, email_decode_header,
    email_parsedate_to_datetime, encode_surrogateescape, has_surrogates, message_from_bytes,
    message_from_string,
};
use sem_parse::{Options, email_parser};

fn oracle() -> Value {
    quiet_progress();
    load_oracle("email")
}

fn email_dir() -> PathBuf {
    fixture("email")
}

fn data_json(data: &EmailData) -> Value {
    serde_json::to_value(data).expect("EmailData serialises")
}

fn result_json(result: Result<EmailData, sem_utils::SemanticaError>) -> Value {
    match result {
        Ok(d) => json!({"ok": data_json(&d)}),
        Err(e) => err_json(&e),
    }
}

/// A `str` that may carry surrogate escapes, as the generator records it.
fn strj(s: Option<&str>) -> Value {
    match s {
        None => Value::Null,
        Some(s) if has_surrogates(s) => {
            json!({"bytes": encode_surrogateescape(s).expect("escaped bytes")})
        }
        Some(s) => Value::String(s.to_string()),
    }
}

/// The generator's `describe(message)`.
fn describe(m: &EmailMessage) -> Value {
    let items = m.items();
    let headers: Vec<Value> = items
        .iter()
        .map(|(k, v)| json!([k, v.to_string()]))
        .collect();
    let kinds: Vec<Value> = items
        .iter()
        .map(|(_, v)| match v {
            EmailHeaderValue::Header(_) => json!("header"),
            EmailHeaderValue::Str(_) => json!("str"),
        })
        .collect();
    let mut d = Map::new();
    d.insert("content_type".into(), json!(m.get_content_type()));
    d.insert("default_type".into(), json!(m.get_default_type()));
    d.insert("is_multipart".into(), json!(m.is_multipart()));
    d.insert("unixfrom".into(), strj(m.get_unixfrom()));
    d.insert("headers".into(), Value::Array(headers));
    d.insert("header_kinds".into(), Value::Array(kinds));
    d.insert("preamble".into(), strj(m.preamble.as_deref()));
    d.insert("epilogue".into(), strj(m.epilogue.as_deref()));
    d.insert("defects".into(), json!(m.defects));
    d.insert("charset".into(), outcome(m.get_content_charset()));
    d.insert("filename".into(), outcome(m.get_filename()));
    d.insert("boundary".into(), outcome(m.get_boundary()));
    d.insert(
        "payload_text".into(),
        if m.is_multipart() {
            json!({"ok": null})
        } else {
            outcome(m.get_payload_text())
        },
    );
    d.insert("payload_decoded".into(), json!(m.get_payload_decoded()));
    d.insert(
        "parts".into(),
        if m.is_multipart() {
            Value::Array(m.parts().iter().map(describe).collect())
        } else {
            Value::Null
        },
    );
    Value::Object(d)
}

fn no_attachments() -> Options {
    let mut o = Options::new();
    o.insert("extract_attachments".into(), json!(false));
    o
}

#[test]
fn test_parse_email_file_corpus_matches_python() {
    let oracle = oracle();
    let parser = EmailParser::default();
    let files = oracle["files"].as_object().expect("files");
    assert!(files.len() >= 40);
    for (name, expected) in files {
        let path = email_dir().join(name);
        let path_text = path.to_str().expect("utf-8 path");
        let got = result_json(parser.parse_email(path_text, &Options::new()));
        assert_eq!(got, expected["default"], "{name} (default options)");
        let got = result_json(parser.parse_email(path.clone(), &no_attachments()));
        assert_eq!(
            got, expected["no_attachments"],
            "{name} (extract_attachments=False)"
        );
    }
}

#[test]
fn test_message_structure_matches_python() {
    let oracle = oracle();
    let files = oracle["files"].as_object().expect("files");
    for (name, expected) in files {
        let bytes = std::fs::read(email_dir().join(name)).expect("fixture");
        let got = match message_from_bytes(&bytes) {
            Ok(m) => json!({"ok": describe(&m)}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, expected["structure_bytes"], "{name}");
    }
}

#[test]
fn test_parse_email_strings_match_python() {
    let oracle = oracle();
    let parser = EmailParser::new(None, Options::new());
    for (name, case) in oracle["strings"].as_object().expect("strings") {
        let input = case["input"].as_str().expect("input");
        let got = result_json(parser.parse_email(input, &Options::new()));
        assert_eq!(got, case["result"], "{name}");
        let structure = match message_from_string(input) {
            Ok(m) => json!({"ok": describe(&m)}),
            Err(e) => err_json(&e),
        };
        assert_eq!(structure, case["structure"], "{name} structure");
    }
}

#[test]
fn test_parse_email_bytes_match_python() {
    let oracle = oracle();
    let parser = EmailParser::default();
    for (name, case) in oracle["bytes"].as_object().expect("bytes") {
        let input: Vec<u8> = case["input"]
            .as_array()
            .expect("input")
            .iter()
            .map(|v| v.as_u64().expect("byte") as u8)
            .collect();
        let got = result_json(parser.parse_email(input.clone(), &Options::new()));
        assert_eq!(got, case["result"], "{name}");
        let got = result_json(parser.parse_email(input.as_slice(), &Options::new()));
        assert_eq!(got, case["result"], "{name} (slice)");
    }
}

#[test]
fn test_parse_email_missing_and_directory_paths() {
    let oracle = oracle();
    let parser = EmailParser::default();
    let dir = email_dir();
    let dir_text = dir
        .canonicalize()
        .expect("fixture dir")
        .display()
        .to_string();
    let placeholder = |e: &sem_utils::SemanticaError| {
        let mut v = err_json(e);
        let msg = v["message"]
            .as_str()
            .expect("message")
            .replace(&dir_text, "<DIR>");
        v["message"] = json!(msg);
        v
    };
    let missing = dir.canonicalize().unwrap().join("does-not-exist.eml");
    let err = parser
        .parse_email(missing, &Options::new())
        .expect_err("missing file");
    assert_eq!(placeholder(&err), oracle["missing_path"]);
    let err = parser
        .parse_email(dir.canonicalize().unwrap(), &Options::new())
        .expect_err("directory");
    assert_eq!(placeholder(&err), oracle["directory_path"]);
}

#[test]
fn test_extract_headers_and_body_match_python() {
    let oracle = oracle();
    let parser = EmailParser::default();
    for name in [
        "simple.eml",
        "encoded_subjects.eml",
        "mixed_attachments.eml",
        "folded.eml",
        "odd_casing.eml",
        "duplicates.eml",
    ] {
        let bytes = std::fs::read(email_dir().join(name)).expect("fixture");
        let message = message_from_bytes(&bytes).expect("parse");
        let expected = &oracle["files"][name]["default"]["ok"];
        let headers = parser.extract_headers(&message).expect("headers");
        assert_eq!(
            serde_json::to_value(&headers).unwrap(),
            expected["headers"],
            "{name}"
        );
        let body = parser.extract_body(&message, None).expect("body");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            expected["body"],
            "{name}"
        );
        let body = parser.extract_body(&message, Some(false)).expect("body");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            oracle["files"][name]["no_attachments"]["ok"]["body"],
            "{name}"
        );
    }
}

#[test]
fn test_analyze_thread_matches_python() {
    let oracle = oracle();
    let parser = EmailParser::default();
    let inputs: Vec<String> = oracle["thread"]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let emails: Vec<EmailData> = inputs
        .iter()
        .map(|n| {
            parser
                .parse_email(email_dir().join(n), &Options::new())
                .expect("parse")
        })
        .collect();
    for analysis in [
        parser.analyze_thread(&emails),
        EmailThreadAnalyzer::default().analyze_thread(&emails),
    ] {
        let mut analysis = analysis;
        // Python collects participants in a set (unordered); compare sorted.
        let mut participants: Vec<String> = analysis["participants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let first_seen = participants.clone();
        participants.sort();
        analysis.insert("participants".into(), json!(participants));
        assert_eq!(Value::Object(analysis), oracle["thread"]["analysis"]);
        // The port keeps first-seen order.
        assert_eq!(first_seen[0], "Alice <alice@example.com>");
    }
    assert_eq!(
        Value::Object(EmailThreadAnalyzer::new(Options::new()).analyze_thread(&[])),
        oracle["thread_empty"]
    );
}

#[test]
fn test_mime_parser_matches_python() {
    let oracle = oracle();
    let mp = MIMEParser::default();
    let text = mp
        .parse_mime_message("Subject: m\nContent-Type: text/html\n\n<b>x</b>\n")
        .expect("text");
    assert_eq!(
        json!({"ok": describe(&text)}),
        oracle["mime_parser"]["text"]
    );
    let bytes = mp
        .parse_mime_message(b"Subject: m\n\nbody \xe9\n".as_slice())
        .expect("bytes");
    assert_eq!(
        json!({"ok": describe(&bytes)}),
        oracle["mime_parser"]["bytes"]
    );
    let err = mp
        .parse_mime_message(PathBuf::from("x.eml"))
        .expect_err("paths are not messages");
    assert_eq!(err.type_name(), "TypeError");
    assert_eq!(MIMEParser::new(Options::new()).config, Options::new());
}

#[test]
fn test_decode_header_matches_python() {
    let oracle = oracle();
    for case in oracle["decode_header"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let got = match email_decode_header(&EmailHeaderValue::Str(input.to_string())) {
            Ok(parts) => {
                let rows: Vec<Value> = parts
                    .into_iter()
                    .map(|(p, cs)| {
                        let p = match p {
                            DecodedPart::Bytes(b) => json!({"b": b}),
                            DecodedPart::Str(s) => json!({"s": s}),
                        };
                        json!([p, cs])
                    })
                    .collect();
                json!({"input": input, "ok": rows})
            }
            Err(e) => json!({"input": input, "error": e.type_name(), "message": e.to_string()}),
        };
        assert_eq!(&got, case, "{input:?}");
    }
}

#[test]
fn test_parsedate_to_datetime_matches_python() {
    let oracle = oracle();
    for case in oracle["parsedate"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        match email_parsedate_to_datetime(input) {
            Ok(dt) => assert_eq!(json!(dt.isoformat()), case["ok"], "{input:?}"),
            Err(_) => assert!(
                case.get("error").is_some(),
                "{input:?} should parse: {case}"
            ),
        }
    }
}

#[test]
fn test_email_dataclasses() {
    let headers = EmailHeaders {
        subject: Some("S".into()),
        ..EmailHeaders::default()
    };
    let body = EmailBody {
        text: Some("t".into()),
        attachments: vec![EmailAttachment {
            filename: "a.bin".into(),
            content_type: "application/octet-stream".into(),
            size: 2,
            data: Some(vec![1, 2]),
        }],
        ..EmailBody::default()
    };
    let data = EmailData::new(headers.clone(), body.clone(), None);
    assert_eq!(
        data_json(&data),
        json!({
            "headers": {
                "subject": "S", "from_address": null, "to_addresses": [], "cc_addresses": [],
                "bcc_addresses": [], "date": null, "message_id": null, "in_reply_to": null,
                "references": [], "custom_headers": {}
            },
            "body": {
                "text": "t", "html": null,
                "attachments": [{"filename": "a.bin", "content_type": "application/octet-stream",
                                  "size": 2, "data": [1, 2]}]
            },
            "metadata": {}
        })
    );
    assert_eq!(data.headers, headers);
    assert_eq!(data.body, body);
}

#[test]
fn test_email_parser_config() {
    let mut mime_cfg = Options::new();
    mime_cfg.insert("strict".into(), json!(true));
    let mut config = Options::new();
    config.insert("mime".into(), Value::Object(mime_cfg.clone()));
    let mut kwargs = Options::new();
    kwargs.insert("thread".into(), json!({"window": 3}));
    let parser = EmailParser::new(Some(config), kwargs);
    assert_eq!(parser.mime_parser.config, mime_cfg);
    assert_eq!(parser.thread_analyzer.config["window"], json!(3));
    assert_eq!(parser.config.len(), 2);
    assert_eq!(parser.logger.name(), "semantica.email_parser");
    assert_eq!(parser.mime_parser.logger.name(), "semantica.mime_parser");
    assert_eq!(
        parser.thread_analyzer.logger.name(),
        "semantica.email_thread_analyzer"
    );
    // `EmailSource` conversions.
    assert_eq!(EmailSource::from("x"), EmailSource::Text("x".into()));
    assert_eq!(
        EmailSource::from(b"x".as_slice()),
        EmailSource::Bytes(b"x".to_vec())
    );
    let _ = email_parser::EmailParser::default();
}
