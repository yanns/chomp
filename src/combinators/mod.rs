//! Basic combinators.

#[macro_use]
mod macros;

pub mod bounded;

use std::iter::FromIterator;

use {ParseResult, Input};

use primitives::State;
use primitives::{IntoInner, Primitives};

/// Applies the parser ``p`` exactly ``num`` times collecting all items into `T: FromIterator`.
///
/// ```
/// use chomp::{U8Result, ParseError, Error, Input, parse_only, count, token, take_remainder};
///
/// fn parse(i: Input<u8>) -> U8Result<Vec<u8>> {
///     count(i, 2, |i| token(i, b'a'))
/// }
///
/// assert_eq!(parse_only(parse, b"a  "), Err(ParseError::Error(b"  ", Error::expected(b'a'))));
/// assert_eq!(parse_only(parse, b"aa "), Ok(vec![b'a', b'a']));
///
/// let with_remainder = |i| parse(i).bind(|i, d| take_remainder(i).map(|r| (r, d)));
///
/// assert_eq!(parse_only(with_remainder, b"aaa"), Ok((&b"a"[..], vec![b'a', b'a'])));
/// ```
#[inline]
pub fn count<I: Input, T, E, F, U>(i: I, num: usize, p: F) -> ParseResult<I, T, E>
  where F: FnMut(I) -> ParseResult<I, U, E>,
        T: FromIterator<U> {
    bounded::many(i, num, p)
}

/// Tries the parser ``f``, on success it yields the parsed value, on failure ``default`` will be
/// yielded instead.
///
/// Incomplete state is propagated. Backtracks on error.
///
/// ```
/// use chomp::{Input, U8Result, parse_only, option, token};
///
/// fn f(i: Input<u8>) -> U8Result<u8> {
///     option(i, |i| token(i, b'a'), b'd')
/// }
///
/// assert_eq!(parse_only(f, b"abc"), Ok(b'a'));
/// assert_eq!(parse_only(f, b"bbc"), Ok(b'd'));
/// ```
#[inline]
pub fn option<I: Input, T, E, F>(i: I, f: F, default: T) -> ParseResult<I, T, E>
  where F: FnOnce(I) -> ParseResult<I, T, E> {
    let m = i.mark();

    // TODO: Make macro? Shared with or
    match f(i).into_inner() {
        State::Data(b, d)    => b.ret(d),
        State::Error(b, _)   => {
            b.restore(m).ret(default)
        },
        State::Incomplete(b, n) => {
            let b = b.restore(m);

            if b.is_end() {
                b.ret(default)
            } else {
                b.incomplete(n)
            }
        },
    }
}

/// Tries to match the parser ``f``, if ``f`` fails it tries ``g``. Returns the success value of
/// the first match, otherwise the error of the last one if both fail.
///
/// Incomplete state is propagated from the first one to report incomplete.
///
/// If multiple `or` combinators are used in the same expression, consider using the `parse!` macro
/// and its alternation operator (`<|>`).
///
/// ```
/// use chomp::{ParseError, Error, parse_only, or, token};
///
/// let p = |i| or(i,
///             |i| token(i, b'a'),
///             |i| token(i, b'b'));
///
/// assert_eq!(parse_only(&p, b"abc"), Ok(b'a'));
/// assert_eq!(parse_only(&p, b"bbc"), Ok(b'b'));
/// assert_eq!(parse_only(&p, b"cbc"), Err(ParseError::Error(b"cbc", Error::expected(b'b'))));
/// ```
#[inline]
pub fn or<I: Input, T, E, F, G>(i: I, f: F, g: G) -> ParseResult<I, T, E>
  where F: FnOnce(I) -> ParseResult<I, T, E>,
        G: FnOnce(I) -> ParseResult<I, T, E> {
    let m = i.mark();

    match f(i).into_inner() {
        State::Data(b, d)    => b.ret(d),
        State::Error(b, _)   => g(b.restore(m)),
        State::Incomplete(b, n) => {
            let b = b.restore(m);

            if b.is_end() {
                g(b)
            } else {
                b.incomplete(n)
            }
        },
    }
}

