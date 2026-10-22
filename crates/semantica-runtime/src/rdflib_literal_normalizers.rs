//! Pure RDFLib 7.6 literal lexical normalization. No dataset or execution owner.
//! Date/time parsing follows CPython 3.13.11; duration follows RDFLib
//! xsd_datetime.py, including its Decimal(precision=28), float timedelta and
//! lexical preservation on failed casts. Source/license records accompany this
//! source-only overlay. This file does not certify arbitrary Python contexts.
use anyhow::{Result, bail, ensure};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

pub(crate) fn normalize(datatype: &str, text: &str) -> Result<String> {
    ensure!(
        text.len() <= 10_000,
        "rdflib_literal_normalizer_operation_budget"
    );
    let Some(kind) = datatype.strip_prefix(XSD) else {
        bail!("rdflib_literal_normalizer_external_module_required");
    };
    let value = match kind {
        "date" => semantica_temporal::normalize_date(text),
        "time" => semantica_temporal::normalize_time(text),
        "dateTime" => semantica_temporal::parse_datetime(text).map(|v| v.isoformat()),
        "duration" | "dayTimeDuration" | "yearMonthDuration" => return duration(text),
        "hexBinary" => {
            if text.len() % 2 == 0 && text.bytes().all(|c| c.is_ascii_hexdigit()) {
                Some(text.to_ascii_lowercase())
            } else {
                None
            }
        }
        "base64Binary" => base64_normalized(text),
        _ => bail!("rdflib_literal_normalizer_wrong_datatype"),
    };
    Ok(value.unwrap_or_else(|| text.to_owned()))
}

fn base64_normalized(text: &str) -> Option<String> {
    if !text.is_ascii() {
        return None;
    }
    let mut out = Vec::new();
    let mut group = Vec::<u8>::new();
    let mut pads = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        };
        if let Some(value) = value {
            pads = 0;
            group.push(value);
            if group.len() == 4 {
                out.extend([
                    group[0] << 2 | group[1] >> 4,
                    group[1] << 4 | group[2] >> 2,
                    group[2] << 6 | group[3],
                ]);
                group.clear();
            }
        } else if byte == b'=' {
            if group.len() >= 2 {
                pads += 1;
                if pads + group.len() >= 4 {
                    out.push(group[0] << 2 | group[1] >> 4);
                    if group.len() == 3 {
                        out.push(group[1] << 4 | group[2] >> 2);
                    }
                    group.clear();
                    break;
                }
            }
        }
    }
    if !group.is_empty() {
        return None;
    }
    use base64::Engine;
    Some(base64::engine::general_purpose::STANDARD.encode(out))
}

fn duration(raw: &str) -> Result<String> {
    semantica_temporal::normalize_duration(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use semantica_numeric::DecimalValue;
    use semantica_temporal::DurationDecimal as Dec;

    #[test]
    fn duration_decimal_adapter_retains_legacy_scale_zero_and_display() {
        // The adapter owns duration str-style formatting. It deliberately
        // differs from the shared core's RDF fixed formatting for these zeros.
        for (n, e, expected) in [
            (0, 3, "0E+3"),
            (0, -7, "0E-7"),
            (1, -6, "0.000001"),
            (1, -7, "1E-7"),
        ] {
            assert_eq!(Dec::from_parts(n.into(), e).unwrap().display(), expected);
        }
        let negative_zero = DecimalValue::Finite {
            negative: true,
            coefficient: 0_u8.into(),
            exponent: (-2).into(),
        };
        assert_eq!(Dec::from_core(negative_zero).unwrap().display(), "0.00");
        assert_eq!(Dec::parse("1.5000").unwrap().display(), "1.5000");
    }

    #[test]
    fn duration_division_rounds_before_exact_floor_extraction() {
        // 10^30 / 12 = 833...333.333..., but precision 28 first produces
        // 833...330. Taking exact floor before the Decimal operation differs.
        let huge = Dec::from_parts(1.into(), 30).unwrap();
        assert_eq!(
            huge.floor_div12().unwrap().display(),
            "83333333333333333333333333330"
        );
        assert_eq!(Dec::from(-13).floor_div12().unwrap().display(), "-2");
        // The real month path rounds its input before divide/floor as well.
        assert_eq!(duration("P11.999999999999999999999999999M").unwrap(), "P1Y");
        assert_eq!(
            duration("-P11.999999999999999999999999999M").unwrap(),
            "-P1Y"
        );
    }

    #[test]
    fn shared_duration_arithmetic_preserves_frozen_constructor_boundaries() {
        // Existing original constructor fixtures remain the authoritative full
        // snapshot tests. These selected observed lexicals expose adapter risks.
        for (input, expected) in [
            ("P13M", "P1Y1M"),
            ("P1.50Y", "P1Y"),
            ("P0.10Y", "P0D"),
            ("P00001.5000M", "P1.5000M"),
            ("P1.25Y2.50M", "P1Y5.50M"),
            ("P0.0000000000000000000000000001M", "P1E-28M"),
            ("P2M1Y", "P2M1Y"),
        ] {
            assert_eq!(duration(input).unwrap(), expected, "{input}");
        }
        assert!(
            duration("P12345678901234567890123456789Y")
                .unwrap_err()
                .to_string()
                .contains("DivisionImpossible")
        );
        assert!(
            duration("-P1Y2M3D")
                .unwrap_err()
                .to_string()
                .contains("negative years and negative days")
        );
    }

    #[test]
    fn duration_numeric_limits_cover_the_existing_ten_thousand_byte_contract() {
        let input = format!("P0.{}1M", "0".repeat(9995));
        assert_eq!(input.len(), 10_000);
        assert_eq!(
            normalize(&format!("{XSD}duration"), &input).unwrap(),
            "P1E-9996M"
        );
        let too_long = format!("P0.{}1M", "0".repeat(9996));
        assert!(
            normalize(&format!("{XSD}duration"), &too_long)
                .unwrap_err()
                .to_string()
                .contains("operation_budget")
        );
    }
}
