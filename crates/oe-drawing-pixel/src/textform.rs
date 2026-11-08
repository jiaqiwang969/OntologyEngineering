//! Text forms of recovered texts: symbol normalisation of what the observer wrote, the stacked
//! tolerance written with the font stacking marks the DXF route's text model reads (U+E1D6 upper
//! begins, U+E1DA upper ends, U+E1D7 lower begins, U+E1D9 lower ends), and the width measure of that
//! text model, so a recovered text box can be given the width factor that reproduces it.

/// Font stacking marks of Chinese CAD fonts.
pub const UPPER_BEGIN: char = '\u{E1D6}';
pub const UPPER_END: char = '\u{E1DA}';
pub const LOWER_BEGIN: char = '\u{E1D7}';
pub const LOWER_END: char = '\u{E1D9}';

/// Symbols in the display form of the DXF route: `%%C`, `φ`, `∅`, `⌀`, `Φ` are `Ø`; `×` is `x`; `%%D`
/// is `°`; `%%P` is `±`; whitespace runs collapse to one space.
pub fn normalise_symbols(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' && i + 2 < chars.len() && chars[i + 1] == '%' {
            let mapped = match chars[i + 2].to_ascii_lowercase() {
                'c' => Some('Ø'),
                'd' => Some('°'),
                'p' => Some('±'),
                _ => None,
            };
            if let Some(m) = mapped {
                out.push(m);
                i += 3;
                continue;
            }
        }
        out.push(match c {
            'φ' | '∅' | '⌀' | 'Φ' | 'ø' => 'Ø',
            '×' => 'x',
            _ => c,
        });
        i += 1;
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut space = false;
    for c in out.trim().chars() {
        if c.is_whitespace() {
            if !space {
                collapsed.push(' ');
            }
            space = true;
        } else {
            collapsed.push(c);
            space = false;
        }
    }
    collapsed
}

/// Key for de-duplication across tiles: symbols normalised, spaces and `/` removed, `X` as `x`.
pub fn normalise_key(s: &str) -> String {
    normalise_symbols(s)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '/')
        .map(|c| if c == 'X' { 'x' } else { c })
        .collect()
}

/// A deviation of a stacked tolerance: an optionally signed decimal number.
fn is_deviation(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && body.chars().any(|c| c.is_ascii_digit())
        && body.matches('.').count() <= 1
}

/// Text of a recovered text member in its two forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextForm {
    /// Display form (expression label), symbols normalised: `☆Ø35 +0.0139/+0.0038`.
    pub label: String,
    /// Entity text with the stacked tolerance in font stacking marks, or the label when flat.
    pub entity_text: String,
    pub stacked: bool,
}

/// The two forms of an observed text. A trailing token `upper/lower` whose parts are deviations
/// (one of them signed or zero) is a stacked tolerance; `3/8` or `1/2` are not.
pub fn text_form(observed: &str) -> TextForm {
    let label = normalise_symbols(observed);
    let mut tokens: Vec<&str> = label.split(' ').collect();
    if tokens.len() >= 2
        && let Some(last) = tokens.last().copied()
        && let Some((upper, lower)) = last.split_once('/')
        && is_deviation(upper)
        && is_deviation(lower)
        && (upper.starts_with(['+', '-']) || lower.starts_with(['+', '-']) || upper == "0" || lower == "0")
    {
        tokens.pop();
        let main = tokens.join(" ");
        if !main.is_empty() {
            return TextForm {
                entity_text: format!("{main}{UPPER_BEGIN}{upper}{UPPER_END}{LOWER_BEGIN}{lower}{LOWER_END}"),
                label,
                stacked: true,
            };
        }
    }
    TextForm {
        entity_text: label.clone(),
        label,
        stacked: false,
    }
}

/// Width of a text in text heights as the checker's text model measures it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    /// Whole width in heights (stacked parts at their reduced scale).
    pub width: f64,
    /// Width of the main text before the first stack.
    pub main_width: f64,
    pub stacked: bool,
}

fn char_width(c: char) -> f64 {
    if c.is_ascii() {
        match c {
            '.' | ',' | ':' | ';' | '\'' | ' ' => 0.45,
            '1' | 'i' | 'l' | 'I' | '|' | '(' | ')' => 0.6,
            _ => 0.8,
        }
    } else {
        1.0
    }
}

