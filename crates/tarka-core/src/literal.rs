//! Typed literals the way spareval (and so oxi-gen and HOLOS) writes them.
//!
//! spareval turns every typed value an expression produces into its own value type
//! and back, so a value is written in canonical form:
//!
//! | datatype                        | written as                     | example                    |
//! |---------------------------------|--------------------------------|----------------------------|
//! | `xsd:boolean`                   | `true` / `false`               | `1` → `true`               |
//! | `xsd:integer` and its subtypes  | no sign, no leading zeros      | `+007` → `7`               |
//! | `xsd:decimal`                   | no trailing zeros, no `.0`     | `5000.00` → `5000`         |
//! | `xsd:double`, `xsd:float`       | shortest form, no exponent     | `1.5E3` → `1500`           |
//! | `xsd:dateTime`                  | `Z` for UTC, fraction trimmed  | `…+00:00` → `…Z`           |
//!
//! Values outside the lexical space, and all other datatypes, are written as they
//! are. tarka's own conversions (the OTTR route) use the same forms, so every route
//! writes the same terms. One difference is deliberate: spareval also changes the
//! datatype of `xsd:int`, `xsd:byte`, … to `xsd:integer`; tarka keeps the datatype.

use std::str::FromStr;

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNodeRef};
use oxsdatatypes::{Boolean, DateTime, Decimal, Double, Float, Integer};

/// The datatypes derived from `xsd:integer`.
const INTEGER_SUBTYPES: [NamedNodeRef<'static>; 12] = [
    xsd::LONG,
    xsd::INT,
    xsd::SHORT,
    xsd::BYTE,
    xsd::NON_NEGATIVE_INTEGER,
    xsd::POSITIVE_INTEGER,
    xsd::NON_POSITIVE_INTEGER,
    xsd::NEGATIVE_INTEGER,
    xsd::UNSIGNED_LONG,
    xsd::UNSIGNED_INT,
    xsd::UNSIGNED_SHORT,
    xsd::UNSIGNED_BYTE,
];

fn display<T: FromStr + ToString>(lexical: &str) -> Option<String> {
    T::from_str(lexical).ok().map(|v| v.to_string())
}

/// The canonical form of `lexical` as a value of `datatype`.
///
/// Returns `None` if `datatype` is one tarka canonicalises and `lexical` is not in
/// its lexical space, and `lexical` unchanged for every other datatype.
pub fn canonical_lexical(datatype: NamedNodeRef<'_>, lexical: &str) -> Option<String> {
    if datatype == xsd::BOOLEAN {
        display::<Boolean>(lexical)
    } else if datatype == xsd::INTEGER || INTEGER_SUBTYPES.contains(&datatype) {
        display::<Integer>(lexical)
    } else if datatype == xsd::DECIMAL {
        display::<Decimal>(lexical)
    } else if datatype == xsd::DOUBLE {
        display::<Double>(lexical)
    } else if datatype == xsd::FLOAT {
        display::<Float>(lexical)
    } else if datatype == xsd::DATE_TIME {
        display::<DateTime>(lexical)
    } else {
        Some(lexical.to_owned())
    }
}

/// A typed literal with `lexical` in canonical form when it is a valid value, and as
/// written otherwise (an ill-typed literal, as `STRDT` makes).
pub fn typed_literal(lexical: &str, datatype: NamedNodeRef<'_>) -> Literal {
    if datatype == xsd::STRING {
        return Literal::new_simple_literal(lexical);
    }
    let lexical = canonical_lexical(datatype, lexical).unwrap_or_else(|| lexical.to_owned());
    Literal::new_typed_literal(lexical, datatype)
}

