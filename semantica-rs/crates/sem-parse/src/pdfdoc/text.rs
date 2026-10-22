//! pdfplumber's default text extraction (`Page.extract_text()` with `layout=False`, and
//! `utils.extract_text` used for table cells): characters are grouped into words by
//! `WordExtractor` (`x_tolerance=3`, `y_tolerance=3`, blank characters split words,
//! rotated text read with the flipped directions), words into lines by clustering their
//! `doctop`/`top`, lines joined with `"\n"` and words with `" "`, ligatures expanded.

use super::interp::Char;

/// `DEFAULT_X_TOLERANCE` / `DEFAULT_Y_TOLERANCE`.
pub const X_TOLERANCE: f64 = 3.0;
pub const Y_TOLERANCE: f64 = 3.0;

fn ligature(text: &str) -> &str {
    match text {
        "\u{fb00}" => "ff",
        "\u{fb03}" => "ffi",
        "\u{fb04}" => "ffl",
        "\u{fb01}" => "fi",
        "\u{fb02}" => "fl",
        "\u{fb06}" => "st",
        "\u{fb05}" => "st",
        other => other,
    }
}

/// Python's `str.isspace()`.
fn py_isspace(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// A word (`WordExtractor.merge_chars`).
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub doctop: f64,
    pub bottom: f64,
    pub chars: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Ttb,
    Ltr,
}