/// Parses many instances of ``f`` until it does no longer match, collecting all matches into the
/// type `T: FromIterator`.
///
/// Note: If the last parser succeeds on the last input item then this parser is still considered
/// incomplete if the input flag END_OF_INPUT is not set as there might be more data to fill.
///
/// Note: Allocates data.
///
/// ```
/// use chomp::{parse_only, token, many, take_while1};
///
/// let r: Result<Vec<_>, _> = parse_only(|i| many(i,
///     |i| take_while1(i, |c| c != b',' && c != b' ')
///         .bind(|i, c| token(i, b',')
///                      .map(|_| c))),
///     b"a,bc,cd ");
///
/// assert_eq!(r, Ok(vec![&b"a"[..], &b"bc"[..]]));
/// ```
#[inline]
pub fn many<I: Input, T, E, F, U>(i: I, f: F) -> ParseResult<I, T, E>
  where F: FnMut(I) -> ParseResult<I, U, E>,
        T: FromIterator<U> {
    bounded::many(i, .., f)
}

/// Parses at least one instance of ``f`` and continues until it does no longer match, collecting
/// all matches into the type `T: FromIterator`.
///
/// Note: If the last parser succeeds on the last input item then this parser is still considered
/// incomplete as there might be more data to fill.
///
/// Note: Allocates data.
///
/// ```
/// use chomp::{ParseError, Error, parse_only, token, many1, take_while1};
///
/// let p = |i| many1(i, |i| take_while1(i, |c| c != b',' && c != b' ')
///             .bind(|i, c| token(i, b',')
///                          .map(|_| c)));
///
/// assert_eq!(parse_only(&p, b"a "), Err(ParseError::Error(b" ", Error::expected(b','))));
/// assert_eq!(parse_only(&p, b"a, "), Ok(vec![&b"a"[..]]));
/// ```
#[inline]
pub fn many1<I: Input, T, E, F, U>(i: I, f: F) -> ParseResult<I, T, E>
  where F: FnMut(I) -> ParseResult<I, U, E>,
        T: FromIterator<U> {
    bounded::many(i, 1.., f)
}

/// Applies the parser `R` zero or more times, separated by the parser `F`. All matches from `R`
/// will be collected into the type `T: FromIterator`.
///
/// If the separator or parser registers error or incomplete this parser stops and yields the
/// collected value.
///
/// Incomplete will be propagated from `R` if end of input has not been read.
///
/// ```
/// use chomp::{parse_only, sep_by, token};
/// use chomp::ascii::decimal;
///
/// let r: Result<Vec<u8>, _> = parse_only(|i| sep_by(i, decimal, |i| token(i, b';')), b"91;03;20");
///
/// assert_eq!(r, Ok(vec![91, 03, 20]));
/// ```
#[inline]
pub fn sep_by<I: Input, T, E, R, F, U, N, V>(i: I, p: R, sep: F) -> ParseResult<I, T, E>
  where T: FromIterator<U>,
        E: From<N>,
        R: FnMut(I) -> ParseResult<I, U, E>,
        F: FnMut(I) -> ParseResult<I, V, N> {
    bounded::sep_by(i, .., p, sep)
}


/// Applies the parser `R` one or more times, separated by the parser `F`. All matches from `R`
/// will be collected into the type `T: FromIterator`.
///
/// If the separator or parser registers error or incomplete this parser stops and yields the
/// collected value if at least one item has been read.
///
/// Incomplete will be propagated from `R` if end of input has not been read.
///
/// ```
/// use chomp::{parse_only, sep_by1, token};
/// use chomp::ascii::decimal;
///
/// let r: Result<Vec<u8>, _> = parse_only(|i| sep_by1(i, decimal, |i| token(i, b';')), b"91;03;20");
///
/// assert_eq!(r, Ok(vec![91, 03, 20]));
/// ```
#[inline]
pub fn sep_by1<I: Input, T, E, R, F, U, N, V>(i: I, p: R, sep: F) -> ParseResult<I, T, E>
  where T: FromIterator<U>,
        E: From<N>,
        R: FnMut(I) -> ParseResult<I, U, E>,
        F: FnMut(I) -> ParseResult<I, V, N> {
    bounded::sep_by(i, 1.., p, sep)
}

