//! Port of `tests/normalize/test_text_normalizer.py` (`TestTextNormalizer`,
//! `TestUnicodeNormalizer`, `TestWhitespaceNormalizer`, `TestSpecialCharacterProcessor`).
//!
//! The original `TestTextNormalizer` patches the progress tracker and `TextCleaner` with
//! `MagicMock`s (and some tests replace engine methods). The port injects the same seams: a
//! fresh progress tracker whose display records every update, and recording fakes of the
//! engine traits.
#![allow(non_snake_case)]

mod common;

use std::sync::{Arc, Mutex};

use sem_normalize::text_cleaner::TextCleaning;
use sem_normalize::text_normalizer::{
    NormalizeTextOptions, SpecialCharacterProcessing, UnicodeNormalizing, WhitespaceNormalizing,
};
use sem_normalize::{
    SpecialCharacterProcessor, TextNormalizer, UnicodeNormalizer, WhitespaceNormalizer,
};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

use common::{CaptureDisplay, capture_tracker, kw};

/// The calls a fake engine received.
type CleanCalls = Arc<Mutex<Vec<(Option<String>, Map<String, Value>)>>>;

/// A `MagicMock` text cleaner: records `clean` calls and returns a fixed value.
#[derive(Default)]
struct MockCleaner {
    calls: CleanCalls,
    return_value: String,
}

impl TextCleaning for MockCleaner {
    fn clean(
        &self,
        text: Option<&str>,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        self.calls
            .lock()
            .unwrap()
            .push((text.map(str::to_string), options.clone()));
        Ok(self.return_value.clone())
    }
}

/// The `setUp` of `TestTextNormalizer`: a normalizer with a mock cleaner and a recording
/// progress tracker.
fn setup() -> (TextNormalizer, Arc<CaptureDisplay>, CleanCalls) {
    let mut normalizer = TextNormalizer::default();
    let (tracker, display) = capture_tracker();
    normalizer.progress_tracker = tracker;
    let cleaner = MockCleaner::default();
    let calls = cleaner.calls.clone();
    normalizer.text_cleaner = Box::new(cleaner);
    (normalizer, display, calls)
}

fn statuses(display: &CaptureDisplay) -> Vec<String> {
    display
        .updates
        .lock()
        .unwrap()
        .iter()
        .map(|i| i.status.clone())
        .collect()
}

#[test]
fn test_text_normalizer__test_init() {
    let (normalizer, _, _) = setup();
    // The cleaner and the three engines are set up by the constructor.
    let built = TextNormalizer::default();
    assert_eq!(
        built
            .text_cleaner
            .clean(Some("<b>x</b>"), &Map::new())
            .unwrap(),
        "x"
    );
    assert_eq!(
        normalizer
            .unicode_normalizer
            .normalize_unicode(Some("x"), "NFC")
            .unwrap(),
        "x"
    );
    assert_eq!(
        normalizer
            .whitespace_normalizer
            .normalize_whitespace(Some("x"), "unix", &Map::new())
            .unwrap(),
        "x"
    );
    assert_eq!(
        normalizer
            .special_char_processor
            .process_special_chars("x", false, &Map::new())
            .unwrap(),
        "x"
    );
    assert!(normalizer.progress_tracker.enabled());
    assert!(built.progress_tracker.enabled());
}

#[test]
fn test_text_normalizer__test_normalize_text_basic() {
    let (normalizer, display, _) = setup();
    let result = normalizer
        .normalize_text("Hello World", &NormalizeTextOptions::default())
        .unwrap();
    assert_eq!(result, "Hello World");
    // start_tracking was called, and stop_tracking(<its id>, status="completed").
    assert_eq!(statuses(&display), vec!["running", "completed"]);
    assert!(normalizer.progress_tracker.active_items().is_empty());
    let items = normalizer.progress_tracker.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, "completed");
    assert_eq!(items[0].message, "");
}

#[test]
fn test_text_normalizer__test_normalize_empty_string() {
    let (normalizer, _, _) = setup();
    let options = NormalizeTextOptions::default();
    assert_eq!(normalizer.normalize_text("", &options).unwrap(), "");
    assert_eq!(normalizer.normalize_text(None, &options).unwrap(), "");
}

#[test]
fn test_text_normalizer__test_normalize_case_options() {
    let (normalizer, _, _) = setup();
    let text = "HeLLo WoRLd";
    let with = |case: &str| {
        normalizer
            .normalize_text(text, &NormalizeTextOptions::with_case(case))
            .unwrap()
    };
    assert_eq!(with("lower"), "hello world");
    assert_eq!(with("upper"), "HELLO WORLD");
    assert_eq!(with("title"), "Hello World");
    // preserve (the default)
    assert_eq!(with("preserve"), "HeLLo WoRLd");
}

type Calls = Arc<Mutex<Vec<Value>>>;

struct FakeUnicode {
    calls: Calls,
    result: Result<String, SemanticaError>,
}