/// Whether `lexical` is in the lexical space of `datatype`, for the datatypes tarka
/// canonicalises (and `true` for every other datatype).
pub fn is_valid(datatype: NamedNodeRef<'_>, lexical: &str) -> bool {
    canonical_lexical(datatype, lexical).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(dt: NamedNodeRef<'_>, lexical: &str) -> Option<String> {
        canonical_lexical(dt, lexical)
    }

    /// Each case was checked against oxi-gen v0.5.1 (spareval) cell by cell.
    #[test]
    fn forms_match_spareval() {
        let cases: &[(NamedNodeRef<'_>, &str, Option<&str>)] = &[
            (xsd::INTEGER, "007", Some("7")),
            (xsd::INTEGER, "+7", Some("7")),
            (xsd::INTEGER, "-0", Some("0")),
            (xsd::INTEGER, " 7 ", None),
            (xsd::INTEGER, "7.0", None),
            (xsd::DECIMAL, "5000.00", Some("5000")),
            (xsd::DECIMAL, "0.50", Some("0.5")),
            (xsd::DECIMAL, "+1.50", Some("1.5")),
            (xsd::DECIMAL, "-0.0", Some("0")),
            (xsd::DECIMAL, ".5", Some("0.5")),
            (xsd::DECIMAL, "5.", Some("5")),
            (xsd::DECIMAL, "1e3", None),
            (xsd::DOUBLE, "1e3", Some("1000")),
            (xsd::DOUBLE, "1.5E3", Some("1500")),
            (xsd::DOUBLE, "-1.25E-2", Some("-0.0125")),
            (xsd::DOUBLE, "-0.0", Some("-0")),
            (xsd::DOUBLE, "1e21", Some("1000000000000000000000")),
            (xsd::DOUBLE, "1e-7", Some("0.0000001")),
            (xsd::DOUBLE, "INF", Some("INF")),
            (xsd::DOUBLE, "NaN", Some("NaN")),
            (xsd::FLOAT, "0.1", Some("0.1")),
            (xsd::FLOAT, "1.0e0", Some("1")),
            (xsd::BOOLEAN, "1", Some("true")),
            (xsd::BOOLEAN, "0", Some("false")),
            (xsd::BOOLEAN, "TRUE", None),
            (xsd::DATE_TIME, "2019-03-01T09:30:00+00:00", Some("2019-03-01T09:30:00Z")),
            (xsd::DATE_TIME, "2019-03-01T09:30:00-00:00", Some("2019-03-01T09:30:00Z")),
            (xsd::DATE_TIME, "2019-03-01T09:30:00.250Z", Some("2019-03-01T09:30:00.25Z")),
            (xsd::DATE_TIME, "2019-03-01T09:30:00.0Z", Some("2019-03-01T09:30:00Z")),
            (xsd::DATE_TIME, "2019-03-01T24:00:00", Some("2019-03-02T00:00:00")),
            (xsd::DATE_TIME, "2019-12-31T24:00:00Z", Some("2020-01-01T00:00:00Z")),
            (xsd::DATE_TIME, "2019-03-01T09:30:00+01:00", Some("2019-03-01T09:30:00+01:00")),
            (xsd::DATE_TIME, "2020-02-29T00:00:00", Some("2020-02-29T00:00:00")),
            (xsd::DATE_TIME, "2019-02-29T00:00:00", None),
            (xsd::DATE_TIME, "2019-03-01", None),
            (xsd::DATE_TIME, "9223372036854775808", None),
            (xsd::DATE, "2019-03-01+00:00", Some("2019-03-01+00:00")),
            (xsd::G_YEAR, "0998", Some("0998")),
            (xsd::INT, "007", Some("7")),
        ];
        for (dt, lexical, expected) in cases {
            assert_eq!(canon(*dt, lexical).as_deref(), *expected, "{dt} {lexical:?}");
        }
    }

    #[test]
    fn typed_literals_keep_bad_values_and_their_datatype() {
        assert_eq!(typed_literal("abc", xsd::DECIMAL), Literal::new_typed_literal("abc", xsd::DECIMAL));
        assert_eq!(typed_literal("129.90", xsd::DECIMAL), Literal::new_typed_literal("129.9", xsd::DECIMAL));
        assert_eq!(typed_literal("007", xsd::INT), Literal::new_typed_literal("7", xsd::INT));
        assert_eq!(typed_literal("x", xsd::STRING), Literal::new_simple_literal("x"));
    }
}
