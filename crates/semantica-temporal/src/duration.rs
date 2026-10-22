//! Original duration grammar and CPython timedelta accumulation, shared by
//! snapshot normalization and source query constructors. Callers keep their
//! own budgets, calendar/Decimal policy and source error boundaries.
use regex::Regex;
use std::sync::OnceLock;

pub struct DurationParts<'a> {
    pub years: &'a str,
    pub months: &'a str,
    pub negative: bool,
    time: [&'a str; 5],
}
impl DurationParts<'_> {
    pub fn timedelta_micros(&self) -> Option<i128> {
        let mut values = [0.0; 5];
        for (target, text) in values.iter_mut().zip(self.time) {
            *target = text.replace(',', ".").parse::<f64>().ok()?;
        }
        timedelta_us(&values)
    }
    pub fn has_calendar_components(&self) -> bool {
        self.years
            .bytes()
            .chain(self.months.bytes())
            .any(|b| b.is_ascii_digit() && b != b'0')
    }
}
pub fn duration_parts(raw: &str) -> Option<DurationParts<'_>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re=RE.get_or_init(||Regex::new(r"^([+-])?P(?:([0-9]+(?:[,.][0-9]+)?)Y)?(?:([0-9]+(?:[,.][0-9]+)?)M)?(?:([0-9]+(?:[,.][0-9]+)?)W)?(?:([0-9]+(?:[,.][0-9]+)?)D)?(?:(T)(?:([0-9]+(?:[,.][0-9]+)?)H)?(?:([0-9]+(?:[,.][0-9]+)?)M)?(?:([0-9]+(?:[,.][0-9]+)?)S)?)?$").expect("fixed original duration grammar"));
    let input = raw.strip_suffix('\n').unwrap_or(raw);
    let c = re
        .captures(input)
        .filter(|_| !matches!(input, "P" | "+P" | "-P"))?;
    let component = |n| c.get(n).map(|m| m.as_str()).unwrap_or("0");
    Some(DurationParts {
        years: component(2),
        months: component(3),
        negative: component(1) == "-",
        time: [
            component(9),
            component(8),
            component(7),
            component(5),
            component(4),
        ],
    })
}

/// Normalized Python timedelta value. It is not an XML Schema calendar duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Timedelta {
    microseconds: i128,
}
impl Timedelta {
    pub fn from_micros(microseconds: i128) -> Option<Self> {
        let days = microseconds.div_euclid(86_400_000_000);
        (-999_999_999..=999_999_999)
            .contains(&days)
            .then_some(Self { microseconds })
    }
    pub fn as_micros(self) -> i128 {
        self.microseconds
    }
    pub fn parts(self) -> (i64, u32, u32) {
        let days = self.microseconds.div_euclid(86_400_000_000);
        let remainder = self.microseconds.rem_euclid(86_400_000_000);
        (
            days as i64,
            (remainder / 1_000_000) as u32,
            (remainder % 1_000_000) as u32,
        )
    }
    pub fn is_zero(self) -> bool {
        self.microseconds == 0
    }
    pub fn isoformat(self) -> String {
        format_duration_tail(
            String::new(),
            self.microseconds.unsigned_abs(),
            self.microseconds < 0,
        )
    }
    pub fn repr(self) -> String {
        let (days, seconds, microseconds) = self.parts();
        let mut fields = Vec::new();
        if days != 0 {
            fields.push(format!("days={days}"));
        }
        if seconds != 0 {
            fields.push(format!("seconds={seconds}"));
        }
        if microseconds != 0 {
            fields.push(format!("microseconds={microseconds}"));
        }
        format!(
            "datetime.timedelta({})",
            if fields.is_empty() {
                "0".to_owned()
            } else {
                fields.join(", ")
            }
        )
    }
}

fn timedelta_us(parts: &[f64; 5]) -> Option<i128> {
    let factors = [
        1_000_000_i128,
        60_000_000,
        3_600_000_000,
        86_400_000_000,
        604_800_000_000,
    ];
    let mut total = 0_i128;
    let mut leftover = 0.0;
    for (value, factor) in parts.iter().zip(factors) {
        if !value.is_finite() || *value < 0.0 || value.trunc() > 1e20 {
            return None;
        }
        total = total.checked_add((value.trunc() as i128).checked_mul(factor)?)?;
        let fraction = value.fract() * (factor as f64);
        total = total.checked_add(fraction.trunc() as i128)?;
        leftover += fraction.fract();
    }
    let mut rounded = leftover.round();
    if (rounded - leftover).abs() == 0.5 {
        let odd = (total % 2) as f64;
        rounded = 2.0 * ((leftover + odd) * 0.5).round() - odd;
    }
    total = total.checked_add(rounded as i128)?;
    if total / 86_400_000_000 > 999_999_999 {
        return None;
    }
    Some(total)
}

/// Shared original duration spelling; callers own calendar and sign policy.
pub fn format_duration_tail(mut body: String, micros: u128, minus: bool) -> String {
    let mut seconds = micros / 1_000_000;
    let us = micros % 1_000_000;
    let days = seconds / 86400;
    seconds %= 86400;
    let hours = seconds / 3600;
    seconds %= 3600;
    let minutes = seconds / 60;
    seconds %= 60;
    if days != 0 {
        body.push_str(&format!("{days}D"));
    }
    if hours != 0 || minutes != 0 || seconds != 0 || us != 0 {
        body.push('T');
        if hours != 0 {
            body.push_str(&format!("{hours}H"));
        }
        if minutes != 0 {
            body.push_str(&format!("{minutes}M"));
        }
        if seconds != 0 || us != 0 {
            if us != 0 {
                let fraction = format!("{us:06}");
                body.push_str(&format!("{seconds}.{}S", fraction.trim_end_matches('0')));
            } else {
                body.push_str(&format!("{seconds}S"));
            }
        }
    }
    if body.is_empty() {
        body = "0D".into();
    }
    format!("{}P{body}", if minus { "-" } else { "" })
}