impl UnicodeNormalizing for FakeUnicode {
    fn normalize_unicode(&self, text: Option<&str>, form: &str) -> Result<String, SemanticaError> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"text": text, "form": form}));
        self.result.clone()
    }
}

struct FakeWhitespace {
    calls: Calls,
}

impl WhitespaceNormalizing for FakeWhitespace {
    fn normalize_whitespace(
        &self,
        text: Option<&str>,
        line_break_type: &str,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"text": text, "line_break_type": line_break_type, "options": options}));
        Ok("W".to_string())
    }
}

struct FakeSpecial {
    calls: Calls,
}

impl SpecialCharacterProcessing for FakeSpecial {
    fn process_special_chars(
        &self,
        text: &str,
        normalize_diacritics: bool,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        self.calls.lock().unwrap().push(
            json!({"text": text, "normalize_diacritics": normalize_diacritics, "options": options}),
        );
        Ok("S".to_string())
    }
}

#[test]
fn test_text_normalizer__test_normalize_delegation() {
    let (mut normalizer, _, _) = setup();
    let unicode_calls = Calls::default();
    let whitespace_calls = Calls::default();
    let special_calls = Calls::default();
    normalizer.unicode_normalizer = Box::new(FakeUnicode {
        calls: unicode_calls.clone(),
        result: Ok("U".to_string()),
    });
    normalizer.whitespace_normalizer = Box::new(FakeWhitespace {
        calls: whitespace_calls.clone(),
    });
    normalizer.special_char_processor = Box::new(FakeSpecial {
        calls: special_calls.clone(),
    });

    let result = normalizer
        .normalize_text(
            "input",
            &NormalizeTextOptions {
                unicode_form: "NFD".to_string(),
                line_break_type: "windows".to_string(),
                normalize_diacritics: true,
                ..NormalizeTextOptions::default()
            },
        )
        .unwrap();

    // assert_called_with("input", form="NFD")
    assert_eq!(
        unicode_calls.lock().unwrap().last().unwrap(),
        &json!({"text": "input", "form": "NFD"})
    );
    // assert_called_with("U", line_break_type="windows")
    assert_eq!(
        whitespace_calls.lock().unwrap().last().unwrap(),
        &json!({"text": "U", "line_break_type": "windows", "options": {}})
    );
    // assert_called_with("W", normalize_diacritics=True)
    assert_eq!(
        special_calls.lock().unwrap().last().unwrap(),
        &json!({"text": "W", "normalize_diacritics": true, "options": {}})
    );
    assert_eq!(result, "S");
}

#[test]
fn test_text_normalizer__test_clean_text() {
    let (mut normalizer, _, _) = setup();
    let text = "<html>body</html>";
    let cleaner = MockCleaner {
        return_value: "body".to_string(),
        ..MockCleaner::default()
    };
    let calls = cleaner.calls.clone();
    normalizer.text_cleaner = Box::new(cleaner);
    let result = normalizer
        .clean_text(text, &kw(json!({"remove_html": true})))
        .unwrap();
    // assert_called_with(text, remove_html=True)
    assert_eq!(
        calls.lock().unwrap().last().unwrap(),
        &(Some(text.to_string()), kw(json!({"remove_html": true})))
    );
    assert_eq!(result, "body");
}

#[test]
fn test_text_normalizer__test_standardize_format() {
    let (normalizer, _, _) = setup();
    let text = "  one   two  ";
    assert_eq!(
        normalizer.standardize_format(text, Some("compact")),
        "one two"
    );
    assert_eq!(
        normalizer.standardize_format(text, Some("preserve")),
        "one   two"
    );
}

#[test]
fn test_text_normalizer__test_process_batch() {
    let (normalizer, _, _) = setup();
    let results = normalizer
        .process_batch(
            &["TEST 1", "Test 2"],
            &NormalizeTextOptions::with_case("lower"),
        )
        .unwrap();
    assert_eq!(results, vec!["test 1", "test 2"]);
}

#[test]
fn test_text_normalizer__test_normalize_overloaded_method() {
    let (normalizer, _, _) = setup();
    let lower = NormalizeTextOptions::with_case("lower");
    assert_eq!(
        normalizer.normalize(&json!("TEST"), &lower).unwrap(),
        json!("test")
    );

    let docs = json!([
        {"id": 1, "content": "DOC 1"},
        {"id": 2, "content": "DOC 2", "other": "meta"},
        {"id": 3, "nocontent": "skip"}
    ]);
    let results = normalizer.normalize(&docs, &lower).unwrap();
    assert_eq!(results[0]["content"], json!("doc 1"));
    assert_eq!(results[1]["content"], json!("doc 2"));
    assert_eq!(results[1]["other"], json!("meta"));
    // assertIn("skip", results[2]): the third document became its normalized str().
    assert!(results[2].as_str().unwrap().contains("skip"));
}

