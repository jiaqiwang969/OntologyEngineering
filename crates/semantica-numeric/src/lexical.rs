//! Exact construction and bounded decimal text conversion.
//!
//! Decimal syntax follows the pinned Python 3.13.11 `_pydecimal` constructor
//! and its Unicode regular expression, including permissive underscores. This
//! does not certify the C `_decimal` constructor's domain or runtime context.

use crate::{DecimalValue, NumericFault, NumericLimits};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{ToPrimitive, Zero};
use semantica_json::{python_character_class, python_decimal_digit, python_strip};

fn resource(operation: &'static str) -> NumericFault {
    NumericFault::ResourceLimit { operation }
}

fn input_characters(text: &str, limits: &NumericLimits) -> Result<usize, NumericFault> {
    let count = text.chars().count();
    if count > limits.max_input_characters {
        return Err(resource("numeric input characters"));
    }
    Ok(count)
}

fn string_with_capacity(capacity: usize) -> Result<String, NumericFault> {
    let mut result = String::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| resource("numeric text allocation"))?;
    Ok(result)
}

/// Construct a Decimal without context rounding or exponent clamping.
///
/// In the reference, strip precedes removal of *all* underscores; it is not
/// repeated afterward. Unicode decimal digits are accepted in the coefficient,
/// exponent and NaN payload. Invalid syntax is an adapter-visible fault, not an
/// implicitly quieted NaN. The adapter owns ConversionSyntax signal handling.
pub fn parse_decimal_exact(
    text: &str,
    limits: &NumericLimits,
) -> Result<DecimalValue, NumericFault> {
    let count = input_characters(text, limits)?;
    let mut normalized = string_with_capacity(count)?;
    for character in python_strip(text).chars() {
        if character == '_' {
            continue;
        }
        let ascii = if let Some(digit) = python_decimal_digit(character) {
            b'0' + digit
        } else if character.is_ascii() {
            character.to_ascii_lowercase() as u8
        } else {
            // Python re.IGNORECASE is Unicode-aware: the dotted/dotless I
            // and long S also match the letters in Infinity and sNaN.
            match character {
                '\u{0130}' | '\u{0131}' => b'i',
                '\u{017f}' => b's',
                _ => return Err(NumericFault::InvalidLexical),
            }
        };
        normalized.push(char::from(ascii));
    }
    let (negative, body) = match normalized.as_bytes().first() {
        Some(b'-') => (true, &normalized[1..]),
        Some(b'+') => (false, &normalized[1..]),
        _ => (false, normalized.as_str()),
    };
    if body == "inf" || body == "infinity" {
        return Ok(DecimalValue::Infinity { negative });
    }
    let nan = body
        .strip_prefix("snan")
        .map(|payload| (true, payload))
        .or_else(|| body.strip_prefix("nan").map(|payload| (false, payload)));
    if let Some((signaling, payload)) = nan {
        if !payload.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(NumericFault::InvalidLexical);
        }
        if payload.len() > limits.max_coefficient_digits {
            return Err(resource("Decimal NaN payload digits"));
        }
        return Ok(DecimalValue::NaN {
            negative,
            signaling,
            payload: payload.trim_start_matches('0').to_owned(),
        });
    }

    let bytes = body.as_bytes();
    let mut cursor = 0;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let integer_end = cursor;
    let fraction_start = if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        cursor
    } else {
        cursor
    };
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let fraction_end = cursor;
    let fraction_digits = fraction_end - fraction_start;
    let coefficient_digits = integer_end + fraction_digits;
    if coefficient_digits == 0 {
        return Err(NumericFault::InvalidLexical);
    }
    let exponent_text = if bytes.get(cursor) == Some(&b'e') {
        cursor += 1;
        let exponent_start = cursor;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let digits_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == digits_start {
            return Err(NumericFault::InvalidLexical);
        }
        Some(&body[exponent_start..cursor])
    } else {
        None
    };
    if cursor != bytes.len() {
        return Err(NumericFault::InvalidLexical);
    }
    if coefficient_digits > limits.max_coefficient_digits {
        return Err(resource("Decimal coefficient digits"));
    }
    let mut coefficient_text = string_with_capacity(coefficient_digits)?;
    coefficient_text.push_str(&body[..integer_end]);
    coefficient_text.push_str(&body[fraction_start..fraction_end]);
    let coefficient = BigUint::parse_bytes(coefficient_text.as_bytes(), 10)
        .ok_or(NumericFault::InvalidLexical)?;
    let exponent = match exponent_text {
        Some(text) => {
            BigInt::parse_bytes(text.as_bytes(), 10).ok_or(NumericFault::InvalidLexical)?
        }
        None => BigInt::zero(),
    } - BigInt::from(fraction_digits);
    Ok(DecimalValue::Finite {
        negative,
        coefficient,
        exponent,
    })
}