/// `cluster_objects(xs, key_fn, tolerance, preserve_order)`: groups of indices.
pub fn cluster_objects(keys: &[f64], tolerance: f64, preserve_order: bool) -> Vec<Vec<usize>> {
    // make_cluster_dict: unique values sorted, chained within tolerance.
    let mut uniq: Vec<f64> = keys.to_vec();
    uniq.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    uniq.dedup();
    let mut cluster_of: Vec<(f64, usize)> = Vec::new();
    if tolerance == 0.0 || uniq.len() < 2 {
        for (i, v) in uniq.iter().enumerate() {
            cluster_of.push((*v, i));
        }
    } else {
        let mut idx = 0;
        let mut last = uniq[0];
        cluster_of.push((uniq[0], 0));
        for &v in &uniq[1..] {
            if v > last + tolerance {
                idx += 1;
            }
            cluster_of.push((v, idx));
            last = v;
        }
    }
    let lookup = |v: f64| -> usize {
        cluster_of
            .iter()
            .find(|(k, _)| *k == v)
            .map(|(_, c)| *c)
            .unwrap_or(0)
    };
    let mut tuples: Vec<(usize, usize)> = keys.iter().enumerate().map(|(i, &k)| (i, lookup(k))).collect();
    if !preserve_order {
        tuples.sort_by_key(|&(_, c)| c);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Option<usize> = None;
    for (i, c) in tuples {
        if current == Some(c) {
            groups.last_mut().expect("group").push(i);
        } else {
            groups.push(vec![i]);
            current = Some(c);
        }
    }
    groups
}

fn char_begins_new_word(prev: &Char, curr: &Char, dir: Dir) -> bool {
    let (x, y, ax, bx, cx, ay, cy) = match dir {
        Dir::Ltr => (X_TOLERANCE, Y_TOLERANCE, prev.x0, prev.x1, curr.x0, prev.top, curr.top),
        Dir::Ttb => (Y_TOLERANCE, X_TOLERANCE, prev.top, prev.bottom, curr.top, prev.x0, curr.x0),
    };
    cx < ax || cx > bx + x || cy > ay + y
}

/// `WordExtractor.extract_words(chars)` with the default settings.
pub fn extract_words(chars: &[Char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // itertools.groupby on "upright": consecutive runs.
        let upright = chars[i].upright;
        let mut j = i;
        while j < chars.len() && chars[j].upright == upright {
            j += 1;
        }
        let group: Vec<usize> = (i..j).collect();
        // Rotated text flips the directions: lines along x0, characters top-to-bottom.
        let (line_key, char_dir): (Box<dyn Fn(&Char) -> f64>, Dir) = if upright {
            (Box::new(|c: &Char| c.top), Dir::Ltr)
        } else {
            (Box::new(|c: &Char| c.x0), Dir::Ttb)
        };
        let keys: Vec<f64> = group.iter().map(|&k| line_key(&chars[k])).collect();
        for sub in cluster_objects(&keys, Y_TOLERANCE, false) {
            let mut line: Vec<usize> = sub.iter().map(|&k| group[k]).collect();
            line.sort_by(|&a, &b| {
                let (ca, cb) = (&chars[a], &chars[b]);
                let ka = match char_dir {
                    Dir::Ltr => (ca.x0, ca.x1),
                    Dir::Ttb => (ca.top, ca.bottom),
                };
                let kb = match char_dir {
                    Dir::Ltr => (cb.x0, cb.x1),
                    Dir::Ttb => (cb.top, cb.bottom),
                };
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut current: Vec<usize> = Vec::new();
            let mut flush = |current: &mut Vec<usize>, words: &mut Vec<Word>| {
                if !current.is_empty() {
                    words.push(merge_chars(chars, current));
                    current.clear();
                }
            };
            for &k in &line {
                let c = &chars[k];
                if py_isspace(&c.text) {
                    flush(&mut current, &mut words);
                } else if c.text.is_empty() {
                    // `text in ""` (split_at_punctuation default) is true for empty text.
                    flush(&mut current, &mut words);
                    current.push(k);
                    flush(&mut current, &mut words);
                } else if let Some(&last) = current.last()
                    && char_begins_new_word(&chars[last], c, char_dir)
                {
                    flush(&mut current, &mut words);
                    current.push(k);
                } else {
                    current.push(k);
                }
            }
            flush(&mut current, &mut words);
        }
        i = j;
    }
    words
}

fn merge_chars(chars: &[Char], idx: &[usize]) -> Word {
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for &k in idx {
        let c = &chars[k];
        x0 = x0.min(c.x0);
        top = top.min(c.top);
        x1 = x1.max(c.x1);
        bottom = bottom.max(c.bottom);
    }
    let first = &chars[idx[0]];
    let doctop_adj = first.doctop - first.top;
    Word {
        text: idx.iter().map(|&k| ligature(&chars[k].text)).collect(),
        x0,
        x1,
        top,
        doctop: top + doctop_adj,
        bottom,
        chars: idx.to_vec(),
    }
}

/// `Page.extract_text()`: words through pdfplumber's `TextMap` (presorted).
pub fn page_text(chars: &[Char]) -> String {
    let words = extract_words(chars);
    if words.is_empty() {
        return String::new();
    }
    let keys: Vec<f64> = words.iter().map(|w| w.doctop).collect();
    let mut out = String::new();
    for (i, line) in cluster_objects(&keys, Y_TOLERANCE, true).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut line_len = 0;
        for &w in line {
            if line_len > 0 {
                out.push(' ');
                line_len += 1;
            }
            for &k in &words[w].chars {
                let letters = ligature(&chars[k].text);
                out.push_str(letters);
                line_len += letters.chars().count();
            }
        }
    }
    out
}

/// `utils.extract_text(chars)` (non-layout): words clustered into lines by `top`.
pub fn simple_text(chars: &[Char]) -> String {
    let words = extract_words(chars);
    if words.is_empty() {
        return String::new();
    }
    let keys: Vec<f64> = words.iter().map(|w| w.top).collect();
    cluster_objects(&keys, Y_TOLERANCE, false)
        .iter()
        .map(|line| {
            line.iter()
                .map(|&w| words[w].text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(text: &str, x0: f64, x1: f64, top: f64) -> Char {
        Char {
            text: text.to_string(),
            fontname: "F".into(),
            x0,
            y0: 0.0,
            x1,
            y1: 0.0,
            top,
            bottom: top + 10.0,
            doctop: top,
            upright: true,
            size: 10.0,
            adv: x1 - x0,
        }
    }

    #[test]
    fn words_and_lines() {
        let chars = vec![
            ch("H", 10.0, 17.0, 100.0),
            ch("i", 17.0, 20.0, 100.0),
            ch(" ", 20.0, 23.0, 100.0),
            ch("y", 23.0, 28.0, 100.5),
            ch("o", 40.0, 45.0, 100.0),
            ch("\u{fb01}", 10.0, 16.0, 120.0),
        ];
        assert_eq!(page_text(&chars), "Hi y o\nfi");
        assert_eq!(simple_text(&chars), "Hi y o\nfi");
    }

    #[test]
    fn clustering_chains_within_tolerance() {
        let groups = cluster_objects(&[10.0, 12.0, 14.5, 30.0, 12.0], 3.0, false);
        assert_eq!(groups, vec![vec![0, 1, 2, 4], vec![3]]);
        let kept = cluster_objects(&[10.0, 30.0, 11.0], 3.0, true);
        assert_eq!(kept, vec![vec![0], vec![1], vec![2]]);
    }
}