/// Measure an entity text as the checker does: plain characters by class, a stacked pair by the
/// wider part at 0.6 scale, private-use characters skipped. Only the forms this crate emits are
/// covered (no MTEXT formatting codes).
pub fn text_measure(entity_text: &str) -> TextMeasure {
    let mut width = 0f64;
    let mut stacked = false;
    let mut first_stack: Option<f64> = None;
    let mut part: Option<f64> = None;
    let mut pending = 0f64;
    for c in entity_text.chars() {
        match c {
            UPPER_BEGIN | LOWER_BEGIN => {
                stacked = true;
                first_stack.get_or_insert(width);
                part = Some(0.0);
            }
            UPPER_END | LOWER_END => {
                if let Some(w) = part.take() {
                    if c == UPPER_END {
                        pending = w;
                    } else {
                        width += w.max(pending) * 0.6;
                        pending = 0.0;
                    }
                }
            }
            '\u{E000}'..='\u{F8FF}' => {}
            _ => match part.as_mut() {
                Some(w) => *w += char_width(c),
                None => width += char_width(c),
            },
        }
    }
    if let Some(w) = part.take() {
        width += w.max(pending) * 0.6;
    } else if pending > 0.0 {
        width += pending * 0.6;
    }
    TextMeasure {
        width,
        main_width: first_stack.unwrap_or(width),
        stacked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_and_keys_are_normalised() {
        assert_eq!(normalise_symbols("  %%C35  +0.01/ -0.02 "), "Ø35 +0.01/ -0.02");
        assert_eq!(normalise_symbols("φ57 2×45%%D %%p0.1"), "Ø57 2x45° ±0.1");
        assert_eq!(normalise_key("☆ Ø32 0/-0.025"), "☆Ø320-0.025");
        assert_eq!(normalise_key("☆Ø32 0/-0.025"), normalise_key("☆ φ32 0 / -0.025"));
        assert_eq!(normalise_key("2X45°"), normalise_key("2×45°"));
    }

    #[test]
    fn stacked_tolerances_take_the_font_marks_and_fractions_do_not() {
        let f = text_form("☆Ø35 +0.0139/+0.0038");
        assert!(f.stacked);
        assert_eq!(f.label, "☆Ø35 +0.0139/+0.0038");
        assert_eq!(f.entity_text, "☆Ø35\u{E1D6}+0.0139\u{E1DA}\u{E1D7}+0.0038\u{E1D9}");
        let f = text_form("Ø32 0/-0.025");
        assert_eq!(f.entity_text, "Ø32\u{E1D6}0\u{E1DA}\u{E1D7}-0.025\u{E1D9}");
        let f = text_form("28 -0.10/-0.20");
        assert_eq!(f.entity_text, "28\u{E1D6}-0.10\u{E1DA}\u{E1D7}-0.20\u{E1D9}");
        for flat in ["6-3/8\"", "1/2", "Ø57.8±0.05", "1x45°", "0/-0.025", "R36"] {
            let f = text_form(flat);
            assert!(!f.stacked, "{flat}");
            assert_eq!(f.entity_text, f.label);
        }
    }

    #[test]
    fn the_width_measure_follows_the_checker_text_model() {
        // Ø19 with a stacked +0.05/+0.02: main 1.0 + 0.6 + 0.8 = 2.4; each part 0.8 + 0.8 + 0.45 +
        // 0.8 + 0.8 = 3.65, the stack is the wider part at 0.6.
        let m = text_measure("Ø19\u{E1D6}+0.05\u{E1DA}\u{E1D7}+0.02\u{E1D9}");
        assert!((m.main_width - 2.4).abs() < 1e-9, "{}", m.main_width);
        assert!(m.stacked);
        assert!((m.width - (2.4 + 3.65 * 0.6)).abs() < 1e-9, "{}", m.width);
        let flat = text_measure("Ø19+0.05+0.02");
        assert!(!flat.stacked && (flat.width - 9.7).abs() < 1e-9, "{}", flat.width);
        assert!(m.width < 0.6 * flat.width);
        assert_eq!(flat.main_width, flat.width);
        assert!((text_measure("30").width - 1.6).abs() < 1e-9);
        assert!((text_measure("技术要求").width - 4.0).abs() < 1e-9);
    }
}