fn integer_whitespace(character: char) -> bool {
    // CPython's integer input maps non-ASCII Unicode whitespace to ASCII
    // space, then uses its ASCII ctype table. Unlike str.strip(), that table
    // excludes U+001C..U+001F.
    if character.is_ascii() {
        matches!(character, '\t'..='\r' | ' ')
    } else {
        python_character_class(character).1
    }
}

/// Parse Python's base-ten integer text grammar with an explicit digit limit.
/// `None` and `Some(0)` disable that limit; resource budgets remain independent.
/// Signs and underscores do not count as digits, but leading zeros do.
pub fn parse_integer(
    text: &str,
    limits: &NumericLimits,
    integer_text_limit: Option<usize>,
) -> Result<BigInt, NumericFault> {
    input_characters(text, limits)?;
    let text = text.trim_matches(integer_whitespace);
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let mut digit_count = 0;
    let mut previous_digit = false;
    for character in body.chars() {
        if python_decimal_digit(character).is_some() {
            digit_count += 1;
            previous_digit = true;
        } else if character == '_' && previous_digit {
            previous_digit = false;
        } else {
            return Err(NumericFault::InvalidLexical);
        }
    }
    if !previous_digit {
        return Err(NumericFault::InvalidLexical);
    }
    if integer_text_limit.is_some_and(|limit| limit != 0 && digit_count > limit) {
        return Err(NumericFault::IntegerTextLimit);
    }
    if digit_count > limits.max_coefficient_digits {
        return Err(resource("integer coefficient digits"));
    }
    let mut digits = string_with_capacity(digit_count)?;
    for character in body.chars() {
        if let Some(digit) = python_decimal_digit(character) {
            digits.push(char::from(b'0' + digit));
        }
    }
    let magnitude =
        BigUint::parse_bytes(digits.as_bytes(), 10).ok_or(NumericFault::InvalidLexical)?;
    Ok(BigInt::from_biguint(
        if negative { Sign::Minus } else { Sign::Plus },
        magnitude,
    ))
}

