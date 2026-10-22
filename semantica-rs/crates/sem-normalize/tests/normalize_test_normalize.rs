//! Port of `tests/normalize/test_normalize.py` (`TestTextNormalizer`, `TestTextCleaner`).
#![allow(non_snake_case)]

use sem_normalize::text_cleaner::CleanOptions;
use sem_normalize::text_normalizer::NormalizeTextOptions;
use sem_normalize::{TextCleaner, TextNormalizer};

#[test]
fn test_text_normalizer__test_normalize_text_case() {
    let normalizer = TextNormalizer::default();
    let text = "Hello World";
    let with = |case: &str| {
        normalizer
            .normalize_text(text, &NormalizeTextOptions::with_case(case))
            .unwrap()
    };
    assert_eq!(with("lower"), "hello world");
    assert_eq!(with("upper"), "HELLO WORLD");
    assert_eq!(with("preserve"), "Hello World");
    assert_eq!(with("title"), "Hello World");
}

#[test]
fn test_text_normalizer__test_normalize_unicode_integration() {
    let normalizer = TextNormalizer::default();
    // e + combining acute accent
    let options = NormalizeTextOptions {
        unicode_form: "NFC".to_string(),
        ..NormalizeTextOptions::default()
    };
    let normalized = normalizer.normalize_text("e\u{301}", &options).unwrap();
    assert_eq!(normalized, "\u{e9}");
}

#[test]
fn test_text_normalizer__test_process_special_chars_integration() {
    let normalizer = TextNormalizer::default();
    // En dash
    let processed = normalizer
        .normalize_text("Hello\u{2013}World", &NormalizeTextOptions::default())
        .unwrap();
    assert_eq!(processed, "Hello-World");
}

#[test]
fn test_text_normalizer__test_component_access() {
    let normalizer = TextNormalizer::default();
    let normalized = normalizer
        .unicode_normalizer
        .normalize_unicode(Some("e\u{301}"), "NFC")
        .unwrap();
    assert_eq!(normalized, "\u{e9}");
}

#[test]
fn test_text_cleaner__test_clean_html() {
    let cleaner = TextCleaner::default();
    let cleaned = cleaner.clean(
        "<p>Hello <b>World</b></p>",
        &CleanOptions {
            remove_html: true,
            ..CleanOptions::default()
        },
    );
    assert_eq!(cleaned.trim(), "Hello World");
}

#[test]
fn test_text_cleaner__test_clean_whitespace() {
    let cleaner = TextCleaner::default();
    let cleaned = cleaner.clean(
        "Hello    World\n\n",
        &CleanOptions {
            normalize_whitespace: true,
            remove_html: false,
            ..CleanOptions::default()
        },
    );
    assert_eq!(cleaned, "Hello World");
}

#[test]
fn test_text_cleaner__test_clean_unicode() {
    let cleaner = TextCleaner::default();
    let cleaned = cleaner.clean(
        "e\u{301}",
        &CleanOptions {
            normalize_unicode: true,
            ..CleanOptions::default()
        },
    );
    assert_eq!(cleaned, "\u{e9}");
}