/// Applies the parser `R` multiple times until the parser `F` succeeds and returns a
/// `T: FromIterator` populated by the values yielded by `R`. Consumes the matched part of `F`.
///
/// This parser is considered incomplete if the parser `R` is considered incomplete.
///
/// Errors from `R` are propagated.
///
/// ```
/// use chomp::{parse_only, many_till, any, token};
///
/// let r: Result<Vec<u8>, _> = parse_only(|i| many_till(i, any, |i| token(i, b';')), b"abc;def");
///
/// assert_eq!(r, Ok(vec![b'a', b'b', b'c']));
/// ```
#[inline]
pub fn many_till<I: Input, T, E, R, F, U, N, V>(i: I, p: R, end: F) -> ParseResult<I, T, E>
  where T: FromIterator<U>,
        R: FnMut(I) -> ParseResult<I, U, E>,
        F: FnMut(I) -> ParseResult<I, V, N> {
    bounded::many_till(i, .., p, end)
}

/// Runs the given parser until it fails, discarding matched input.
///
/// Incomplete state will be propagated.
///
/// This is more efficient compared to using ``many`` and then just discarding the result as
/// ``many`` allocates a separate data structure to contain the data before proceeding.
///
/// ```
/// use chomp::{parse_only, skip_many, token};
///
/// let r = parse_only(|i| skip_many(i, |i| token(i, b'a')).then(|i| token(i, b'b')), b"aaaabc");
///
/// assert_eq!(r, Ok(b'b'));
/// ```
#[inline]
pub fn skip_many<I: Input, T, E, F>(i: I, f: F) -> ParseResult<I, (), E>
  where F: FnMut(I) -> ParseResult<I, T, E> {
    //bounded::skip_many(i, .., f)
    //FIXME: Implement
    unimplemented!()
}

/// Runs the given parser until it fails, discarding matched input, expects at least one match.
///
/// Incomplete state will be propagated. Will propagate the error if it occurs during the first
/// attempt.
///
/// This is more efficient compared to using ``many1`` and then just discarding the result as
/// ``many1`` allocates a separate data structure to contain the data before proceeding.
///
/// ```
/// use chomp::{ParseError, Error, parse_only, skip_many1, token};
///
/// let p = |i| skip_many1(i, |i| token(i, b'a')).bind(|i, _| token(i, b'b'));
///
/// assert_eq!(parse_only(&p, b"aaaabc"), Ok(b'b'));
/// assert_eq!(parse_only(&p, b"abc"), Ok(b'b'));
///
/// assert_eq!(parse_only(&p, b"bc"), Err(ParseError::Error(b"bc", Error::expected(b'a'))));
/// ```
#[inline]
pub fn skip_many1<I: Input, T, E, F>(i: I, f: F) -> ParseResult<I, (), E>
  where F: FnMut(I) -> ParseResult<I, T, E> {
      // FIXME: Implement
    //bounded::skip_many(i, 1.., f)
    unimplemented!()
}

/// Returns the result of the given parser as well as the slice which matched it.
///
/// ```
/// use chomp::{parse_only, matched_by};
/// use chomp::ascii::decimal;
///
/// assert_eq!(parse_only(|i| matched_by(i, decimal), b"123"), Ok((&b"123"[..], 123u32)));
/// ```
#[inline]
pub fn matched_by<I: Input, T, E, F>(i: I, f: F) -> ParseResult<I, (I::Buffer, T), E>
  where F: FnOnce(I) -> ParseResult<I, T, E> {
    // TODO: Implement
    unimplemented!()
        /*
    let buf = i.buffer();

    match f(i.clone()).into_inner() {
        State::Data(b, t) => {
            // b is remainder, find out how much the parser used
            let diff = buf.len() - b.buffer().len();
            let n    = &buf[..diff];

            b.ret((n, t))
        },
        State::Error(b, e)   => i.replace(b).err(e),
        State::Incomplete(n) => i.incomplete(n),
    }
    */
}

