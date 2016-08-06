//! Utilities and parsers for dealing with ASCII data in `u8` format.

use std::ops::{Add, Mul};

use conv::{
    NoError,
    ValueFrom,
};
use conv::errors::UnwrapOk;

use types::{Buffer, Input};
use combinators::option;
use parsers::{
    SimpleResult,
    satisfy,
    take_while,
    take_while1,
};

/// Lowercase ASCII predicate.
#[inline]
pub fn is_lowercase(c: u8) -> bool {
    b'a' <= c && c <= b'z'
}

/// Uppercase ASCII character predicate.
#[inline]
pub fn is_uppercase(c: u8) -> bool {
    b'A' <= c && c <= b'Z'
}

/// ASCII whitespace predicate.
///
/// Includes:
///
/// * Horizontal tab (TAB)
/// * Line feed (LF)
/// * Vertical tab (VT)
/// * Form feed (FF)
/// * Carriage return (CR)
/// * Space
#[inline]
pub fn is_whitespace(c: u8) -> bool {
    9 <= c && c <= 13 || c == b' '
}

/// A predicate which matches either space (' ') or horizontal tab ('\t').
#[inline]
pub fn is_horizontal_space(c: u8) -> bool {
    c == b'\t' || c == b' '
}

/// A predicate matching eithr a newline ('\n') or a carriage return ('\r').
#[inline]
pub fn is_end_of_line(c: u8) -> bool {
    c == b'\n' || c == b'\r'
}

/// ASCII digit predicate.
#[inline]
pub fn is_digit(c: u8) -> bool {
    b'0' <= c && c <= b'9'
}

/// ASCII alphabetic predicate.
#[inline]
pub fn is_alpha(c: u8) -> bool {
    is_lowercase(c) || is_uppercase(c)
}

/// ASCII alphanumeric predicate.
#[inline]
pub fn is_alphanumeric(c: u8) -> bool {
    is_alpha(c) || is_digit(c)
}

/// Skips over whitespace.
///
/// Matches zero-length.
///
/// # Example
///
/// ```
/// use chomp::parse_only;
/// use chomp::ascii::skip_whitespace;
///
/// assert_eq!(parse_only(skip_whitespace, b" \t "), Ok(()));
/// ```
#[inline]
pub fn skip_whitespace<I: Input<Token=u8>>(i: I) -> SimpleResult<I, ()> {
    take_while(i, is_whitespace).map(|_| ())
}

/// Parses a single digit.
///
/// # Note
///
/// The result is the code of the digit, no conversion takes place.
///
/// # Example
///
/// ```
/// use chomp::parse_only;
/// use chomp::ascii::digit;
///
/// assert_eq!(parse_only(digit, b"1"), Ok(b'1'));
/// ```
#[inline]
pub fn digit<I: Input<Token=u8>>(i: I) -> SimpleResult<I, u8> {
    satisfy(i, is_digit)
}

/// Parses a number with an optional leading '+' or '-'.
///
/// # Note
///
/// The from `i8` bound here is usually smaller than the number parser requirement for signed
/// integers (usually the smallest possible signed is `i16`).
///
/// # Example
///
/// ```
/// use chomp::parse_only;
/// use chomp::ascii::{decimal, signed};
///
/// let r: Result<i16, _> = parse_only(|i| signed(i, decimal), b"-123");
///
/// assert_eq!(r, Ok(-123i16));
/// ```
#[inline]
pub fn signed<I: Input<Token=u8>, T, F>(i: I, f: F) -> SimpleResult<I, T>
  where T: Copy + ValueFrom<i8, Err=NoError> + Add<Output=T> + Mul<Output=T>,
        F: FnOnce(I) -> SimpleResult<I, T> {
    option(i,
           |i| satisfy(i, |c| c == b'-' || c == b'+')
               .map(|s| T::value_from(if s == b'+' { 1 } else { -1 }).unwrap_ok()),
           T::value_from(1).unwrap_ok())
        .bind(|i, sign| f(i).map(|num| sign * num))
}

/// Parses a series of digits and converts them to an integer.
///
/// # Note
///
/// The `T` type must be larger than `u8` if it is signed.
///
/// # Example
///
/// ```
/// use chomp::parse_only;
/// use chomp::ascii::decimal;
///
/// let r = parse_only(decimal::<u8>, b"123");
///
/// assert_eq!(r, Ok(123u8));
/// ```
// TODO: Use methods on `Buffer` to implement `to_decimal`
#[inline]
pub fn decimal<'a, I: Input<Token=u8>, T: Copy + ValueFrom<u8, Err=NoError> + Add<Output=T> + Mul<Output=T>>(i: I) -> SimpleResult<I, T> {
    take_while1(i, is_digit).map(|b| to_decimal(b.iter()))
}

/// Internal function converting a `[u8]` to the given integer type `T`.
///
/// # Notes
///
/// * The slice must not contain any other characters besides 0 to 9.
/// * The `T` type must be larger than `u8` if it is signed.
#[inline]
fn to_decimal<T: Copy + ValueFrom<u8, Err=NoError> + Add<Output=T> + Mul<Output=T>, I: Iterator<Item=u8>>(iter: I) -> T {
    iter.fold(T::value_from(0).unwrap_ok(), |a, n| a * T::value_from(10).unwrap_ok() + T::value_from(n - b'0').unwrap_ok())
}

#[cfg(test)]
mod test {
    use super::to_decimal;

    macro_rules! test_to_decimal {
        ( $($n:ty),+ ) => { $(
            assert_eq!(to_decimal::<$n, _>(b"".iter().cloned()), 0);
            assert_eq!(to_decimal::<$n, _>(b"0".iter().cloned()), 0);
            assert_eq!(to_decimal::<$n, _>(b"1".iter().cloned()), 1);
            assert_eq!(to_decimal::<$n, _>(b"2".iter().cloned()), 2);
            assert_eq!(to_decimal::<$n, _>(b"10".iter().cloned()), 10);
            assert_eq!(to_decimal::<$n, _>(b"20".iter().cloned()), 20);
            assert_eq!(to_decimal::<$n, _>(b"25".iter().cloned()), 25);
        )+ }
    }

    #[test]
    fn test_to_decimal_u8() {
        test_to_decimal!(u8, u16, u32, u64, i16, i32, i64);
    }
}
