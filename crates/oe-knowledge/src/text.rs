//! Existing catalog search normalization; native text is never normalized in place.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

pub fn normalize(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '−' | '–' | '—' | '‐' | '‑' => '-',
            other => other,
        })
        .collect()
}

pub fn tokens(text: &str) -> Vec<String> {
    let text = normalize(text);
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i].is_ascii_lowercase() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            while i + 1 < bytes.len()
                && matches!(bytes[i], b'-' | b'_' | b'/')
                && bytes[i + 1].is_ascii_alphanumeric()
            {
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
            }
            let word = &text[start..i];
            result.push(word.to_owned());
            let prefix = word.bytes().take_while(u8::is_ascii_lowercase).count();
            if prefix >= 2 && prefix < word.len() {
                result.push(word[..prefix].to_owned());
            }
        } else if bytes[i].is_ascii_digit() {
            let mut end = i + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end + 1 < bytes.len() && bytes[end] == b'.' && bytes[end + 1].is_ascii_digit() {
                end += 2;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            let letters = end;
            while end < bytes.len() && bytes[end].is_ascii_lowercase() {
                end += 1;
            }
            if end > letters {
                result.push(text[start..end].to_owned());
                i = end;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        if !cjk(chars[start]) {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < chars.len() && cjk(chars[end]) {
            end += 1;
        }
        let run = &chars[start..end];
        if run.len() == 1 {
            result.push(run.iter().collect());
        } else {
            for size in [2, 3] {
                for part in run.windows(size) {
                    result.push(part.iter().collect());
                }
            }
        }
        start = end;
    }
    result
}

fn cjk(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}')
}

pub fn match_expression(query: &str) -> String {
    let mut seen = BTreeSet::new();
    tokens(query)
        .into_iter()
        .filter(|word| seen.insert(word.clone()))
        .take(160)
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Python-compatible character offsets, not UTF-8 bytes or UTF-16 code units.
pub fn char_slice(body: &str, start: usize, end: usize) -> Result<String> {
    let boundaries: Vec<usize> = body
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(body.len()))
        .collect();
    ensure!(
        start <= end && end < boundaries.len(),
        "invalid_character_span"
    );
    Ok(body[boundaries[start]..boundaries[end]].to_owned())
}

pub fn excerpt(body: &str, query: &str, length: usize) -> Value {
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= length {
        return json!({"text":body,"start":0,"end":chars.len()});
    }
    let terms: BTreeSet<String> = tokens(query).into_iter().collect();
    let mut best = (0usize, 0usize);
    for start in (0..chars.len()).step_by((length / 2).max(1)) {
        let window: String = chars[start..(start + length).min(chars.len())]
            .iter()
            .collect();
        let words: BTreeSet<String> = tokens(&window).into_iter().collect();
        let score = terms.intersection(&words).count();
        if score > best.1 {
            best = (start, score);
        }
    }
    let end = (best.0 + length).min(chars.len());
    json!({"text":chars[best.0..end].iter().collect::<String>(),"start":best.0,"end":end})
}
