//! Characterisation of `text_normalizer` (`TextNormalizer`, `UnicodeNormalizer`,
//! `WhitespaceNormalizer`, `SpecialCharacterProcessor`) against the oracle.

mod common;

use sem_normalize::text_normalizer::NormalizeTextOptions;
use sem_normalize::{
    SpecialCharacterProcessor, TextNormalizer, UnicodeNormalizer, WhitespaceNormalizer,
};
use serde_json::{Value, json};

use common::{check, data_arg, obj, oracle, outcome, text_opt};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

#[test]
fn test_normalize_text_matches_oracle() {
    let normalizer = TextNormalizer::default();
    for case in cases("normalize_text") {
        let options = NormalizeTextOptions::from_kwargs(&obj(&case["kwargs"]));
        let result = normalizer.normalize_text(text_opt(&case["text"]), &options);
        check(outcome(result), case);
    }
    assert_eq!(
        outcome(normalizer.normalize_text(None, &NormalizeTextOptions::default())),
        *oracle("normalize_text_none")
    );
}

#[test]
fn test_normalize_matches_oracle() {
    let normalizer = TextNormalizer::default();
    for case in cases("normalize") {
        let options = NormalizeTextOptions::from_kwargs(&obj(&case["kwargs"]));
        check(
            outcome(normalizer.normalize(&case["source"], &options)),
            case,
        );
    }
}

#[test]
fn test_clean_text_standardize_format_and_batch_match_oracle() {
    let normalizer = TextNormalizer::default();
    for case in cases("clean_text") {
        let result = normalizer.clean_text(text_opt(&case["text"]), &obj(&case["kwargs"]));
        check(outcome(result), case);
    }
    for case in cases("standardize_format") {
        let result = normalizer
            .standardize_format(case["text"].as_str().unwrap(), case["format_type"].as_str());
        check(json!({"ok": result}), case);
    }
    for case in cases("process_batch") {
        let texts: Vec<&str> = case["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        let options = NormalizeTextOptions::from_kwargs(&obj(&case["kwargs"]));
        check(outcome(normalizer.process_batch(&texts, &options)), case);
    }
}

#[test]
fn test_unicode_normalizer_matches_oracle() {
    let normalizer = UnicodeNormalizer::default();
    for case in cases("normalize_unicode") {
        let result = normalizer.normalize_unicode(text_opt(&case["text"]), case["form"].as_str());
        check(json!({"ok": result}), case);
    }
    for case in cases("handle_encoding") {
        let result = normalizer.handle_encoding(
            data_arg(&case["data"]),
            case["source"].as_str().unwrap(),
            case["target"].as_str(),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("unicode_process_special_chars") {
        let result = normalizer.process_special_chars(case["text"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_whitespace_normalizer_matches_oracle() {
    let normalizer = WhitespaceNormalizer::default();
    for case in cases("normalize_whitespace") {
        let result = normalizer
            .normalize_whitespace(case["text"].as_str(), case["line_break_type"].as_str());
        check(json!({"ok": result}), case);
    }
    for case in cases("handle_line_breaks") {
        let result = normalizer.handle_line_breaks(
            case["text"].as_str().unwrap(),
            case["line_break_type"].as_str(),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("process_indentation") {
        let result = normalizer
            .process_indentation(case["text"].as_str().unwrap(), case["indent_type"].as_str());
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_special_character_processor_matches_oracle() {
    let processor = SpecialCharacterProcessor::default();
    for case in cases("process_special_chars") {
        let result = processor.process_special_chars(
            case["text"].as_str().unwrap(),
            case["normalize_diacritics"].as_bool().unwrap(),
            &obj(&case["kwargs"]),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("normalize_punctuation") {
        let result = processor.normalize_punctuation(case["text"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
    for case in cases("process_diacritics") {
        let result = processor.process_diacritics(
            case["text"].as_str().unwrap(),
            case["remove"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
}
