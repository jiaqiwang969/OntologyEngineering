//! Display-text decoding and lexical facts for drawing annotations.
//!
//! The functions read only the text bytes of one source occurrence. They decode
//! MTEXT formatting, AutoCAD control codes and stacked deviations, then report
//! lexical facts (diameter sign, radius, thread designation, chamfer, angle,
//! roman numeral, scale, note). A lexical fact is evidence for an expression;
//! it never decides the measured feature or a manufacturing requirement.

/// Decoded display text with stacked deviations kept separate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decoded {
    /// Readable single-line text, deviations rendered as `+a/-b`.
    pub text: String,
    /// Main text without stacked deviations.
    pub main: String,
    pub upper: Option<String>,
    pub lower: Option<String>,
}

fn push_stack(out: &mut Decoded, upper: &str, lower: &str) {
    let (u, l) = (upper.trim(), lower.trim());
    if !u.is_empty() || !l.is_empty() {
        out.upper = Some(u.to_string());
        out.lower = Some(l.to_string());
        out.text.push_str(&format!(" {u}/{l}"));
    }
}

/// Decode MTEXT/TEXT content. Private-use code points U+E1D6..U+E1DA used by
/// SHX big fonts for stacked tolerances are interpreted as upper/lower fields.
pub fn decode(raw: &str) -> Decoded {
    let mut out = Decoded::default();
    let chars: Vec<char> = raw
        .replace("%%c", "Ø")
        .replace("%%C", "Ø")
        .replace("%%d", "°")
        .replace("%%D", "°")
        .replace("%%p", "±")
        .replace("%%P", "±")
        .replace("%%u", "")
        .replace("%%U", "")
        .replace("%%o", "")
        .replace("%%O", "")
        .chars()
        .collect();
    let mut i = 0;
    let mut upper = String::new();
    let mut lower = String::new();
    // 0 = main, 1 = upper, 2 = lower
    let mut mode = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\u{e1d6}' => {
                mode = 1;
                i += 1;
                continue;
            }
            '\u{e1d7}' => {
                mode = 2;
                i += 1;
                continue;
            }
            '\u{e1da}' | '\u{e1d9}' | '\u{e1d8}' => {
                if mode == 2 || c == '\u{e1d9}' {
                    push_stack(&mut out, &upper, &lower);
                    upper.clear();
                    lower.clear();
                }
                mode = 0;
                i += 1;
                continue;
            }
            '{' | '}' => {
                i += 1;
                continue;
            }
            '\\' if i + 1 < chars.len() => {
                let k = chars[i + 1];
                match k {
                    'P' | 'p' => {
                        out.text.push(' ');
                        out.main.push(' ');
                        i += 2;
                    }
                    'S' => {
                        // \Supper^lower;  \Supper/lower;  \Supper#lower;
                        let mut j = i + 2;
                        let mut a = String::new();
                        let mut b = String::new();
                        let mut second = false;
                        while j < chars.len() && chars[j] != ';' {
                            if !second && matches!(chars[j], '^' | '/' | '#') {
                                second = true;
                            } else if second {
                                b.push(chars[j]);
                            } else {
                                a.push(chars[j]);
                            }
                            j += 1;
                        }
                        push_stack(&mut out, &a, &b);
                        i = j + 1;
                    }
                    'A' | 'C' | 'c' | 'F' | 'f' | 'H' | 'h' | 'Q' | 'q' | 'T' | 't' | 'W' | 'w' => {
                        let mut j = i + 2;
                        while j < chars.len() && chars[j] != ';' {
                            j += 1;
                        }
                        i = j + 1;
                    }
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' | 'N' | 'X' => i += 2,
                    '\\' | '{' | '}' => {
                        out.text.push(k);
                        out.main.push(k);
                        i += 2;
                    }
                    _ => {
                        out.text.push(k);
                        out.main.push(k);
                        i += 2;
                    }
                }
            }
            _ => {
                match mode {
                    1 => upper.push(c),
                    2 => lower.push(c),
                    _ => {
                        out.text.push(c);
                        out.main.push(c);
                    }
                }
                i += 1;
            }
        }
    }
    if mode != 0 && (!upper.is_empty() || !lower.is_empty()) {
        push_stack(&mut out, &upper, &lower);
    }
    out.text = collapse(&out.text);
    out.main = collapse(&out.main);
    out
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c)
}

