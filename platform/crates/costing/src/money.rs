//! Money, in satang.
//!
//! Never a float. A contractor's project runs to seven figures in baht and is
//! settled to the satang; `0.1 + 0.2` is the wrong tool for that, and a total
//! that is off by a rounding error is worse than useless here — the whole
//! product rests on the claim that if the numbers do not add up, something is
//! wrong with the reading rather than with us.
//!
//! `i64` satang reaches ±92 trillion baht, which is larger than any project.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub};

use serde::{Deserialize, Serialize};

/// An amount of money, held as satang.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Self = Self(0);

    pub const fn from_satang(satang: i64) -> Self {
        Self(satang)
    }

    pub const fn from_baht(baht: i64) -> Self {
        Self(baht * 100)
    }

    pub const fn satang(self) -> i64 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Reads "1,234.56" and "1234.5" and "2800" — what OCR hands back from a
    /// Thai invoice. Refuses anything else rather than guessing: a misread
    /// amount that silently becomes zero is the failure this crate exists to
    /// catch, so it must not be introduced while parsing.
    pub fn parse(text: &str) -> Result<Self, ParseMoneyError> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ',' && *c != '฿')
            .collect();
        if cleaned.is_empty() {
            return Err(ParseMoneyError(text.to_owned()));
        }

        let (sign, digits) = match cleaned.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, cleaned.as_str()),
        };

        let (baht, satang) = match digits.split_once('.') {
            Some((b, s)) => {
                if s.len() > 2 || s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err(ParseMoneyError(text.to_owned()));
                }
                // "1234.5" is five satang short of "1234.50", not five satang.
                let scaled = if s.len() == 1 {
                    s.parse::<i64>().unwrap() * 10
                } else {
                    s.parse().unwrap()
                };
                (b, scaled)
            }
            None => (digits, 0),
        };

        if baht.is_empty() || !baht.chars().all(|c| c.is_ascii_digit()) {
            return Err(ParseMoneyError(text.to_owned()));
        }
        let baht: i64 = baht.parse().map_err(|_| ParseMoneyError(text.to_owned()))?;

        baht.checked_mul(100)
            .and_then(|b| b.checked_add(satang))
            .map(|total| Self(sign * total))
            .ok_or_else(|| ParseMoneyError(text.to_owned()))
    }

    /// This amount as a share of `whole`, in percent, rounded to one decimal.
    ///
    /// `None` when `whole` is zero: a project with no budget has no percentage
    /// over it, and reporting 0% or infinity would both be lies.
    pub fn percent_of(self, whole: Self) -> Option<f64> {
        if whole.0 == 0 {
            return None;
        }
        Some(((self.0 as f64 / whole.0 as f64) * 1000.0).round() / 10.0)
    }
}

impl Add for Money {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|m| m.0).sum())
    }
}

/// Baht with two decimals and thousands separators — how it is read back to a
/// person, and how it appeared on the document it came from.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let negative = self.0 < 0;
        let abs = self.0.unsigned_abs();
        let baht = abs / 100;
        let satang = abs % 100;

        let digits = baht.to_string();
        let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(c);
        }

        write!(
            f,
            "{}{grouped}.{satang:02}",
            if negative { "-" } else { "" }
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not an amount of money")]
pub struct ParseMoneyError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_ocr_hands_back_from_a_thai_invoice_parses() {
        assert_eq!(
            Money::parse("279,007.85").unwrap(),
            Money::from_satang(27_900_785)
        );
        assert_eq!(Money::parse("2,800.00").unwrap(), Money::from_baht(2800));
        assert_eq!(Money::parse("185000").unwrap(), Money::from_baht(185_000));
        assert_eq!(
            Money::parse(" 1,234.56 ").unwrap(),
            Money::from_satang(123_456)
        );
        assert_eq!(Money::parse("฿520.00").unwrap(), Money::from_baht(520));
    }

    #[test]
    fn one_decimal_place_is_tenths_of_a_baht_not_satang() {
        // "1234.5" is 1234.50, not 1234.05. Getting this backwards would take
        // 45 satang off every such line, quietly.
        assert_eq!(Money::parse("1234.5").unwrap(), Money::from_satang(123_450));
    }

    #[test]
    fn nonsense_is_refused_rather_than_read_as_zero() {
        // The whole point: a misread amount must not become a silent zero.
        for bad in ["", "  ", "abc", "1.234", "1.", ".5", "12-34", "1,2,3.456"] {
            assert!(Money::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn it_reads_back_the_way_it_appeared_on_the_document() {
        assert_eq!(Money::from_satang(27_900_785).to_string(), "279,007.85");
        assert_eq!(Money::from_baht(0).to_string(), "0.00");
        assert_eq!(Money::from_satang(5).to_string(), "0.05");
        assert_eq!(Money::from_baht(1000).to_string(), "1,000.00");
        assert_eq!(Money::from_satang(-123_456).to_string(), "-1,234.56");
    }

    #[test]
    fn a_round_trip_through_text_changes_nothing() {
        for satang in [0, 5, 99, 100, 123_456, 27_900_785, -450] {
            let money = Money::from_satang(satang);
            assert_eq!(Money::parse(&money.to_string()).unwrap(), money);
        }
    }

    #[test]
    fn totals_add_up_exactly() {
        // The sum that decides whether a reading is trusted. In floats,
        // 0.1 + 0.2 != 0.3; here a hundred invoices of 33.33 are exact.
        let lines: Money = std::iter::repeat_n(Money::parse("33.33").unwrap(), 100).sum();
        assert_eq!(lines, Money::parse("3333.00").unwrap());
    }

    #[test]
    fn a_budget_of_nothing_has_no_percentage_over_it() {
        assert_eq!(Money::from_baht(100).percent_of(Money::ZERO), None);
        assert_eq!(
            Money::from_baht(50).percent_of(Money::from_baht(200)),
            Some(25.0)
        );
        assert_eq!(
            Money::from_baht(112).percent_of(Money::from_baht(100)),
            Some(112.0)
        );
    }
}