#[test]
fn test_text_normalizer__test_normalize_error_handling() {
    let (mut normalizer, display, _) = setup();
    normalizer.unicode_normalizer = Box::new(FakeUnicode {
        calls: Calls::default(),
        result: Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "Exception".to_string(),
            message: "Test Error".to_string(),
        }),
    });
    assert!(
        normalizer
            .normalize_text("input", &NormalizeTextOptions::default())
            .is_err()
    );
    // stop_tracking(<id>, status="failed", message="Test Error")
    let updates = display.updates.lock().unwrap();
    let last = updates.last().unwrap();
    assert_eq!(last.status, "failed");
    assert_eq!(last.message, "Test Error");
    assert!(normalizer.progress_tracker.active_items().is_empty());
}

#[test]
fn test_unicode_normalizer__test_normalize_unicode_forms() {
    let normalizer = UnicodeNormalizer::default();
    let text_nfc = "\u{e9}";
    let text_nfd = "\u{65}\u{301}";
    assert_eq!(
        normalizer.normalize_unicode(text_nfd, Some("NFC")),
        text_nfc
    );
    assert_eq!(
        normalizer.normalize_unicode(text_nfc, Some("NFD")),
        text_nfd
    );
}

#[test]
fn test_unicode_normalizer__test_normalize_none() {
    let normalizer = UnicodeNormalizer::default();
    assert_eq!(normalizer.normalize_unicode(None, None), "");
    assert_eq!(normalizer.normalize_unicode("", None), "");
}

#[test]
fn test_unicode_normalizer__test_normalize_failure_fallback() {
    // Python patches unicodedata.normalize to raise; in the port normalization fails for an
    // unknown form, which takes the same fallback path.
    let normalizer = UnicodeNormalizer::default();
    assert_eq!(normalizer.normalize_unicode("test", Some("BOOM")), "test");
}

#[test]
fn test_unicode_normalizer__test_handle_encoding() {
    let normalizer = UnicodeNormalizer::default();
    assert_eq!(normalizer.handle_encoding("test", "utf-8", None), "test");
    // bytes in
    assert_eq!(normalizer.handle_encoding(b"test", "utf-8", None), "test");
    // cross encoding
    let latin_bytes: Vec<u8> = "café".chars().map(|c| c as u32 as u8).collect();
    assert_eq!(
        normalizer.handle_encoding(latin_bytes, "latin-1", Some("utf-8")),
        "café"
    );
    // broken bytes still give a string
    let result: String = normalizer.handle_encoding(b"\xff", "utf-8", None);
    assert_eq!(result, "\u{fffd}");
}

#[test]
fn test_unicode_normalizer__test_process_special_chars_replacement() {
    let normalizer = UnicodeNormalizer::default();
    let input = "\u{2018}single\u{2019} \u{201C}double\u{201D} \u{2013} \u{2014} \u{2026}";
    assert_eq!(
        normalizer.process_special_chars(input),
        "'single' \"double\" - -- ..."
    );
}

#[test]
fn test_whitespace_normalizer__test_normalize_whitespace_basic() {
    let normalizer = WhitespaceNormalizer::default();
    assert_eq!(
        normalizer.normalize_whitespace("Hello   World\tTest", None),
        "Hello World Test"
    );
}

#[test]
fn test_whitespace_normalizer__test_handle_line_breaks() {
    let normalizer = WhitespaceNormalizer::default();
    assert_eq!(
        normalizer.handle_line_breaks("Row1\r\nRow2\rRow3\n", Some("unix")),
        "Row1\nRow2\nRow3\n"
    );
    assert_eq!(
        normalizer.handle_line_breaks("Row1\nRow2", Some("windows")),
        "Row1\r\nRow2"
    );
}

#[test]
fn test_whitespace_normalizer__test_process_indentation() {
    let normalizer = WhitespaceNormalizer::default();
    assert_eq!(
        normalizer.process_indentation("    Code", Some("tabs")),
        "\tCode"
    );
    assert_eq!(
        normalizer.process_indentation("\tCode", Some("spaces")),
        "    Code"
    );
}

#[test]
fn test_special_character_processor__test_normalize_punctuation() {
    let processor = SpecialCharacterProcessor::default();
    assert_eq!(
        processor
            .normalize_punctuation("\u{201c}Hello\u{201d} \u{2018}World\u{2019} \u{2013} \u{2026}"),
        "\"Hello\" 'World' - ..."
    );
}

#[test]
fn test_special_character_processor__test_process_diacritics_remove() {
    let processor = SpecialCharacterProcessor::default();
    assert_eq!(
        processor.process_diacritics("Crème Brûlée", true),
        "Creme Brulee"
    );
}

#[test]
fn test_special_character_processor__test_process_diacritics_normalize() {
    let processor = SpecialCharacterProcessor::default();
    assert_eq!(processor.process_diacritics("e\u{301}", false), "\u{e9}");
}

#[test]
fn test_special_character_processor__test_process_special_chars_integration() {
    let processor = SpecialCharacterProcessor::default();
    let result = processor.process_special_chars(
        "\u{201c}Crème\u{201d}",
        true,
        &kw(json!({"remove_diacritics": true})),
    );
    assert_eq!(result, "\"Creme\"");
}