/// Convert without first allocating an unbounded BigUint decimal string.
/// Each input word updates a little-endian decimal digit buffer, and every
/// growth is checked before it occurs. The bit test avoids walking an enormous
/// externally constructed integer that plainly exceeds the requested budget.
fn bounded_digits(
    value: &BigUint,
    limit: usize,
    limit_fault: NumericFault,
) -> Result<String, NumericFault> {
    if limit == 0 || u128::from(value.bits()) > (limit as u128) * 4 {
        return Err(limit_fault);
    }
    let capacity = usize::try_from(value.bits())
        .unwrap_or(usize::MAX)
        .min(limit)
        .max(1);
    let mut digits = Vec::new();
    digits
        .try_reserve_exact(capacity)
        .map_err(|_| resource("numeric digit allocation"))?;
    digits.push(0_u8);
    for word in value.iter_u32_digits().rev() {
        let mut carry = u64::from(word);
        for digit in &mut digits {
            let expanded = u64::from(*digit) * (1_u64 << 32) + carry;
            *digit = (expanded % 10) as u8;
            carry = expanded / 10;
        }
        while carry != 0 {
            if digits.len() == limit {
                return Err(limit_fault);
            }
            // The initial capacity bounds every possible output digit: a
            // nonzero integer has no more decimal digits than binary bits.
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    digits.reverse();
    for digit in &mut digits {
        *digit += b'0';
    }
    // All entries are constructed ASCII decimal digits; this cannot fail.
    String::from_utf8(digits).map_err(|_| NumericFault::InvalidOperands)
}

fn checked_output_length(
    components: &[usize],
    limits: &NumericLimits,
) -> Result<usize, NumericFault> {
    let length = components.iter().try_fold(0_usize, |total, part| {
        total
            .checked_add(*part)
            .ok_or_else(|| resource("numeric output characters"))
    })?;
    if length > limits.max_output_characters {
        return Err(resource("numeric output characters"));
    }
    Ok(length)
}

fn start_output(length: usize, negative: bool) -> Result<String, NumericFault> {
    let mut output = string_with_capacity(length)?;
    if negative {
        output.push('-');
    }
    Ok(output)
}

/// Format with Python Decimal's unqualified `f` semantics, without rounding.
/// Scale and negative zero are retained. A zero with a positive exponent is
/// printed as `0` or `-0`; huge exponents are checked before zero expansion.
pub fn format_decimal_fixed(
    value: &DecimalValue,
    limits: &NumericLimits,
) -> Result<String, NumericFault> {
    match value {
        DecimalValue::Infinity { negative } => {
            let length = checked_output_length(&[usize::from(*negative), 8], limits)?;
            let mut output = start_output(length, *negative)?;
            output.push_str("Infinity");
            Ok(output)
        }
        DecimalValue::NaN {
            negative,
            signaling,
            payload,
        } => {
            if !payload.bytes().all(|byte| byte.is_ascii_digit()) || payload.starts_with('0') {
                return Err(NumericFault::InvalidOperands);
            }
            if payload.len() > limits.max_coefficient_digits {
                return Err(resource("Decimal NaN payload digits"));
            }
            let prefix = if *signaling { "sNaN" } else { "NaN" };
            let length = checked_output_length(
                &[usize::from(*negative), prefix.len(), payload.len()],
                limits,
            )?;
            let mut output = start_output(length, *negative)?;
            output.push_str(prefix);
            output.push_str(payload);
            Ok(output)
        }
        DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        } => {
            let sign_length = usize::from(*negative);
            let digit_limit = limits
                .max_coefficient_digits
                .min(limits.max_output_characters.saturating_sub(sign_length));
            let digits = bounded_digits(
                coefficient,
                digit_limit,
                resource("Decimal coefficient or output digits"),
            )?;
            if exponent.sign() != Sign::Minus {
                // Positive-exponent zero does not require exponent conversion
                // to a machine integer and never materializes those zeros.
                let zeros = if coefficient.is_zero() {
                    0
                } else {
                    exponent
                        .to_usize()
                        .ok_or_else(|| resource("numeric output characters"))?
                };
                let length = checked_output_length(&[sign_length, digits.len(), zeros], limits)?;
                let mut output = start_output(length, *negative)?;
                output.push_str(&digits);
                output.extend(std::iter::repeat('0').take(zeros));
                return Ok(output);
            }
            let scale = exponent
                .magnitude()
                .to_usize()
                .ok_or_else(|| resource("numeric output characters"))?;
            if scale >= digits.len() {
                let length = checked_output_length(&[sign_length, 2, scale], limits)?;
                let mut output = start_output(length, *negative)?;
                output.push_str("0.");
                output.extend(std::iter::repeat('0').take(scale - digits.len()));
                output.push_str(&digits);
                Ok(output)
            } else {
                let length = checked_output_length(&[sign_length, digits.len(), 1], limits)?;
                let mut output = start_output(length, *negative)?;
                let split = digits.len() - scale;
                output.push_str(&digits[..split]);
                output.push('.');
                output.push_str(&digits[split..]);
                Ok(output)
            }
        }
    }
}

/// Canonical base-ten integer output with independent text and resource limits.
pub fn format_integer(
    value: &BigInt,
    limits: &NumericLimits,
    integer_text_limit: Option<usize>,
) -> Result<String, NumericFault> {
    let negative = value.sign() == Sign::Minus;
    let sign_length = usize::from(negative);
    let mut digit_limit = limits
        .max_coefficient_digits
        .min(limits.max_output_characters.saturating_sub(sign_length));
    let mut fault = resource("integer coefficient or output digits");
    if let Some(limit) = integer_text_limit.filter(|limit| *limit != 0) {
        if limit < digit_limit {
            digit_limit = limit;
            fault = NumericFault::IntegerTextLimit;
        }
    }
    let digits = bounded_digits(value.magnitude(), digit_limit, fault)?;
    let length = checked_output_length(&[sign_length, digits.len()], limits)?;
    let mut output = start_output(length, negative)?;
    output.push_str(&digits);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> DecimalValue {
        parse_decimal_exact(text, &NumericLimits::default()).unwrap()
    }

    #[test]
    fn exact_decimal_retains_scale_signed_zero_and_unbounded_exponent() {
        assert_eq!(
            decimal("-000.００"),
            DecimalValue::Finite {
                negative: true,
                coefficient: BigUint::zero(),
                exponent: BigInt::from(-2),
            }
        );
        assert_eq!(
            decimal("１２.٣０e+４"),
            DecimalValue::Finite {
                negative: false,
                coefficient: BigUint::from(1230_u32),
                exponent: BigInt::from(2),
            }
        );
        let limits = NumericLimits::default();
        let text = "1234567890123456789012345678901234567890";
        assert_eq!(format_decimal_fixed(&decimal(text), &limits).unwrap(), text);
        assert_eq!(
            decimal("1e100000000000000000000"),
            DecimalValue::Finite {
                negative: false,
                coefficient: BigUint::from(1_u8),
                exponent: BigInt::parse_bytes(b"100000000000000000000", 10).unwrap(),
            }
        );
    }

    #[test]
    fn decimal_constructor_uses_strip_then_unrestricted_underscore_removal() {
        assert_eq!(
            decimal(" \u{001c}_１__._２_e_+_３_\u{00a0}"),
            decimal("1.2e3")
        );
        for invalid in ["_ 1 _", "", ".", "1e", "1e+", "1\0", "１．２", "−1"] {
            assert_eq!(
                parse_decimal_exact(invalid, &NumericLimits::default()),
                Err(NumericFault::InvalidLexical),
                "{invalid:?}"
            );
        }
        assert_eq!(decimal("1."), decimal("1"));
        assert_eq!(decimal("1.e2"), decimal("1e2"));
        assert_eq!(decimal(".1"), decimal("0.1"));
    }

    #[test]
    fn decimal_specials_preserve_sign_signaling_payload_and_unicode_casefold() {
        assert_eq!(decimal("-ıNf"), DecimalValue::Infinity { negative: true });
        assert_eq!(
            decimal("İnfinity"),
            DecimalValue::Infinity { negative: false }
        );
        assert_eq!(
            decimal("-ſNaN００１０"),
            DecimalValue::NaN {
                negative: true,
                signaling: true,
                payload: "10".into()
            }
        );
        assert_eq!(
            decimal("+NaN000"),
            DecimalValue::NaN {
                negative: false,
                signaling: false,
                payload: String::new()
            }
        );
        for invalid in ["Inf0", "qNaN", "sNaN-1", "NaN 1", "Nan.1", "infinıty!"] {
            assert_eq!(
                parse_decimal_exact(invalid, &NumericLimits::default()),
                Err(NumericFault::InvalidLexical)
            );
        }
    }

    #[test]
    fn integer_grammar_has_separate_underscore_and_ascii_whitespace_rules() {
        let limits = NumericLimits::default();
        assert_eq!(
            parse_integer("\u{00a0}\t-١_２_000\r", &limits, None).unwrap(),
            BigInt::from(-12000)
        );
        assert_eq!(
            parse_integer("-000", &limits, None).unwrap(),
            BigInt::zero()
        );
        for invalid in [
            "\u{001c}1",
            "1\u{001f}",
            "_1",
            "1_",
            "1__2",
            "+_1",
            "1_ 2",
            "0x10",
            "1.0",
            "1e2",
            "1\0",
        ] {
            assert_eq!(
                parse_integer(invalid, &limits, None),
                Err(NumericFault::InvalidLexical),
                "{invalid:?}"
            );
        }
        assert_eq!(decimal("\u{001c}1\u{001f}"), decimal("1"));
    }

    #[test]
    fn integer_text_limit_counts_leading_zeros_but_not_sign_or_underscores() {
        let limits = NumericLimits::default();
        assert_eq!(
            parse_integer("-１_２_３", &limits, Some(3)).unwrap(),
            BigInt::from(-123)
        );
        assert_eq!(
            parse_integer("0000", &limits, Some(3)),
            Err(NumericFault::IntegerTextLimit)
        );
        assert_eq!(
            parse_integer("0000", &limits, Some(0)).unwrap(),
            BigInt::zero()
        );
        assert_eq!(
            format_integer(&BigInt::from(-123), &limits, Some(3)).unwrap(),
            "-123"
        );
        assert_eq!(
            format_integer(&BigInt::from(1234), &limits, Some(3)),
            Err(NumericFault::IntegerTextLimit)
        );
    }

    #[test]
    fn fixed_format_preserves_fractional_scale_and_collapses_positive_exponent_zero() {
        let limits = NumericLimits::default();
        for (input, expected) in [
            ("1.2300", "1.2300"),
            ("-0.00", "-0.00"),
            ("1E2", "100"),
            ("1E-3", "0.001"),
            ("12E-2", "0.12"),
            ("0E3", "0"),
            ("-0E100000000000000000000", "-0"),
            ("-Inf", "-Infinity"),
            ("-sNaN0042", "-sNaN42"),
            ("NaN000", "NaN"),
        ] {
            assert_eq!(
                format_decimal_fixed(&decimal(input), &limits).unwrap(),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn numeric_lexical_budgets_are_checked_before_expansion_and_count_codepoints() {
        let mut limits = NumericLimits {
            max_input_characters: 2,
            ..NumericLimits::default()
        };
        assert_eq!(
            parse_integer("１２", &limits, None).unwrap(),
            BigInt::from(12)
        );
        assert!(matches!(
            parse_integer("123", &limits, None),
            Err(NumericFault::ResourceLimit { .. })
        ));
        limits.max_output_characters = 5;
        assert_eq!(
            format_decimal_fixed(&decimal("1e-3"), &limits).unwrap(),
            "0.001"
        );
        assert!(matches!(
            format_decimal_fixed(&decimal("1e-4"), &limits),
            Err(NumericFault::ResourceLimit { .. })
        ));
        assert!(matches!(
            format_decimal_fixed(&decimal("1e100000000000000000000"), &limits),
            Err(NumericFault::ResourceLimit { .. })
        ));
        assert!(matches!(
            format_decimal_fixed(&decimal("0e-100000000000000000000"), &limits),
            Err(NumericFault::ResourceLimit { .. })
        ));
        let huge = BigUint::from(1_u8) << 100_000_usize;
        assert!(matches!(
            format_integer(&BigInt::from(huge), &limits, None),
            Err(NumericFault::ResourceLimit { .. })
        ));
        limits.max_output_characters = 3;
        assert_eq!(
            format_integer(&BigInt::from(999), &limits, None).unwrap(),
            "999"
        );
        assert!(matches!(
            format_integer(&BigInt::from(1000), &limits, None),
            Err(NumericFault::ResourceLimit { .. })
        ));
    }

    #[test]
    fn integer_format_handles_word_boundaries_without_losing_digits() {
        let limits = NumericLimits::default();
        for text in [
            "0",
            "4294967295",
            "4294967296",
            "18446744073709551616",
            "-1000000000000000000000000000000000000000000000000000000000001",
        ] {
            let integer = parse_integer(text, &limits, None).unwrap();
            assert_eq!(format_integer(&integer, &limits, None).unwrap(), text);
        }
    }
}
