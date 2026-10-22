//! Shared RDFLib calendar Duration values and the existing snapshot policy.
//! Calendar components are not converted to a fixed number of days. Numerical
//! operations use semantica-numeric; grammar/timedelta use the temporal owner.
use super::{Timedelta, duration_parts, format_duration_tail, parse_datetime};
use anyhow::{Result, ensure};
use num_bigint::{BigInt, Sign};
use semantica_numeric::{
    DecimalContext, DecimalOp, DecimalSignal, DecimalSignals, DecimalValue, NumericFault,
    NumericLimits,
};
use std::cell::Cell;
type Dec = DurationDecimal;
/// Legacy duration representation/formatting policy over the shared arithmetic
/// owner. BigInt deliberately has no signed zero; conversion retains exponent.
/// This adapter is not the RDF fixed-format Decimal policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurationDecimal {
    n: BigInt,
    e: i32,
    // Finite components retain the original BigInt/scale compatibility policy.
    // Caller contexts may also produce Infinity or NaN without trapping.
    special: Option<DecimalValue>,
}

// normalize() admits at most 10,000 UTF-8 bytes. Components are ASCII and their
// combined significant/fractional lengths fit that budget. The extra 64 cover
// 28-digit division guards, a year/month carry, and exact floor extraction.
// Do not impose the general numeric core's unrelated 4,096-digit defaults.
const DURATION_NUMERIC_BOUND: usize = 10_064;
fn duration_numeric_limits() -> NumericLimits {
    NumericLimits {
        max_input_characters: DURATION_NUMERIC_BOUND,
        max_coefficient_digits: DURATION_NUMERIC_BOUND,
        max_output_characters: DURATION_NUMERIC_BOUND,
        max_materialized_power_of_ten: DURATION_NUMERIC_BOUND,
    }
}
fn pow10(n: u32) -> BigInt {
    BigInt::from(10_u8).pow(n)
}
impl DurationDecimal {
    pub fn as_decimal(&self) -> DecimalValue {
        self.to_core()
    }
    fn zero() -> Self {
        Self {
            n: 0.into(),
            e: 0,
            special: None,
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Self::from_core(semantica_numeric::parse_decimal_exact(
            &s.replace(',', "."),
            &duration_numeric_limits(),
        )?)
    }
    pub fn from(n: i128) -> Self {
        Self {
            n: n.into(),
            e: 0,
            special: None,
        }
    }
    fn to_core(&self) -> DecimalValue {
        if let Some(value) = &self.special {
            return value.clone();
        }
        DecimalValue::Finite {
            negative: self.n.sign() == Sign::Minus,
            coefficient: self.n.magnitude().clone(),
            exponent: self.e.into(),
        }
    }
    pub fn from_core(value: DecimalValue) -> Result<Self> {
        let DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        } = value
        else {
            return Ok(Self {
                n: 0.into(),
                e: 0,
                special: Some(value),
            });
        };
        let e = i32::try_from(exponent)?;
        ensure!(
            e.unsigned_abs() as usize <= DURATION_NUMERIC_BOUND,
            "rdflib_duration_decimal_exponent_budget"
        );
        Ok(Self {
            // Legacy duration discarded a zero's sign through BigInt, while
            // preserving its scale. Keep that explicit compatibility policy.
            n: BigInt::from_biguint(if negative { Sign::Minus } else { Sign::Plus }, coefficient),
            e,
            special: None,
        })
    }
    // Duration's exact year/month quotient and remainder, not Decimal division
    // and not another precision/rounding kernel. Its representation feeds the
    // unchanged legacy formatter below.
    fn split12(&self) -> (Self, Self) {
        let e = self.e.min(0);
        let n = &self.n * pow10((self.e - e) as u32);
        let denominator = BigInt::from(12) * pow10((-e) as u32);
        let q = &n / &denominator;
        let r = &n % &denominator;
        (
            Self {
                n: q,
                e: 0,
                special: None,
            },
            Self {
                n: r,
                e,
                special: None,
            },
        )
    }
    pub fn floor_div12(&self) -> Result<Self> {
        let decimal = DecimalContext::default();
        DurationContext::new(&decimal).floor_div12(self)
    }
    pub fn from_parts(n: BigInt, e: i32) -> Result<Self> {
        ensure!(
            n.to_string().len() <= DURATION_NUMERIC_BOUND
                && e.unsigned_abs() as usize <= DURATION_NUMERIC_BOUND,
            "rdflib_duration_decimal_exponent_budget"
        );
        Ok(Self {
            n,
            e,
            special: None,
        })
    }
    pub fn display(&self) -> String {
        if let Some(value) = &self.special {
            return semantica_numeric::format_decimal_fixed(value, &duration_numeric_limits())
                .expect("bounded calendar special value");
        }
        let negative = self.n < BigInt::from(0);
        let s = self.n.to_string();
        let s = s.trim_start_matches('-');
        let adjusted = self.e + s.len() as i32 - 1;
        let body = if self.e > 0 || adjusted < -6 {
            let digits = if s.len() > 1 {
                format!("{}.{}", &s[..1], &s[1..])
            } else {
                s.into()
            };
            format!(
                "{digits}E{}{adjusted}",
                if adjusted >= 0 { "+" } else { "" }
            )
        } else if self.e == 0 {
            s.into()
        } else {
            let point = s.len() as i32 + self.e;
            if point > 0 {
                format!("{}.{}", &s[..point as usize], &s[point as usize..])
            } else {
                format!("0.{}{s}", "0".repeat((-point) as usize))
            }
        };
        format!("{}{body}", if negative { "-" } else { "" })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurationValue {
    Fixed(Timedelta),
    Calendar {
        years: DurationDecimal,
        months: DurationDecimal,
        delta: Timedelta,
    },
}
impl DurationValue {
    pub fn delta(&self) -> Timedelta {
        match self {
            Self::Fixed(d) | Self::Calendar { delta: d, .. } => *d,
        }
    }
    pub fn calendar_parts(&self) -> Option<(&DurationDecimal, &DurationDecimal)> {
        match self {
            Self::Calendar { years, months, .. } => Some((years, months)),
            Self::Fixed(_) => None,
        }
    }
    pub fn python_truth(&self) -> bool {
        match self {
            Self::Fixed(d) => !d.is_zero(),
            Self::Calendar { .. } => true,
        }
    }
    pub fn repr(&self) -> String {
        match self {
            Self::Fixed(d) => d.repr(),
            Self::Calendar {
                years,
                months,
                delta,
            } => {
                let (days, seconds, micros) = delta.parts();
                format!(
                    "rdflib.xsd_datetime.Duration({days}, {seconds}, {micros}, years={}, months={})",
                    years.display(),
                    months.display()
                )
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurationError {
    Value(&'static str),
    YearRange(i32),
    Overflow(&'static str),
    DivisionImpossible,
}
impl std::fmt::Display for DurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(v) => f.write_str(v),
            Self::YearRange(year) => write!(f, "year {year} is out of range"),
            Self::Overflow(v) => f.write_str(v),
            Self::DivisionImpossible => f.write_str("rdflib_decimal_DivisionImpossible"),
        }
    }
}
impl std::error::Error for DurationError {}

/// A caller-owned Decimal context and observed sticky signals. Validation may
/// create a separate instance, so checking an immutable cache does not execute
/// extra arithmetic in the query's observed context.
pub struct DurationContext<'a> {
    decimal: &'a DecimalContext,
    signals: Cell<DecimalSignals>,
}
impl<'a> DurationContext<'a> {
    pub fn new(decimal: &'a DecimalContext) -> Self {
        Self {
            decimal,
            signals: Cell::new(DecimalSignals::EMPTY),
        }
    }
    pub fn signals(&self) -> DecimalSignals {
        self.signals.get()
    }
    fn apply(&self, op: DecimalOp, a: &Dec, b: Option<&Dec>) -> Result<Dec> {
        let rhs = b.map(Dec::to_core);
        match self
            .decimal
            .apply(op, &a.to_core(), rhs.as_ref(), &duration_numeric_limits())
        {
            Ok(out) => {
                self.signals.set(self.signals.get() | out.signals);
                Dec::from_core(out.value)
            }
            Err(e) => {
                if let NumericFault::DecimalTrap { signals, .. } = &e {
                    self.signals.set(self.signals.get() | *signals);
                }
                Err(e.into())
            }
        }
    }
    fn floor_div12(&self, a: &Dec) -> Result<Dec> {
        let q = self.apply(DecimalOp::Divide, a, Some(&Dec::from(12)))?;
        match &q.special {
            Some(DecimalValue::Infinity { .. }) => {
                return Err(DurationError::Overflow("cannot convert Infinity to integer").into());
            }
            Some(DecimalValue::NaN { .. }) => {
                return Err(DurationError::Value("cannot convert NaN to integer").into());
            }
            _ => {}
        }
        let divisor = pow10((-q.e).max(0) as u32);
        let mut integral = if q.e >= 0 {
            &q.n * pow10(q.e as u32)
        } else {
            &q.n / &divisor
        };
        if q.n < BigInt::from(0) && &q.n % &divisor != BigInt::from(0) {
            integral -= 1;
        }
        Ok(Dec {
            n: integral,
            e: 0,
            special: None,
        })
    }
    // Decimal divmod has an integral quotient and a context-finalized remainder.
    // DivisionImpossible is an exception condition, not a second sticky signal.
    fn split_month_count(&self, total: &Dec) -> Result<(Dec, Dec)> {
        let nan = || {
            Dec::from_core(DecimalValue::NaN {
                negative: false,
                signaling: false,
                payload: String::new(),
            })
        };
        match &total.special {
            Some(DecimalValue::NaN { .. }) => return Ok((total.clone(), total.clone())),
            Some(DecimalValue::Infinity { .. }) => {
                self.signals.set(
                    self.signals.get() | DecimalSignals::from(DecimalSignal::InvalidOperation),
                );
                if self.decimal.traps.contains(DecimalSignal::InvalidOperation) {
                    return Err(NumericFault::DecimalTrap {
                        primary: DecimalSignal::InvalidOperation,
                        signals: DecimalSignals::from(DecimalSignal::InvalidOperation),
                    }
                    .into());
                }
                return Ok((total.clone(), nan()?));
            }
            _ => {}
        }
        let (years, months) = total.split12();
        if years.n.to_string().len() > self.decimal.precision {
            self.signals
                .set(self.signals.get() | DecimalSignals::from(DecimalSignal::InvalidOperation));
            if self.decimal.traps.contains(DecimalSignal::InvalidOperation) {
                return Err(DurationError::DivisionImpossible.into());
            }
            return Ok((nan()?, nan()?));
        }
        Ok((years, self.apply(DecimalOp::Plus, &months, None)?))
    }
    fn normalize_months(&self, years: Dec, months: Dec) -> Result<(Dec, Dec)> {
        let a = self.apply(DecimalOp::Subtract, &months, Some(&Dec::zero()))?;
        let carry = self.floor_div12(&a)?;
        let product = self.apply(DecimalOp::Multiply, &carry, Some(&Dec::from(12)))?;
        let rest = self.apply(DecimalOp::Subtract, &a, Some(&product))?;
        let rest = self.apply(DecimalOp::Add, &rest, Some(&Dec::zero()))?;
        Ok((self.apply(DecimalOp::Add, &years, Some(&carry))?, rest))
    }
    pub fn parse(&self, raw: &str) -> Result<Option<DurationValue>> {
        ensure!(
            raw.len() <= 10_000,
            "rdflib_literal_normalizer_operation_budget"
        );
        let (years, months, micros, negative, calendar) = if let Some(parts) = duration_parts(raw) {
            let y = Dec::parse(parts.years)?;
            let m = Dec::parse(parts.months)?;
            let calendar = y.n != BigInt::from(0) || m.n != BigInt::from(0);
            // The original normalizes months before constructing timedelta;
            // a later timedelta overflow does not erase reached Decimal flags.
            let (y, m) = if calendar {
                self.normalize_months(y, m)?
            } else {
                (y, m)
            };
            let Some(us) = parts.timedelta_micros() else {
                return Ok(None);
            };
            (y, m, us, parts.negative, calendar)
        } else if let Some(value) = raw.strip_prefix('P').and_then(parse_datetime) {
            let (y, m, d) = value.date_parts();
            let [h, min, s, us] = value.time_parts();
            let (y, m) = self.normalize_months(Dec::from(y.into()), Dec::from(m.into()))?;
            (
                y,
                m,
                i128::from(d) * 86_400_000_000
                    + i128::from(h) * 3_600_000_000
                    + i128::from(min) * 60_000_000
                    + i128::from(s) * 1_000_000
                    + i128::from(us),
                false,
                true,
            )
        } else {
            return Ok(None);
        };
        let (years, months) = if negative && calendar {
            let y = self.apply(DecimalOp::Subtract, &Dec::zero(), Some(&years))?;
            let m = self.apply(DecimalOp::Subtract, &Dec::zero(), Some(&months))?;
            self.normalize_months(y, m)?
        } else {
            (years, months)
        };
        let Some(delta) = Timedelta::from_micros(if negative { -micros } else { micros }) else {
            return Ok(None);
        };
        Ok(Some(if calendar {
            DurationValue::Calendar {
                years,
                months,
                delta,
            }
        } else {
            DurationValue::Fixed(delta)
        }))
    }
    pub fn format(&self, value: &DurationValue) -> Result<String> {
        let mut body = String::new();
        let mut minus = false;
        let mut has_year_month = false;
        if let DurationValue::Calendar { years, months, .. } = value {
            has_year_month = !years.to_core().is_zero() || !months.to_core().is_zero();
            if has_year_month {
                let years12 = self.apply(DecimalOp::Multiply, years, Some(&Dec::from(12)))?;
                let mut total = self.apply(DecimalOp::Add, &years12, Some(months))?;
                if total.to_core().is_negative() && !total.to_core().is_zero() {
                    minus = true;
                    total = self.apply(DecimalOp::Minus, &total, None)?;
                }
                let (new_years, new_months) = self.split_month_count(&total)?;
                if new_years.to_core().python_truth() {
                    body.push_str(&format!("{}Y", new_years.display()));
                }
                // Keep the original tdt.months predicate, including its
                // fractional-year formatting behavior.
                if months.to_core().python_truth() {
                    body.push_str(&format!("{}M", new_months.display()));
                }
            }
        }
        let us = value.delta().as_micros();
        if us < 0 {
            if minus {
                return Err(DurationError::Value(
                    "Duration cannot have negative years and negative days",
                )
                .into());
            }
            if has_year_month {
                return Err(DurationError::Value(
                    "Duration cannot have positive years and months but negative days",
                )
                .into());
            }
            minus = true;
        }
        Ok(format_duration_tail(body, us.unsigned_abs(), minus))
    }
    pub fn value_eq(&self, a: &DurationValue, b: &DurationValue) -> Result<bool> {
        let total = |years: &Dec, months: &Dec| -> Result<Dec> {
            let y = self.apply(DecimalOp::Multiply, years, Some(&Dec::from(12)))?;
            self.apply(DecimalOp::Add, &y, Some(months))
        };
        let equals = match (a, b) {
            (
                DurationValue::Calendar {
                    years: ay,
                    months: am,
                    ..
                },
                DurationValue::Calendar {
                    years: by,
                    months: bm,
                    ..
                },
            ) => {
                let (a, b) = (total(ay, am)?, total(by, bm)?);
                let relation = self.decimal.compare_numeric(
                    semantica_numeric::NumericRelation::Equal,
                    &semantica_numeric::NumericValue::Decimal(a.to_core()),
                    &semantica_numeric::NumericValue::Decimal(b.to_core()),
                    &duration_numeric_limits(),
                );
                self.signals.set(self.signals.get() | relation.signals);
                relation.result?
            }
            (DurationValue::Calendar { years, months, .. }, DurationValue::Fixed(_))
            | (DurationValue::Fixed(_), DurationValue::Calendar { years, months, .. }) => {
                years.to_core().is_zero() && months.to_core().is_zero()
            }
            _ => true,
        };
        Ok(equals && a.delta() == b.delta())
    }
}
pub fn normalize_duration(raw: &str) -> Result<String> {
    let decimal = DecimalContext::default();
    let context = DurationContext::new(&decimal);
    match context.parse(raw)? {
        Some(value) => context.format(&value),
        None => Ok(raw.into()),
    }
}

#[cfg(test)]
mod context_receiving {
    use super::*;
    #[test]
    fn calendar_divmod_original_contexts() {
        let rows: Vec<_> = include_str!("fixtures/calendar_context_divmod.tsv")
            .lines()
            .collect();
        assert_eq!(rows.len(), 21);
        for row in rows {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 5);
            let decimal = DecimalContext {
                precision: fields[0].parse().unwrap(),
                traps: DecimalSignals::EMPTY,
                ..DecimalContext::default()
            };
            let context = DurationContext::new(&decimal);
            let value = DurationDecimal::parse(fields[1]).unwrap();
            let (years, months) = context.split_month_count(&value).unwrap();
            let mut signals: Vec<_> = context.signals().iter().map(|s| format!("{s:?}")).collect();
            signals.sort();
            assert_eq!(
                (years.display(), months.display(), signals.join(",")),
                (fields[2].into(), fields[3].into(), fields[4].into()),
                "original divmod {row}"
            );
        }
    }
}

impl DurationContext<'_> {
    // xsd_datetime.Duration.__add__/__rsub__: change calendar fields first,
    // clamp the day in the resulting month, then apply the fixed timedelta.
    fn shifted_date(
        &self,
        date: chrono::NaiveDate,
        value: &DurationValue,
        subtract: bool,
    ) -> Result<chrono::NaiveDate> {
        use chrono::Datelike;
        let DurationValue::Calendar { years, months, .. } = value else {
            return Ok(date);
        };
        for v in [years, months] {
            let float =
                semantica_numeric::binary64_from_decimal(&v.to_core(), &duration_numeric_limits())?;
            if !float.is_finite() || float.fract() != 0.0 {
                return Err(DurationError::Value(
                    "fractional years or months not supported for date calculations",
                )
                .into());
            }
        }
        let op = if subtract {
            DecimalOp::Subtract
        } else {
            DecimalOp::Add
        };
        let month = self.apply(op, &Dec::from(i128::from(date.month())), Some(months))?;
        let a = self.apply(DecimalOp::Subtract, &month, Some(&Dec::from(1)))?;
        let carry = self.floor_div12(&a)?;
        let product = self.apply(DecimalOp::Multiply, &carry, Some(&Dec::from(12)))?;
        let rest = self.apply(DecimalOp::Subtract, &a, Some(&product))?;
        let month = self.apply(DecimalOp::Add, &rest, Some(&Dec::from(1)))?;
        let integer = |v: &Dec| {
            if v.e >= 0 {
                &v.n * pow10(v.e as u32)
            } else {
                &v.n / pow10((-v.e) as u32)
            }
        };
        let year = BigInt::from(date.year())
            + if subtract {
                -integer(years)
            } else {
                integer(years)
            }
            + carry.n;
        // datetime.replace converts the Python integer to C int before it
        // checks the calendar year; these are different original exceptions.
        let year = i32::try_from(year)
            .map_err(|_| DurationError::Overflow("Python int too large to convert to C int"))?;
        if !(1..=9999).contains(&year) {
            return Err(DurationError::YearRange(year).into());
        }
        let month = u32::try_from(integer(&month))?;
        let mut day = date.day();
        loop {
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                return Ok(date);
            }
            ensure!(day > 1, "unreceived calendar replacement exception");
            day -= 1;
        }
    }
    pub fn apply_to_date(
        &self,
        date: super::Date,
        value: &DurationValue,
        subtract: bool,
    ) -> Result<super::Date> {
        use chrono::Datelike;
        let date = self.shifted_date(date.0, value, subtract)?;
        let (days, _, _) = value.delta().parts();
        let days = if subtract { -days } else { days };
        let date = date
            .checked_add_signed(chrono::Duration::days(days))
            .ok_or(DurationError::Overflow("date value out of range"))?;
        if !(1..=9999).contains(&date.year()) {
            return Err(DurationError::Overflow("date value out of range").into());
        }
        Ok(super::Date(date))
    }
    pub fn apply_to_datetime(
        &self,
        date: super::DateTime,
        value: &DurationValue,
        subtract: bool,
    ) -> Result<super::DateTime> {
        use chrono::{Datelike, Timelike};
        let shifted = self.shifted_date(date.date, value, subtract)?;
        let instant = shifted
            .and_hms_micro_opt(date.time.h, date.time.m, date.time.s, date.time.us)
            .ok_or_else(|| anyhow::anyhow!("invalid received datetime cache"))?;
        let delta = value.delta().as_micros();
        let delta = if subtract { -delta } else { delta };
        let delta =
            i64::try_from(delta).map_err(|_| DurationError::Overflow("date value out of range"))?;
        let result = instant
            .checked_add_signed(chrono::Duration::microseconds(delta))
            .ok_or(DurationError::Overflow("date value out of range"))?;
        if !(1..=9999).contains(&result.year()) {
            return Err(DurationError::Overflow("date value out of range").into());
        }
        Ok(super::DateTime {
            date: result.date(),
            time: super::Time {
                h: result.hour(),
                m: result.minute(),
                s: result.second(),
                us: result.nanosecond() / 1000,
                offset: date.time.offset,
            },
        })
    }
}