/// Applies the parser `F` without consuming any input.
///
/// ```
/// use chomp::{parse_only, take};
/// use chomp::combinators::look_ahead;
///
/// let p = |i| look_ahead(i, |i| take(i, 4)).bind(|i, t| take(i, 7).map(|u| (t, u)));
///
/// assert_eq!(parse_only(p, b"testing"), Ok((&b"test"[..], &b"testing"[..])));
/// ```
#[inline]
pub fn look_ahead<I: Input, T, E, F>(i: I, f: F) -> ParseResult<I, T, E>
  where F: FnOnce(I) -> ParseResult<I, T, E> {
    let m = i.mark();

    match f(i).into_inner() {
        State::Data(b, t)       => b.restore(m).ret(t),
        State::Error(b, t)      => b.restore(m).err(t),
        State::Incomplete(b, n) => b.restore(m).incomplete(n),
    }
}

#[cfg(test)]
mod test {
    use {Input, ParseResult};
    use primitives::State;
    use primitives::input::{new_buf, DEFAULT, END_OF_INPUT};
    use primitives::IntoInner;
    use super::*;

    use parsers::{any, take, token, string};

    #[test]
    fn option_test() {
        assert_eq!(option(new_buf(DEFAULT, b""), any, b'-').into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));
        assert_eq!(option(new_buf(DEFAULT, b"a"), any, b'-').into_inner(), State::Data(new_buf(DEFAULT, b""), b'a'));
        assert_eq!(option(new_buf(DEFAULT, b""), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 2));
        assert_eq!(option(new_buf(DEFAULT, b"a"), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Incomplete(new_buf(DEFAULT, b"a"), 1));
        assert_eq!(option(new_buf(DEFAULT, b"ab"), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Data(new_buf(DEFAULT, b""), vec![b'a', b'b']));

        assert_eq!(option(new_buf(DEFAULT, b"a"), |i| token(i, b' ').map_err(|_| "token_err"), b'-').into_inner(), State::Data(new_buf(DEFAULT, b"a"), b'-'));

        assert_eq!(option(new_buf(END_OF_INPUT, b""), any, b'-').into_inner(), State::Data(new_buf(END_OF_INPUT, b""), b'-'));
        assert_eq!(option(new_buf(END_OF_INPUT, b"a"), any, b'-').into_inner(), State::Data(new_buf(END_OF_INPUT, b""), b'a'));
        assert_eq!(option(new_buf(END_OF_INPUT, b""), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![]));
        assert_eq!(option(new_buf(END_OF_INPUT, b"a"), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Data(new_buf(END_OF_INPUT, b"a"), vec![]));
        assert_eq!(option(new_buf(END_OF_INPUT, b"ab"), |i| take(i, 2).map(ToOwned::to_owned), vec![]).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'b']));

        assert_eq!(option(new_buf(END_OF_INPUT, b"a"), |i| token(i, b' ').map_err(|_| "token_err"), b'-').into_inner(), State::Data(new_buf(END_OF_INPUT, b"a"), b'-'));
    }

    #[test]
    fn or_test() {
        assert_eq!(or(new_buf(DEFAULT, b""), any, any).into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));
        assert_eq!(or(new_buf(DEFAULT, b"a"), any, any).into_inner(), State::Data(new_buf(DEFAULT, b""), b'a'));
        assert_eq!(or(new_buf(DEFAULT, b"a"), |i| take(i, 2), |i| take(i, 1)).into_inner(), State::Incomplete(new_buf(DEFAULT, b"a"), 1));
        assert_eq!(or(new_buf(DEFAULT, b"ab"), |i| take(i, 2), |i| take(i, 1)).into_inner(), State::Data(new_buf(DEFAULT, b""), &b"ab"[..]));
        assert_eq!(or(new_buf(DEFAULT, b"a"), |i| token(i, b'a'), |i| token(i, b'b')).into_inner(), State::Data(new_buf(DEFAULT, b""), b'a'));
        assert_eq!(or(new_buf(DEFAULT, b"b"), |i| token(i, b'a'), |i| token(i, b'b')).into_inner(), State::Data(new_buf(DEFAULT, b""), b'b'));
        assert_eq!(or(new_buf(DEFAULT, b"c"), |i| token(i, b'a').map_err(|_| "a err"), |i| token(i, b'b').map_err(|_| "b err")).into_inner(), State::Error(new_buf(DEFAULT, b"c"), "b err"));

