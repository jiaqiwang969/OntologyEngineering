//! Port of `tests/normalize/test_language_detector.py` (`TestLanguageDetector`).
//!
//! `test_detect_language` and `test_detect_with_confidence` are `skipUnless(
//! LANGDETECT_AVAILABLE)`: langdetect was not installed in the original environment, so they
//! were skipped there. The port always has a detection backend (whatlang), so they run.
#![allow(non_snake_case)]

use sem_normalize::LanguageDetector;
use sem_normalize::language_detector::LANGDETECT_AVAILABLE;

#[test]
fn test_language_detector__test_detect_language() {
    // @unittest.skipUnless(LANGDETECT_AVAILABLE, ...)
    if !LANGDETECT_AVAILABLE {
        return;
    }
    let detector = LanguageDetector::default();
    // English
    assert_eq!(detector.detect("This is a simple English sentence."), "en");
    // French
    assert_eq!(
        detector.detect("Ceci est une phrase française simple."),
        "fr"
    );
    // German
    assert_eq!(
        detector.detect("Dies ist ein einfacher deutscher Satz."),
        "de"
    );
}

#[test]
fn test_language_detector__test_detect_short_text() {
    let detector = LanguageDetector::default();
    // Very short text gives the default language.
    assert_eq!(detector.detect("Hi"), "en");
}

#[test]
fn test_language_detector__test_detect_with_confidence() {
    // @unittest.skipUnless(LANGDETECT_AVAILABLE, ...)
    if !LANGDETECT_AVAILABLE {
        return;
    }
    let detector = LanguageDetector::default();
    let (lang, conf) = detector.detect_with_confidence("This is definitely an English sentence.");
    assert_eq!(lang, "en");
    assert!(conf > 0.5, "{conf}");
}

#[test]
fn test_language_detector__test_get_language_name() {
    let detector = LanguageDetector::default();
    assert_eq!(detector.get_language_name("en"), "English");
    assert_eq!(detector.get_language_name("fr"), "French");
    assert_eq!(detector.get_language_name("xx"), "XX");
}
