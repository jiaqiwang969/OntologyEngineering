//! Characterisation of `text_cleaner.TextCleaner` against the oracle (HTML removal with and
//! without `preserve_structure`, whitespace, Unicode, special characters, sanitizing,
//! trimming, batches).

mod common;

use sem_normalize::TextCleaner;
use sem_normalize::text_cleaner::{BEAUTIFULSOUP_AVAILABLE, CleanOptions};
use serde_json::{Value, json};

use common::{check, obj, oracle, text_opt};

// HTML text extraction (the BeautifulSoup path) is always available.
const _: () = assert!(BEAUTIFULSOUP_AVAILABLE);

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

#[test]
fn test_clean_matches_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("clean") {
        let options = CleanOptions::from_kwargs(&obj(&case["kwargs"]));
        let result = cleaner.clean(text_opt(&case["text"]), &options);
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_remove_html_matches_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("remove_html") {
        let result = cleaner.remove_html(
            case["text"].as_str(),
            case["preserve_structure"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_whitespace_and_unicode_match_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("cleaner_normalize_whitespace") {
        check(
            json!({"ok": cleaner.normalize_whitespace(case["text"].as_str())}),
            case,
        );
    }
    for case in cases("cleaner_normalize_unicode") {
        let result = cleaner.normalize_unicode(case["text"].as_str(), case["form"].as_str());
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_remove_special_chars_matches_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("remove_special_chars") {
        let result = cleaner.remove_special_chars(
            case["text"].as_str(),
            case["allow_spaces"].as_bool().unwrap(),
            case["allow_newlines"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_sanitize_and_trim_match_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("sanitize") {
        let result = cleaner.sanitize(
            case["text"].as_str(),
            case["remove_data_urls"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("trim") {
        let result = cleaner.trim(
            case["text"].as_str(),
            case["remove_empty_lines"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_clean_batch_matches_oracle() {
    let cleaner = TextCleaner::default();
    for case in cases("clean_batch") {
        let texts: Vec<&str> = case["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        let options = CleanOptions::from_kwargs(&obj(&case["kwargs"]));
        check(json!({"ok": cleaner.clean_batch(&texts, &options)}), case);
    }
}