/// Lexical classes used as annotation evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lex {
    Diameter,
    Radius,
    Thread,
    ThreadDepth,
    Chamfer,
    Angle,
    Number,
    ToleranceValue,
    Letters,
    Roman,
    Scale,
    ViewSuffix,
    Heading,
    ListNumber,
    Note,
    Other,
}

fn strip_marks(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '☆' | '★' | '*' | '(' | ')' | '（' | '）' | ' '))
        .collect()
}

fn starts_number(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

pub fn is_roman(s: &str) -> bool {
    matches!(
        s.trim(),
        "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X"
    )
}

pub fn is_scale(s: &str) -> bool {
    let t = s.trim();
    let mut parts = t.split(':');
    let (Some(a), Some(b), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !a.is_empty()
        && !b.is_empty()
        && a.chars().all(|c| c.is_ascii_digit() || c == '.')
        && b.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Unified inch thread written as nominal size in inches (a fraction or whole inches), the inch mark,
/// a dash and the threads per inch (`7/16"-14`, `1"-8 UNC`); an optional count prefix `n-` before it.
pub fn is_inch_thread(s: &str) -> bool {
    let t = strip_marks(s);
    t.match_indices("\"-").any(|(i, _)| {
        // the size is the run of digits, fraction bar and point just before the inch mark
        let size: String = t[..i]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '/' || *c == '.')
            .collect();
        size.chars().any(|c| c.is_ascii_digit())
            && size.matches('/').count() <= 1
            && !size.ends_with('/')
            && !size.starts_with('/')
            && t[i + 2..].chars().next().is_some_and(|c| c.is_ascii_digit())
    })
}

/// Thread designation: metric M.., unified ..UNC/UNF/UNEF or inch size with threads per inch, pipe
/// G/Rc, trapezoid Tr.
pub fn is_thread(s: &str) -> bool {
    let t = strip_marks(s);
    let u = t.to_uppercase();
    if u.contains("UNF") || u.contains("UNC") || u.contains("UNEF") || u.contains("NPT") || is_inch_thread(&t) {
        return true;
    }
    let mut it = t
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '×' || c == 'x')
        .chars();
    let first = it.next();
    let second = it.next();
    match (first, second) {
        (Some('M'), Some(d)) => d.is_ascii_digit(),
        (Some('T'), Some('r')) => true,
        (Some('G'), Some(d)) => d.is_ascii_digit() && t.contains('/'),
        _ => false,
    }
}

/// Chamfer or taper callout `a×b°` / `axb°` / `C1`.
pub fn is_chamfer(s: &str) -> bool {
    let t = strip_marks(s);
    if let Some(rest) = t.strip_prefix('C') {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.');
    }
    if !t.contains('°') || !starts_number(&t) {
        return false;
    }
    t.contains('×') || t.contains('x') || t.contains('X') || t.contains('*')
}

pub fn lex(text: &str) -> Lex {
    let t = text.trim();
    if t.is_empty() {
        return Lex::Other;
    }
    if t.contains("技术要求") {
        return Lex::Heading;
    }
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let cs: Vec<char> = compact.chars().collect();
    if (2..=4).contains(&cs.len())
        && matches!(cs[cs.len() - 1], '.' | '、')
        && cs[..cs.len() - 1].iter().all(|c| c.is_ascii_digit())
    {
        return Lex::ListNumber;
    }
    if compact == "向" {
        return Lex::ViewSuffix;
    }
    if is_roman(&compact) {
        return Lex::Roman;
    }
    if is_scale(&compact) {
        return Lex::Scale;
    }
    if compact.starts_with("攻深")
        || compact.starts_with("孔深")
        || compact == "均布"
        || compact.starts_with("口部倒角")
    {
        return Lex::ThreadDepth;
    }
    if is_thread(&compact) {
        return Lex::Thread;
    }
    let cjk = compact.chars().filter(|c| is_cjk(*c)).count();
    if cjk >= 2 {
        return Lex::Note;
    }
    let bare = strip_marks(&compact);
    if bare.contains('Ø') || bare.contains('φ') || bare.contains('Φ') {
        return Lex::Diameter;
    }
    if (bare.starts_with('R') || bare.starts_with("SR")) && bare.len() > 1 {
        let rest = bare.trim_start_matches('S').trim_start_matches('R');
        if rest.is_empty() || starts_number(rest) {
            return Lex::Radius;
        }
    }
    if bare == "R" {
        return Lex::Radius;
    }
    if is_chamfer(&bare) {
        return Lex::Chamfer;
    }
    if bare.contains('°') && starts_number(&bare) {
        return Lex::Angle;
    }
    if starts_number(&bare) || bare.starts_with(['+', '-', '±']) {
        if bare.starts_with("0.") && bare.len() <= 6 && !bare.contains('/') {
            return Lex::ToleranceValue;
        }
        return Lex::Number;
    }
    if !bare.is_empty()
        && bare.len() <= 5
        && bare.chars().all(|c| c.is_ascii_uppercase() || c == '-')
    {
        return Lex::Letters;
    }
    Lex::Other
}

/// Expression kind suggested by a dimension display text; None means the text
/// does not decide it and native construction must be used.
pub fn dimension_kind(text: &str) -> Option<&'static str> {
    match lex(text) {
        Lex::Thread => Some("thread_note"),
        Lex::Diameter => Some("dimension_diameter"),
        Lex::Radius => Some("dimension_radius"),
        Lex::Chamfer => Some("dimension_chamfer"),
        Lex::Angle => Some("dimension_angle"),
        Lex::Number | Lex::ToleranceValue => Some("dimension_linear"),
        _ => None,
    }
}

