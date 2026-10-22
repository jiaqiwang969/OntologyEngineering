//! Exact Unicode ranges for bounded reading; window selection is not interpretation.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn bounded_chunks(
    text: &str,
    byte_budget: usize,
    overlap: usize,
) -> Result<Vec<(usize, String)>> {
    ensure!(
        byte_budget > 0,
        "knowledge_request_exceeds_local_byte_guard"
    );
    if text.is_empty() {
        return Ok(vec![(0, String::new())]);
    }
    let mut offsets: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
    offsets.push(text.len());
    let mut result = Vec::new();
    let mut start = 0;
    while start + 1 < offsets.len() {
        let mut low = start + 1;
        let mut high = (offsets.len() - 1).min(start + byte_budget);
        let mut end = start;
        while low <= high {
            let mid = low + (high - low) / 2;
            if serde_json::to_vec(&text[offsets[start]..offsets[mid]])?.len() - 2 <= byte_budget {
                end = mid;
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        ensure!(end > start, "knowledge_request_exceeds_local_byte_guard");
        result.push((start, text[offsets[start]..offsets[end]].to_string()));
        if end + 1 == offsets.len() {
            break;
        }
        start = end - overlap.min((end - start) / 4);
    }
    Ok(result)
}

fn prefix(text: &str, bytes: usize) -> &str {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Keep the opening and a context/condition window. All omitted ranges stay unknown.
pub fn review_windows(
    units: &[Value],
    query: &str,
    context: &Value,
    byte_budget: usize,
) -> Result<Vec<(String, usize, usize)>> {
    if units.is_empty() || byte_budget == 0 {
        return Ok(vec![]);
    }
    let mut ordered: Vec<_> = units.iter().collect();
    ordered.sort_by_key(|u| (u["source_span"]["start"].as_u64(), u["id"].as_str()));
    let data = |u: &Value| -> Result<(String, usize, String)> {
        Ok((
            u["id"].as_str().context("unit_id_missing")?.into(),
            u["source_span"]["start"]
                .as_u64()
                .context("unit_start_missing")? as usize,
            u["source_span"]["text"]
                .as_str()
                .context("unit_text_missing")?
                .into(),
        ))
    };
    let entries = ordered
        .iter()
        .map(|u| data(u))
        .collect::<Result<Vec<_>>>()?;
    if entries.len() <= 2 && entries.iter().map(|(_, _, s)| s.len()).sum::<usize>() <= byte_budget {
        return Ok(entries
            .iter()
            .map(|(id, start, s)| (id.clone(), *start, *start + s.chars().count()))
            .collect());
    }
    let mut weights = BTreeMap::<String, f64>::new();
    let mut fields = vec![
        (query, 2.0),
        (context["goal"].as_str().unwrap_or(""), 2.0),
        (context["object"]["name"].as_str().unwrap_or(""), 1.0),
    ];
    for field in ["functions", "changes"] {
        if let Some(values) = context[field].as_array() {
            for value in values {
                if let Some(s) = value.as_str() {
                    fields.push((s, 3.0));
                }
            }
        }
    }
    for (text, weight) in fields {
        for token in crate::text::tokens(text)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        {
            *weights.entry(token).or_default() += weight;
        }
    }
    let all = crate::text::normalize(
        &entries
            .iter()
            .map(|(_, _, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    for (term, weight) in &mut weights {
        *weight /= 1.0 + all.matches(term.as_str()).count() as f64;
    }
    let opening = &entries[0];
    let opening_text = prefix(&opening.2, 180.min(byte_budget / 5));
    let mut windows = if opening_text.is_empty() {
        vec![]
    } else {
        vec![(
            opening.0.clone(),
            opening.1,
            opening.1 + opening_text.chars().count(),
        )]
    };
    let remaining = byte_budget - opening_text.len();
    let mut best: Option<(f64, usize, usize, usize)> = None;
    for (unit, (_, _, text)) in entries.iter().enumerate() {
        let mut start = 0;
        for piece in text.split_inclusive(['\n', '。', '！', '？']) {
            let end = start + piece.len();
            let normalized = crate::text::normalize(piece);
            let relevance: f64 = weights
                .iter()
                .filter(|(term, _)| normalized.contains(term.as_str()))
                .map(|(_, w)| *w)
                .sum();
            let conditions = [
                "禁止",
                "不得",
                "不能",
                "仅",
                "必须",
                "条件",
                "除外",
                "例外",
                "注意",
                "不保证",
                "must",
                "shall",
                "only",
                "unless",
                "except",
                "cannot",
                "warning",
                "caution",
            ]
            .iter()
            .filter(|word| normalized.contains(**word))
            .count()
            .min(3) as f64;
            let score = (relevance * (1.0 + 0.6 * conditions) + 0.3 * conditions)
                / (piece.chars().count() as f64 / 350.0).max(1.0);
            if best.is_none_or(|(s, _, _, _)| score > s) {
                best = Some((score, unit, start, end));
            }
            start = end;
        }
    }
    if remaining > 0
        && let Some((_, unit, start, end)) = best
    {
        let (id, absolute, text) = &entries[unit];
        let mut offsets: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
        offsets.push(text.len());
        let center = (start + end) / 2;
        let left_byte = center
            .saturating_sub(remaining / 2)
            .min(text.len().saturating_sub(remaining));
        let mut left = offsets.partition_point(|x| *x < left_byte);
        let boundary = text[..offsets[left]]
            .rfind(['\n', '。', '！', '？'])
            .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
        if offsets[left] - boundary <= 80.min(remaining / 4)
            && (end - start > remaining || boundary + remaining >= end)
        {
            left = offsets.partition_point(|x| *x < boundary);
        }
        let right = offsets.partition_point(|x| *x <= offsets[left] + remaining) - 1;
        if right > left {
            let (left, right) = (*absolute + left, *absolute + right);
            if let Some(w) = windows.first_mut().filter(|w| w.0 == *id && left <= w.2) {
                w.1 = w.1.min(left);
                w.2 = w.2.max(right);
            } else {
                windows.push((id.clone(), left, right));
            }
        }
    }
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chunks_cover_unicode_and_json_escaping_without_cutting_bytes() {
        let text = "长\"线\n𠮷e\u{301}".repeat(100);
        let chunks = bounded_chunks(&text, 64, 350).unwrap();
        let mut covered = 0;
        for (start, part) in chunks {
            assert!(start <= covered);
            assert!(serde_json::to_vec(&part).unwrap().len() - 2 <= 64);
            let end = start + part.chars().count();
            assert_eq!(crate::text::char_slice(&text, start, end).unwrap(), part);
            covered = covered.max(end);
        }
        assert_eq!(covered, text.chars().count());
        assert!(bounded_chunks("𠮷", 1, 0).is_err());
    }
    #[test]
    fn condition_at_the_end_can_enter_window_with_exact_coordinates() {
        let text = format!(
            "元件安装。{}\n旋转要求仅在锁紧释放时允许，必须核对载荷。",
            "直线运动介绍。".repeat(200)
        );
        let units = vec![
            json!({"id":"u","source_span":{"start":10,"end":10+text.chars().count(),"text":text}}),
        ];
        let context = json!({"goal":"新增旋转要求","object":{"name":"元件"},"functions":[],"changes":["旋转要求"]});
        let windows = review_windows(&units, "旋转锁紧", &context, 240).unwrap();
        let mut bytes = 0;
        let mut found = false;
        for (_, start, end) in windows {
            let s = crate::text::char_slice(&text, start - 10, end - 10).unwrap();
            bytes += s.len();
            found |= s.contains("必须核对载荷");
        }
        assert!(bytes <= 240);
        assert!(found);
    }
}
