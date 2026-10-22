//! Characterisation of `language_detector.LanguageDetector`.
//!
//! langdetect is not installed in the oracle environment, so the original always answers with
//! the default language; a detector without a backend reproduces those recorded answers. With
//! the default (whatlang) backend only the guesses the original tests pin are asserted.

mod common;

use std::sync::Arc;

use sem_normalize::LanguageDetector;
use sem_normalize::language_detector::{
    DetectedLanguage, LANGDETECT_AVAILABLE, LangDetectException, LanguageBackend, WhatlangBackend,
};
use serde_json::{Map, Value, json};

use common::{check, kw, oracle, text_opt};

// The port always has a detection backend.
const _: () = assert!(LANGDETECT_AVAILABLE);

fn without_backend(config: Map<String, Value>) -> LanguageDetector {
    LanguageDetector::with_backend(config, None)
}

#[test]
fn test_detector_without_langdetect_matches_oracle() {
    let expected = oracle("language_unavailable");
    assert_eq!(expected["available"], json!(false));
    let detector = without_backend(Map::new());
    assert!(detector.backend.is_none());
    let detector_fr = without_backend(kw(json!({"default_language": "fr", "min_confidence": 0.0})));
    for case in expected["detect"].as_array().unwrap() {
        check(
            json!({"ok": detector.detect(text_opt(&case["text"]))}),
            case,
        );
    }
    for case in expected["detect_fr"].as_array().unwrap() {
        check(
            json!({"ok": detector_fr.detect(text_opt(&case["text"]))}),
            case,
        );
    }
    for case in expected["detect_with_confidence"].as_array().unwrap() {
        let (lang, conf) = detector.detect_with_confidence(text_opt(&case["text"]));
        check(json!({"ok": [lang, conf]}), case);
    }
    for case in expected["detect_multiple"].as_array().unwrap() {
        let result = detector.detect_multiple(
            text_opt(&case["text"]),
            case["top_n"].as_u64().map(|n| n as usize),
        );
        check(json!({"ok": result}), case);
    }
    let texts = [
        "This is a simple English sentence.",
        "Ceci est une phrase française simple.",
        "Hi",
        "",
    ];
    assert_eq!(
        json!({"ok": detector.detect_batch(&texts)}),
        expected["detect_batch"]
    );
    assert_eq!(
        json!({"ok": detector.detect_batch_with_confidence(&texts)}),
        expected["detect_batch_with_confidence"]
    );
    for case in expected["is_language"].as_array().unwrap() {
        let result = detector.is_language(
            case["text"].as_str().unwrap(),
            case["target"].as_str().unwrap(),
            case["min"].as_f64(),
        );
        check(json!({"ok": result}), case);
    }
    for case in expected["is_language_fr"].as_array().unwrap() {
        let result = detector_fr.is_language(
            case["text"].as_str().unwrap(),
            case["target"].as_str().unwrap(),
            None,
        );
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_get_language_name_matches_oracle() {
    let detector = LanguageDetector::default();
    for case in oracle("get_language_name").as_array().unwrap() {
        check(
            json!({"ok": detector.get_language_name(case["code"].as_str().unwrap())}),
            case,
        );
    }
}

#[test]
fn test_whatlang_backend_gives_the_pinned_guesses() {
    let detector = LanguageDetector::default();
    // The sentences and answers of test_detect_language / test_detect_with_confidence.
    assert_eq!(
        detector.detect_batch(&[
            "This is a simple English sentence.",
            "Ceci est une phrase française simple.",
            "Dies ist ein einfacher deutscher Satz.",
        ]),
        vec!["en", "fr", "de"]
    );
    let sentence = "This is definitely an English sentence.";
    assert!(detector.is_language(sentence, "en", None));
    let multiple = detector.detect_multiple(sentence, Some(3));
    assert_eq!(multiple[0].0, "en");
    assert!(multiple[0].1 > 0.5);
    let batch = detector.detect_batch_with_confidence(&[sentence, "Hi"]);
    assert_eq!(batch[0].0, "en");
    assert_eq!(batch[1], ("en".to_string(), 0.0));
    assert_eq!(
        WhatlangBackend.detect("1234567890 !!!"),
        Err(LangDetectException("No features in text.".to_string()))
    );
    // Undetectable text falls back to the default language.
    assert_eq!(detector.detect("1234567890 !!!"), "en");
}

/// A backend returning fixed candidates (the original tests mock langdetect the same way).
struct Fixed(Vec<DetectedLanguage>);

impl LanguageBackend for Fixed {
    fn detect(&self, _text: &str) -> Result<String, LangDetectException> {
        Ok(self.0[0].lang.clone())
    }

    fn detect_langs(&self, _text: &str) -> Result<Vec<DetectedLanguage>, LangDetectException> {
        Ok(self.0.clone())
    }
}

#[test]
fn test_backend_results_flow_through_like_langdetect() {
    // The oracle injects the same fake langdetect functions into the original module.
    let expected = oracle("language_fake_backend");
    let languages = expected["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| DetectedLanguage {
            lang: pair[0].as_str().unwrap().to_string(),
            prob: pair[1].as_f64().unwrap(),
        })
        .collect();
    let detector = LanguageDetector::with_backend(Map::new(), Some(Arc::new(Fixed(languages))));
    let text = "long enough text";
    assert_eq!(json!({"ok": detector.detect(text)}), expected["detect"]);
    let (lang, conf) = detector.detect_with_confidence(text);
    assert_eq!(
        json!({"ok": [lang, conf]}),
        expected["detect_with_confidence"]
    );
    assert_eq!(
        json!({"ok": detector.detect_multiple(text, Some(2))}),
        expected["detect_multiple"]
    );
    assert_eq!(
        json!({"ok": detector.is_language(text, "fr", None)}),
        expected["is_language_fr"]
    );
    assert_eq!(
        json!({"ok": detector.is_language(text, "en", Some(0.3))}),
        expected["is_language_en"]
    );
    assert_eq!(
        json!({"ok": detector.detect("short")}),
        expected["detect_short"]
    );
}