/// Qualifiers carried by the display text.
pub fn qualifiers(d: &Decoded) -> Vec<String> {
    let mut q = Vec::new();
    if d.text.contains('☆') || d.text.contains('★') {
        q.push("key_characteristic".to_string());
    }
    let t = d.text.trim();
    if (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('（') && t.ends_with('）')) {
        q.push("reference_dimension".to_string());
    }
    if d.upper.is_some() || d.text.contains('±') {
        q.push("toleranced".to_string());
    }
    q
}

/// Approximate advance width of a text in units of its height (for placement only).
pub fn width_factor(text: &str) -> f64 {
    text.chars()
        .map(|c| {
            if is_cjk(c) {
                1.0
            } else if c == ' ' {
                0.45
            } else {
                0.68
            }
        })
        .sum::<f64>()
        .max(0.5)
}

/// Technical-requirement topic from item wording; general when no topic matches.
pub fn note_kind(text: &str) -> &'static str {
    let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
    if has(&[
        "热处理",
        "调质",
        "淬火",
        "回火",
        "渗碳",
        "渗氮",
        "HRC",
        "HB",
        "HV",
        "硬度",
        "材料",
    ]) {
        "material_heat"
    } else if has(&["清洗", "防锈", "涂", "漆", "铁屑", "清洁", "去毛刺", "油"]) {
        "protection_requirement"
    } else if has(&[
        "钻",
        "铣",
        "车削",
        "磨削",
        "加工",
        "装配",
        "配作",
        "相对位置",
        "键槽",
    ]) {
        "process_requirement"
    } else {
        "technical_general"
    }
}

/// Title-block or registration-panel vocabulary.
pub const TITLE_WORDS: [&str; 26] = [
    "设计",
    "设 计",
    "校对",
    "校 对",
    "审核",
    "审 核",
    "工艺",
    "工 艺",
    "批准",
    "批 准",
    "标准化",
    "日期",
    "日 期",
    "比例",
    "比 例",
    "数量",
    "质量",
    "图样标记",
    "签字",
    "签 字",
    "描图",
    "描校",
    "底图总号",
    "用件登记",
    "更改文件号",
    "处数",
];