        assert_eq!(or(new_buf(END_OF_INPUT, b""), any, any).into_inner(), State::Incomplete(new_buf(END_OF_INPUT, b""), 1));
        assert_eq!(or(new_buf(END_OF_INPUT, b"a"), any, any).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), b'a'));
        assert_eq!(or(new_buf(END_OF_INPUT, b"a"), |i| take(i, 2), |i| take(i, 1)).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), &b"a"[..]));
        assert_eq!(or(new_buf(END_OF_INPUT, b"ab"), |i| take(i, 2), |i| take(i, 1)).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), &b"ab"[..]));
        assert_eq!(or(new_buf(END_OF_INPUT, b"a"), |i| token(i, b'a'), |i| token(i, b'b')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), b'a'));
        assert_eq!(or(new_buf(END_OF_INPUT, b"b"), |i| token(i, b'a'), |i| token(i, b'b')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), b'b'));
        assert_eq!(or(new_buf(END_OF_INPUT, b"c"), |i| token(i, b'a').map_err(|_| "a err"), |i| token(i, b'b').map_err(|_| "b err")).into_inner(), State::Error(new_buf(END_OF_INPUT, b"c"), "b err"));
    }

    #[test]
    fn many_test() {
        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b""), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b"a"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b"aa"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b"bbb"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"bbb"), vec![]));
        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b"abb"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"bb"), vec![b'a']));
        let r: State<_, Vec<_>, _> = many(new_buf(DEFAULT, b"aab"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"b"), vec![b'a', b'a']));

        let r: State<_, Vec<_>, _> = many(new_buf(END_OF_INPUT, b""), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![]));
        let r: State<_, Vec<_>, _> = many(new_buf(END_OF_INPUT, b"a"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![b'a']));
        let r: State<_, Vec<_>, _> = many(new_buf(END_OF_INPUT, b"aa"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'a']));

        let r: State<_, Vec<_>, _> = many(new_buf(END_OF_INPUT, b"aab"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b"b"), vec![b'a', b'a']));
    }

    #[test]
    fn many1_test() {
        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b""), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b"a"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b"aa"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b"bbb"), |i| token(i, b'a').map_err(|_| "token_error")).into_inner();
        assert_eq!(r, State::Error(new_buf(DEFAULT, b"bbb"), "token_error"));
        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b"abb"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"bb"), vec![b'a']));
        let r: State<_, Vec<_>, _> = many1(new_buf(DEFAULT, b"aab"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"b"), vec![b'a', b'a']));

        let r: State<_, Vec<_>, _> = many1(new_buf(END_OF_INPUT, b""), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(END_OF_INPUT, b""), 1));
        let r: State<_, Vec<_>, _> = many1(new_buf(END_OF_INPUT, b"a"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![b'a']));
        let r: State<_, Vec<_>, _> = many1(new_buf(END_OF_INPUT, b"aa"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'a']));

        let r: State<_, Vec<_>, _> = many1(new_buf(END_OF_INPUT, b"aab"), |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b"b"), vec![b'a', b'a']));
    }

    #[test]
    fn count_test() {
        let r: State<_, Vec<_>, _> = count(new_buf(DEFAULT, b""), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(DEFAULT, b"a"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(DEFAULT, b"aa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(DEFAULT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(DEFAULT, b"aaa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b""), vec![b'a', b'a', b'a']));
        let r: State<_, Vec<_>, _> = count(new_buf(DEFAULT, b"aaaa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(DEFAULT, b"a"), vec![b'a', b'a', b'a']));

        let r: State<_, Vec<_>, _> = count(new_buf(END_OF_INPUT, b""), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(END_OF_INPUT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(END_OF_INPUT, b"a"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(END_OF_INPUT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(END_OF_INPUT, b"aa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Incomplete(new_buf(END_OF_INPUT, b""), 1));
        let r: State<_, Vec<_>, _> = count(new_buf(END_OF_INPUT, b"aaa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'a', b'a']));
        let r: State<_, Vec<_>, _> = count(new_buf(END_OF_INPUT, b"aaaa"), 3,  |i| token(i, b'a')).into_inner();
        assert_eq!(r, State::Data(new_buf(END_OF_INPUT, b"a"), vec![b'a', b'a', b'a']));
    }

    #[test]
    fn skip_many1_test() {
        assert_eq!(skip_many1(new_buf(DEFAULT, b"aabc"), |i| token(i, b'a')).into_inner(), State::Data(new_buf(DEFAULT, b"bc"), ()));
        assert_eq!(skip_many1(new_buf(DEFAULT, b"abc"), |i| token(i, b'a')).into_inner(), State::Data(new_buf(DEFAULT, b"bc"), ()));
        assert_eq!(skip_many1(new_buf(DEFAULT, b"bc"), |i| i.err::<(), _>("error")).into_inner(), State::Error(new_buf(DEFAULT, b"bc"), "error"));
        assert_eq!(skip_many1(new_buf(DEFAULT, b"aaa"), |i| token(i, b'a')).into_inner(), State::Incomplete(new_buf(DEFAULT, b"aaa"), 1));
        assert_eq!(skip_many1(new_buf(END_OF_INPUT, b"aabc"), |i| token(i, b'a')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"bc"), ()));
        assert_eq!(skip_many1(new_buf(END_OF_INPUT, b"abc"), |i| token(i, b'a')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"bc"), ()));
        assert_eq!(skip_many1(new_buf(END_OF_INPUT, b"bc"), |i| i.err::<(), _>("error")).into_inner(), State::Error(new_buf(END_OF_INPUT, b"bc"), "error"));
        assert_eq!(skip_many1(new_buf(END_OF_INPUT, b"aaa"), |i| token(i, b'a')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), ()));
    }

    #[test]
    fn many_till_test() {
        assert_eq!(many_till(new_buf(DEFAULT, b"abcd"), any, |i| token(i, b'c')).into_inner(), State::Data(new_buf(DEFAULT, b"d"), vec![b'a', b'b']));
        let r: ParseResult<_, Vec<_>, _> = many_till(new_buf(DEFAULT, b"abd"), any, |i| token(i, b'c'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<u8>, _> = many_till(new_buf(DEFAULT, b"abcd"), |i| i.err(()), |i| token(i, b'c'));
        assert_eq!(r.into_inner(), State::Error(new_buf(DEFAULT, b"abcd"), ()));

        // Variant to make sure error slice is propagated
        let mut n = 0;
        let r: ParseResult<_, Vec<_>, _> = many_till(new_buf(DEFAULT, b"abcd"), |i| if n == 0 { n += 1; any(i).map_err(|_| "any err") } else { i.err("the error") }, |i| token(i, b'c'));
        assert_eq!(r.into_inner(), State::Error(new_buf(DEFAULT, b"bcd"), "the error"));
    }

    #[test]
    fn matched_by_test() {
        assert_eq!(matched_by(new_buf(DEFAULT, b"abc"), any).into_inner(), State::Data(new_buf(DEFAULT, b"bc"), (&b"a"[..], b'a')));
        assert_eq!(matched_by(new_buf(DEFAULT, b"abc"), |i| i.err::<(), _>("my error")).into_inner(), State::Error(new_buf(DEFAULT, b"abc"), "my error"));
        assert_eq!(matched_by(new_buf(DEFAULT, b"abc"), |i| any(i).map_err(|_| "any error").then(|i| i.err::<(), _>("my error"))).into_inner(), State::Error(new_buf(DEFAULT, b"bc"), "my error"));
        assert_eq!(matched_by(new_buf(DEFAULT, b""), any).into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));
    }

    #[test]
    fn sep_by_test() {
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b""), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![]));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"b"), |i| token(i, b'a'), |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"b"), vec![]));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"a"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![b'a']));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"a;c"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'c']));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"a;c;"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b";"), vec![b'a', b'c']));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"a--c-"), any, |i| string(i, b"--")).into_inner(), State::Data(new_buf(END_OF_INPUT, b"-"), vec![b'a', b'c']));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"abc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"bc"), vec![b'a']));
        assert_eq!(sep_by(new_buf(END_OF_INPUT, b"a;bc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"c"), vec![b'a', b'b']));

        assert_eq!(sep_by(new_buf(DEFAULT, b"abc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(DEFAULT, b"bc"), vec![b'a']));
        assert_eq!(sep_by(new_buf(DEFAULT, b"a;bc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(DEFAULT, b"c"), vec![b'a', b'b']));

        // Incomplete becasue there might be another separator or item to be read
        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b""), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"a"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"a;"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"a;c"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"a;c;"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"a--c-"), any, |i| string(i, b"--"));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b"-"), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by(new_buf(DEFAULT, b"aaa--a"), |i| string(i, b"aaa"), |i| string(i, b"--"));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b"a"), 2));

    }

    #[test]
    fn sep_by1_test() {
        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(END_OF_INPUT, b""), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(END_OF_INPUT, b""), 1));

        let r: ParseResult<_, Vec<()>, _> = sep_by1(new_buf(END_OF_INPUT, b"b"), |i| i.err("my err"), |i| token(i, b';').map_err(|_| "token_err"));
        assert_eq!(r.into_inner(), State::Error(new_buf(END_OF_INPUT, b"b"), "my err"));

        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"a"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![b'a']));
        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"a;c"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b""), vec![b'a', b'c']));
        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"a;c;"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b";"), vec![b'a', b'c']));
        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"a--c-"), any, |i| string(i, b"--")).into_inner(), State::Data(new_buf(END_OF_INPUT, b"-"), vec![b'a', b'c']));
        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"abc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"bc"), vec![b'a']));
        assert_eq!(sep_by1(new_buf(END_OF_INPUT, b"a;bc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(END_OF_INPUT, b"c"), vec![b'a', b'b']));

        assert_eq!(sep_by1(new_buf(DEFAULT, b"abc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(DEFAULT, b"bc"), vec![b'a']));
        assert_eq!(sep_by1(new_buf(DEFAULT, b"a;bc"), any, |i| token(i, b';')).into_inner(), State::Data(new_buf(DEFAULT, b"c"), vec![b'a', b'b']));

        // Incomplete becasue there might be another separator or item to be read
        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b""), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"a"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"a;"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"a;c"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"a;c;"), any, |i| token(i, b';'));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b""), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"a--c-"), any, |i| string(i, b"--"));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b"-"), 1));

        let r: ParseResult<_, Vec<_>, _> = sep_by1(new_buf(DEFAULT, b"aaa--a"), |i| string(i, b"aaa"), |i| string(i, b"--"));
        assert_eq!(r.into_inner(), State::Incomplete(new_buf(DEFAULT, b"a"), 2));
    }

    #[test]
    fn look_ahead_test() {
        assert_eq!(look_ahead(new_buf(DEFAULT, b"abc"), any).into_inner(), State::Data(new_buf(DEFAULT, b"abc"), b'a'));
        assert_eq!(look_ahead(new_buf(DEFAULT, b"a"), |i| string(i, b"abc")).into_inner(), State::Incomplete(new_buf(DEFAULT, b"a"), 2));
        assert_eq!(look_ahead(new_buf(DEFAULT, b"aa"), |i| token(i, b'a').then(|i| token(i, b'b')).map_err(|_| "err")).into_inner(), State::Error(new_buf(DEFAULT, b"aa"), "err"));
    }
}
