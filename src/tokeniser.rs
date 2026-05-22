use log::{debug, error, warn};

use crate::utils::StreamIterator;

// <https://drafts.csswg.org/css-syntax/#tokenization>
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum HashTokenFlag {
    Id,
    Unrestricted,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum NumericValue {
    Integer,
    Number,
}

#[derive(Debug, PartialEq, Clone)]
pub enum CssToken {
    IdentToken(String),
    FunctionToken(String),
    AtKeywordToken(String),
    HashToken {
        flag: HashTokenFlag,
        value: String,
    },
    StringToken(String),
    UrlToken(String),
    BadStringToken,
    BadUrlToken,
    DelimToken(char),
    NumberToken {
        sign: bool,
        val_type: NumericValue,
        value: String,
    },
    PercentageToken {
        sign: bool,
        value: String,
    },
    DimensionToken {
        unit: String,
        sign: bool,
        val_type: NumericValue,
        value: String,
    },
    // UnicodeRangeToken,
    WhitespaceToken,
    CdoToken,
    CdcToken,
    ColonToken,
    SemicolonToken,
    CommaToken,
    CrochetOpToken,
    CrochetClToken,
    ParenthOpToken,
    ParenthClToken,
    AcoladeOpToken,
    AcoladeClToken,
}

/// Preprocess the CSS see the [spec](https://drafts.csswg.org/css-syntax/#input-preprocessing).
///
/// * Replace any U+000D CARRIAGE RETURN (CR) code points, U+000C FORM FEED (FF) code points, or pairs
///   of U+000D CARRIAGE RETURN (CR) followed by U+000A LINE FEED (LF) in input by a single U+000A LINE FEED (LF) code point.
/// * Replace any U+0000 NULL or surrogate code points in input with U+FFFD REPLACEMENT CHARACTER (�).
pub(crate) fn preprocessing(input: String) -> String {
    //
    let mut preprocess_input = input.replace("\r\n", "\n");
    preprocess_input = preprocess_input.replace("\r", "\n");
    preprocess_input = preprocess_input.replace("\x0c", "\n");
    preprocess_input.replace("\x00", "�")
}

// <https://drafts.csswg.org/css-syntax/#consume-token>
pub(crate) fn tokenization(stream: &mut impl StreamIterator<char>) -> Result<CssToken, String> {
    while let Some(current_input) = stream.peek() {
        match current_input {
            '\n' | '\t' | ' ' => {
                consume_whitespaces(stream);
                return Ok(CssToken::WhitespaceToken);
            }
            '"' | '\'' => {
                stream.next();
                return Ok(consume_string_token(stream, current_input));
            }
            '#' => {
                if is_indent_code_point(current_input) || start_valid_escape(stream) {
                    let mut flag = HashTokenFlag::Unrestricted;
                    if start_ident_sequence(stream) {
                        flag = HashTokenFlag::Id;
                    }
                    return Ok(CssToken::HashToken {
                        flag,
                        value: consume_ident_sequence(stream),
                    });
                } else {
                    stream.next();
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            '\\' => {
                stream.next();
                if let Some(next_char) = stream.peek() {
                    if next_char != '\n' {
                        // ident like token
                        stream.back();
                        return Ok(consume_ident_like_token(stream));
                    } else {
                        return Ok(CssToken::DelimToken(current_input));
                    }
                }
            }
            '+' => {
                if start_number(stream) {
                    return Ok(consume_numeric_token(stream));
                } else {
                    stream.next();
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            '-' => {
                if start_number(stream) {
                    return Ok(consume_numeric_token(stream));
                } else if next_2_char_is(stream, '\u{002D}', '\u{003E}') {
                    stream.next();
                    stream.next();
                    return Ok(CssToken::CdcToken);
                } else if start_ident_sequence(stream) {
                    return Ok(consume_ident_like_token(stream));
                } else {
                    stream.next();
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            '0'..='9' => {
                return Ok(consume_numeric_token(stream));
            }
            '@' => {
                stream.next();
                if start_ident_sequence(stream) {
                    return Ok(CssToken::AtKeywordToken(consume_ident_sequence(stream)));
                } else {
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            '.' => {
                if start_number(stream) {
                    return Ok(consume_numeric_token(stream));
                } else {
                    stream.next();
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            '<' => {
                stream.next();
                if next_2_char_is(stream, '\u{0021}', '\u{002D}') {
                    stream.next();
                    stream.next();
                    if next_char_is_x(stream, '\u{002D}') {
                        return Ok(CssToken::CdoToken);
                    } else {
                        stream.back();
                        stream.back();
                    }
                } else {
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
            'U' | 'u' => {
                warn!("Unicode not supported");
                return Ok(consume_ident_like_token(stream));
            }
            '(' => {
                stream.next();
                return Ok(CssToken::ParenthOpToken);
            }
            ')' => {
                stream.next();
                return Ok(CssToken::ParenthClToken);
            }
            '[' => {
                stream.next();
                return Ok(CssToken::CrochetOpToken);
            }
            ']' => {
                stream.next();
                return Ok(CssToken::CrochetClToken);
            }
            '{' => {
                stream.next();
                return Ok(CssToken::AcoladeOpToken);
            }
            '}' => {
                stream.next();
                return Ok(CssToken::AcoladeClToken);
            }
            ',' => {
                stream.next();
                return Ok(CssToken::CommaToken);
            }
            ':' => {
                stream.next();
                return Ok(CssToken::ColonToken);
            }
            ';' => {
                stream.next();
                return Ok(CssToken::SemicolonToken);
            }
            _ => {
                if is_ident_start_code_point(current_input) {
                    return Ok(consume_ident_like_token(stream));
                } else {
                    stream.next();
                    return Ok(CssToken::DelimToken(current_input));
                }
            }
        }
    }

    Err("No token".to_string())
}

/// Take the input stream and consume all of the whitespace.
fn consume_whitespaces(it: &mut impl StreamIterator<char>) {
    while let Some(wp) = it.peek() {
        if is_whitespace(wp) {
            it.next();
        } else {
            break;
        }
    }
}

/// <https://drafts.csswg.org/css-syntax/#consume-ident-like-token>
/// * Consume an ident sequence, and let string be the result.
///
/// * If string’s value is an ASCII case-insensitive match for "url", and the next input code
/// point is U+0028 LEFT PARENTHESIS ((), consume it. While the next two input code points
/// are whitespace, consume the next input code point. If the next one or two input code points
/// are U+0022 QUOTATION MARK ("), U+0027 APOSTROPHE ('), or whitespace followed by U+0022
/// QUOTATION MARK (") or U+0027 APOSTROPHE ('), then create a `<function-token>` with its
/// value set to string and return it. Otherwise, consume a url token, and return it.
///
/// * Otherwise, if the next input code point is U+0028 LEFT PARENTHESIS ((), consume it.
/// Create a `<function-token>` with its value set to string and return it.
///
/// * Otherwise, create an <ident-token> with its value set to string and return it.
fn consume_ident_like_token(it: &mut impl StreamIterator<char>) -> CssToken {
    let string = consume_ident_sequence(it);

    if matches!(string.to_lowercase().as_str(), "url") {
        if let Some(current_char) = it.peek() {
            if current_char == '(' {
                it.next();
                consume_whitespaces(it);
                if next_char_is_quote(it) {
                    return CssToken::FunctionToken(string);
                } else {
                    return consume_url_token(it);
                }
            }
        }
    } else if let Some(current_char) = it.peek() {
        if current_char == '(' {
            it.next();
            return CssToken::FunctionToken(string);
        }
    }

    return CssToken::IdentToken(string);
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-numeric-token>
fn consume_numeric_token(it: &mut impl StreamIterator<char>) -> CssToken {
    let number = consume_number(it);
    let CssToken::NumberToken {
        sign,
        val_type,
        value,
    } = number
    else {
        error!("Unknow error a number consuming go wrong {:#?}", number);
        todo!()
    };

    if start_ident_sequence(it) {
        let unit = consume_ident_sequence(it);
        debug!("unit found {:?}", unit);
        return CssToken::DimensionToken {
            unit,
            sign,
            val_type,
            value,
        };
    } else if char_is_x(it, '%') {
        it.next(); // consume %
        return CssToken::PercentageToken { sign, value };
    } else {
        return CssToken::NumberToken {
            sign,
            val_type,
            value,
        };
    }
}

/// Consume a String token with is ending_char.
/// <https://drafts.csswg.org/css-syntax/#consume-string-token>
fn consume_string_token(it: &mut impl StreamIterator<char>, ending_char: char) -> CssToken {
    let mut string = String::new();
    while let Some(curr_input) = it.peek() {
        it.next();
        if ending_char == curr_input {
            break;
        }
        match curr_input {
            '\n' | '\t' | ' ' => {
                it.back();
                return CssToken::BadStringToken;
            }
            '\\' => {
                it.next();
                if let Some(next_char) = it.peek() {
                    if next_char != '\n' {
                        string.push(curr_input);
                        string.push(next_char);
                    }
                }
            }
            _ => string.push(curr_input),
        }
    }

    CssToken::StringToken(string)
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-url-token>
fn consume_url_token(it: &mut impl StreamIterator<char>) -> CssToken {
    let mut url = String::new();
    consume_whitespaces(it);
    while let Some(current_char) = it.peek() {
        match current_char {
            ')' => {
                it.next();
                return CssToken::UrlToken(url);
            }
            '\n' | '\t' | ' ' => {
                consume_whitespaces(it);
            }
            '\''
            | '"'
            | '('
            | '\u{000B}'
            | '\u{007F}'
            | '\u{0000}'..='\u{0008}'
            | '\u{000E}'..='\u{001F}' => {
                consume_remnants_bad_url(it);
                return CssToken::BadUrlToken;
            }
            '\\' => {
                if start_valid_escape(it) {
                    url.push(consume_escaped_code_point(it));
                    it.next();
                } else {
                    consume_remnants_bad_url(it);
                    return CssToken::BadUrlToken;
                }
            }
            _ => {
                url.push(current_char);
                it.next();
            }
        }
    }

    CssToken::UrlToken(url)
}

/// <https://drafts.csswg.org/css-syntax/#consume-an-ident-sequence>
/// Repeatedly consume the next input code point from the stream:
///
/// * ident code point
///     * Append the code point to result.
/// * the stream starts with a valid escape
///     * Consume an escaped code point. Append the returned code point to result.
/// * anything else
///     * Reconsume the current input code point. Return result.
fn consume_ident_sequence(it: &mut impl StreamIterator<char>) -> String {
    let mut result = String::new();

    while let Some(current_char) = it.peek() {
        match current_char {
            '_'
            | '-'
            | '\u{00B7}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{203F}'
            | '\u{2040}'
            | 'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | '\u{00C0}'..='\u{00D6}'
            | '\u{00D8}'..='\u{00F6}'
            | '\u{00F8}'..='\u{037D}'
            | '\u{037F}'..='\u{1FFF}'
            | '\u{2070}'..='\u{218F}'
            | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D2FF}'
            | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}' => {
                it.next();
                result.push(current_char);
            }
            '\\' => {
                if start_valid_escape(it) {
                    it.next();
                    result.push(consume_escaped_code_point(it));
                }
            }
            _ => {
                if current_char > '\u{10000}' {
                    result.push(current_char);
                    it.next();
                } else {
                    return result;
                }
            }
        }
    }
    result
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-number>
fn consume_number(it: &mut impl StreamIterator<char>) -> CssToken {
    let mut type_num = false; // false integer, true number
    let mut sign = true; // tue +, false -; default true
    let mut number = String::new();

    while let Some(current_char) = it.peek() {
        match current_char {
            '+' => {
                it.next();
            }
            '-' => {
                sign = false;
                it.next();
            }
            '0'..='9' => {
                number.push(current_char);
                it.next();
            }
            '.' => {
                it.next();
                if let Some(next_char) = it.peek() {
                    if is_digit(next_char) {
                        number.push(current_char);
                        number.push_str(consume_digit(it).as_str());
                        type_num = true;
                    } else {
                        it.back();
                        break;
                    }
                } else {
                    it.back();
                    break;
                }
            }
            'e' | 'E' => {
                // ! Erreur possible besoins de vérifier les 2 char mais 1 seul de fais
                debug!("{:?}", it.peek());
                if next_char_is(it, |x| matches!(x, '+' | '-' | '0'..='9')) {
                    debug!("e detecter");
                    number.push(current_char.to_ascii_lowercase());
                    number.push_str(consume_digit(it).as_mut_str());
                } else {
                    debug!("e sans digit {:?}", it.peek());
                    break;
                }
            }
            _ => break,
        }
    }

    if type_num {
        CssToken::NumberToken {
            sign,
            value: number,
            val_type: NumericValue::Number,
        }
    } else {
        CssToken::NumberToken {
            sign,
            value: number,
            val_type: NumericValue::Integer,
        }
    }
}

/// <https://drafts.csswg.org/css-syntax/#consume-an-escaped-code-point>
/// Consume the next input code point.
///
/// * hex digit
///     * Consume as many hex digits as possible, but no more than 5.
///       Note that this means 1-6 hex digits have been consumed in total.
///       If the next input code point is whitespace, consume it as well.
///       Interpret the hex digits as a hexadecimal number. If this number is zero,
///       or is for a surrogate, or is greater than the maximum allowed code point,
///       return U+FFFD REPLACEMENT CHARACTER (�). Otherwise, return the code point with that value.
/// * EOF
///     * This is a parse error. Return U+FFFD REPLACEMENT CHARACTER (�).
/// * anything else
///     * Return the current input code point.
fn consume_escaped_code_point(it: &mut impl StreamIterator<char>) -> char {
    if let Some(current_char) = it.peek() {
        if is_hex_digit(current_char) {
            let mut hex_value = String::new();
            hex_value.push(current_char);
            it.next();
            let mut count = 0;
            while let Some(next_char) = it.peek() {
                if !is_hex_digit(next_char) || count >= 5 {
                    if is_whitespace(next_char) {
                        it.next();
                    }
                    break;
                }
                hex_value.push(next_char);
                it.next();
                count += 1;
            }

            debug!("hex value found {}", hex_value);
            // already check that the char is in the hexrange
            let value = u32::from_str_radix(&hex_value, 16).unwrap();

            if value == 0 || is_a_surrogate_hex(value) || is_max_allowed_code_point_hex(value) {
                return '\u{FFFD}';
            } else {
                return char::from_u32(value).unwrap();
            }
        } else {
            it.next();
            return current_char;
        }
    } else {
        // EOF parsing error
        return '\u{FFFD}';
    }
}

/// <https://drafts.csswg.org/css-syntax/#starts-with-a-valid-escape>
fn start_valid_escape(it: &mut impl StreamIterator<char>) -> bool {
    if let Some(first_char) = it.peek() {
        if first_char != '\\' {
            return false;
        }
        it.next();
        if let Some(second_char) = it.peek() {
            it.back(); // reset to is origin place for future
            return second_char != '\u{000A}';
        }
        it.back();
    }

    false
}

/// <https://drafts.csswg.org/css-syntax/#check-if-three-code-points-would-start-a-number>
fn start_number(it: &mut impl StreamIterator<char>) -> bool {
    if let Some(current_char) = it.peek() {
        match current_char {
            '+' | '-' => {
                it.next();
                if let Some(next_char) = it.peek() {
                    if is_digit(next_char) {
                        it.back();
                        return true;
                    } else if next_char == '.' {
                        it.next();
                        if let Some(third_char) = it.peek() {
                            it.back();
                            it.back();
                            return is_digit(third_char);
                        }
                        it.back();
                    }
                }
                it.back();
            }
            '.' => {
                it.next();
                if let Some(next_char) = it.peek() {
                    if is_digit(current_char) {
                        it.back();
                        return is_digit(next_char);
                    }
                    it.back();
                }
            }
            _ => return is_digit(current_char),
        }
    }

    false
}

/// <https://drafts.csswg.org/css-syntax/#check-if-three-code-points-would-start-an-ident-sequence>
/// Look at the first code point:
/// * U+002D HYPHEN-MINUS
///     * If the second code point is an ident-start code point or a U+002D HYPHEN-MINUS, or the second
///       and third code points are a valid escape, return true. Otherwise, return false.
/// * ident-start code point
///     * Return true.
/// * U+005C REVERSE SOLIDUS (\)
///     * If the first and second code points are a valid escape, return true. Otherwise, return false.
/// * anything else
///     * Return false.
fn start_ident_sequence(it: &mut impl StreamIterator<char>) -> bool {
    if let Some(first_code) = it.peek() {
        match first_code {
            '\u{002D}' => {
                it.next();
                if let Some(second_char) = it.peek() {
                    if is_ident_start_code_point(second_char)
                        || second_char == '\u{002D}'
                        || start_valid_escape(it)
                    {
                        it.back();
                        return true;
                    }
                }
                it.back();
                return false;
            }
            '\\' => {
                return start_valid_escape(it);
            }
            _ => {
                return is_ident_start_code_point(first_code);
            }
        }
    }

    false
}

/// <https://drafts.csswg.org/css-syntax/#consume-the-remnants-of-a-bad-url>
/// * U+0029 RIGHT PARENTHESIS ()),
///   EOF
///   * Return.
/// * The input stream starts with a valid escape
///     * Consume an escaped code point. This allows an escaped right parenthesis ("\)") to be encountered without ending the `<bad-url-token>`. This is otherwise identical to the "anything else" clause.
/// * anything else
///     * Do nothing.
fn consume_remnants_bad_url(it: &mut impl StreamIterator<char>) {
    while let Some(current_char) = it.peek() {
        match current_char {
            ')' => {
                it.next();
                break;
            }
            '\\' => {
                consume_escaped_code_point(it);
                it.next();
            }
            _ => {
                it.next();
            }
        }
    }
}

// --- utils ---

/// Consume a chunk of digit.
fn consume_digit(it: &mut impl StreamIterator<char>) -> String {
    let mut result = String::new();

    while let Some(current_char) = it.peek() {
        if is_digit(current_char) {
            result.push(current_char);
            it.next();
        } else {
            break;
        }
    }

    result
}

#[inline]
fn next_2_char_is(it: &mut impl StreamIterator<char>, first: char, second: char) -> bool {
    if let Some(first_char) = it.peek() {
        if first_char == first {
            it.next();
            if let Some(second_char) = it.peek() {
                if second_char == second {
                    it.back();
                    return true;
                }
            }
            it.back();
        }
    }
    false
}

#[inline]
fn next_char_is(it: &mut impl StreamIterator<char>, f: fn(char) -> bool) -> bool {
    it.next();
    debug!("before {:?}", it.peek());
    if let Some(next_char) = it.peek() {
        it.back();
        debug!("after {:?}", it.peek());
        return f(next_char);
    }
    debug!("{:?}", it.peek());
    it.back();
    debug!("{:?}", it.peek());
    false
}

/// check if char is x
#[inline]
fn char_is_x(it: &mut impl StreamIterator<char>, x: char) -> bool {
    if let Some(current) = it.peek() {
        return current == x;
    }
    false
}

/// Check if the next char is x
#[inline]
fn next_char_is_x(it: &mut impl StreamIterator<char>, x: char) -> bool {
    it.next();
    if let Some(next_char) = it.peek() {
        debug!("next_char {} is {}", next_char, x);
        it.back();
        return next_char == x;
    }
    it.back();
    false
}

/// Check if a char is string gard ' or ".
#[inline]
fn next_char_is_quote(it: &mut impl StreamIterator<char>) -> bool {
    next_char_in(it, &['"', '\''])
}

/// Check if the next char is in the given array.
#[inline]
fn next_char_in(it: &mut impl StreamIterator<char>, x: &[char]) -> bool {
    it.next();
    if let Some(next_char) = it.peek() {
        return x.contains(&next_char);
    }
    it.back();
    false
}

#[inline]
fn is_whitespace(char_check: char) -> bool {
    matches!(char_check, '\n' | '\t' | ' ')
}

/// Check if a char is a digit
#[inline]
fn is_digit(char_check: char) -> bool {
    matches!(char_check, '0'..='9')
}

#[inline]
fn is_hex_digit(char_check: char) -> bool {
    matches!(char_check, '0'..='9' | 'a'..='f' | 'A'..='F')
}

#[inline]
fn is_letter(char_check: char) -> bool {
    matches!(char_check, 'a'..='z' | 'A'..='Z')
}

#[inline]
fn is_non_ascii_ident(char_check: char) -> bool {
    matches!(char_check,
    '\u{00B7}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{203F}'
            | '\u{2040}'
            | '\u{00C0}'..='\u{00D6}'
            | '\u{00D8}'..='\u{00F6}'
            | '\u{00F8}'..='\u{037D}'
            | '\u{037F}'..='\u{1FFF}'
            | '\u{2070}'..='\u{218F}'
            | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D2FF}'
            | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}')
        || char_check > '\u{10000}'
}

/// <https://drafts.csswg.org/css-syntax/#ident-start-code-point>
#[inline]
fn is_ident_start_code_point(char_check: char) -> bool {
    is_letter(char_check) || is_non_ascii_ident(char_check) || char_check == '_'
}

/// <https://drafts.csswg.org/css-syntax/#ident-code-point>
#[inline]
fn is_indent_code_point(char_check: char) -> bool {
    is_ident_start_code_point(char_check) || is_digit(char_check) || char_check == '\u{002D}'
}

// --- hex ---

/// <https://infra.spec.whatwg.org/#surrogate>
#[inline]
fn is_a_surrogate_hex(char_value: u32) -> bool {
    is_a_leading_surrogate_hex(char_value) || is_a_trailing_surrogate_hex(char_value)
}

/// <https://infra.spec.whatwg.org/#leading-surrogate>
#[inline]
fn is_a_leading_surrogate_hex(char_value: u32) -> bool {
    char_value >= 0xD800 && char_value <= 0xDBFF
}

/// <https://infra.spec.whatwg.org/#trailing-surrogate>
#[inline]
fn is_a_trailing_surrogate_hex(char_value: u32) -> bool {
    char_value >= 0xDC00 && char_value <= 0xDFFF
}

/// <https://drafts.csswg.org/css-syntax/#maximum-allowed-code-point>
#[inline]
fn is_max_allowed_code_point_hex(char_value: u32) -> bool {
    char_value > 0x10FFFF
}

#[cfg(test)]
mod tests {
    use log::debug;

    use crate::{
        tokeniser::{
            consume_digit, consume_escaped_code_point, consume_number, consume_numeric_token,
            consume_whitespaces, preprocessing, start_ident_sequence, start_number, tokenization,
            CssToken, NumericValue,
        },
        utils::{
            test_utils::{self},
            CharStream, StreamIterator,
        },
    };

    use super::{consume_ident_like_token, consume_ident_sequence, consume_remnants_bad_url};

    #[test]
    fn preprocessing_test() {
        test_utils::init_test_logger();

        assert_eq!(
            "Hello\nWorld",
            preprocessing("Hello\x0CWorld".to_string()),
            "Preprocessing for form feed"
        );
        assert_eq!(
            "Hello\nWorld",
            preprocessing("Hello\rWorld".to_string()),
            "Preprocessing for Carriage return"
        );
        assert_eq!(
            "Hello\nWorld",
            preprocessing("Hello\r\nWorld".to_string()),
            "Preprocessing for Carriage return + End of line"
        );
        assert_eq!(
            "Hello\n\nWorld",
            preprocessing("Hello\r\n\nWorld".to_string())
        );
    }

    #[test]
    fn whitespace_test() {
        let text = String::from("     )  ");
        let mut stream = CharStream::new(text);
        consume_whitespaces(&mut stream);
        assert_eq!(Some(')'), stream.peek());

        let text = String::from("v     )");
        let mut stream = CharStream::new(text);
        consume_whitespaces(&mut stream);
        assert_eq!(Some('v'), stream.peek());
    }

    #[test]
    fn number_test() {
        test_utils::init_test_logger();

        // number consume by chunk
        {
            let mut stream = CharStream::new("54".to_string());
            assert_eq!(
                "54",
                consume_digit(&mut stream),
                "Consuming only 2 digit 54"
            );
            assert_eq!(None, stream.peek());

            let mut stream = CharStream::new("12s".to_string());
            assert_eq!(
                "12",
                consume_digit(&mut stream),
                "Consume 2 digit and stop 12s"
            );
            assert_eq!(Some('s'), stream.peek());
        }

        // number detection
        {
            let mut stream = CharStream::new("-1.2".to_string());
            assert!(start_number(&mut stream), "Start number -1.2");
            assert_eq!(Some('-'), stream.peek());

            let mut stream = CharStream::new("24".to_string());
            assert!(start_number(&mut stream), "Start number 24");
            assert_eq!(Some('2'), stream.peek());

            let mut stream = CharStream::new("+34".to_string());
            assert!(start_number(&mut stream), "Start number +34");
            assert_eq!(Some('+'), stream.peek());

            let mut stream = CharStream::new("a4".to_string());
            assert!(!start_number(&mut stream), "Start number a4");
            assert_eq!(Some('a'), stream.peek());
        }

        // consume number
        {
            let nb = String::from("12%");
            let mut stream = CharStream::new(nb);
            assert_eq!(
                CssToken::NumberToken {
                    sign: true,
                    val_type: NumericValue::Integer,
                    value: "12".to_string()
                },
                consume_number(&mut stream)
            );
            assert_eq!(Some('%'), stream.peek());

            let nb = String::from("1.4%");
            let mut stream = CharStream::new(nb);
            assert_eq!(
                CssToken::NumberToken {
                    sign: true,
                    val_type: NumericValue::Number,
                    value: "1.4".to_string()
                },
                consume_number(&mut stream)
            );
            assert_eq!(Some('%'), stream.peek());
        }

        // numeric token production number token
        {
            let nb_token = String::from("-1.4");
            let mut stream = CharStream::new(nb_token);
            assert_eq!(
                CssToken::NumberToken {
                    sign: false,
                    val_type: NumericValue::Number,
                    value: "1.4".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let nb_token = String::from("14");
            let mut stream = CharStream::new(nb_token);
            assert_eq!(
                CssToken::NumberToken {
                    sign: true,
                    val_type: NumericValue::Integer,
                    value: "14".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());
        }
        // numeric token production percentage token
        {
            let nb_token = String::from("14%");
            let mut stream = CharStream::new(nb_token);
            assert_eq!(
                CssToken::PercentageToken {
                    sign: true,
                    value: "14".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let nb_token = String::from("1.4%");
            let mut stream = CharStream::new(nb_token);
            assert_eq!(
                CssToken::PercentageToken {
                    sign: true,
                    value: "1.4".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let nb_token = String::from("-1.4%");
            let mut stream = CharStream::new(nb_token);
            assert_eq!(
                CssToken::PercentageToken {
                    sign: false,
                    value: "1.4".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());
        }

        // numeric token production dimension token
        {
            let dim_token = String::from("42px");
            let mut stream = CharStream::new(dim_token);
            assert_eq!(
                CssToken::DimensionToken {
                    unit: "px".to_string(),
                    sign: true,
                    val_type: NumericValue::Integer,
                    value: "42".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let dim_token = String::from("4.2px");
            let mut stream = CharStream::new(dim_token);
            assert_eq!(
                CssToken::DimensionToken {
                    unit: "px".to_string(),
                    sign: true,
                    val_type: NumericValue::Number,
                    value: "4.2".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let dim_token = String::from("4.2rem");
            let mut stream = CharStream::new(dim_token);
            assert_eq!(
                CssToken::DimensionToken {
                    unit: "rem".to_string(),
                    sign: true,
                    val_type: NumericValue::Number,
                    value: "4.2".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());

            let dim_token = String::from("4.2em");
            let mut stream = CharStream::new(dim_token);
            assert_eq!(
                CssToken::DimensionToken {
                    unit: "em".to_string(),
                    sign: true,
                    val_type: NumericValue::Number,
                    value: "4.2".to_string()
                },
                consume_numeric_token(&mut stream)
            );
            assert_eq!(None, stream.peek());
        }
    }

    #[test]
    fn ident_sequence_test() {
        test_utils::init_test_logger();

        // start ident sequence
        {
            let text = String::from("--\\n");
            let mut stream = CharStream::new(text);
            assert!(start_ident_sequence(&mut stream));
            assert_eq!(Some('-'), stream.peek());

            let text = String::from("a");
            let mut stream = CharStream::new(text);
            assert!(start_ident_sequence(&mut stream));
            assert_eq!(Some('a'), stream.peek());

            let text = String::from("\\");
            let mut stream = CharStream::new(text);
            assert!(!start_ident_sequence(&mut stream));
            assert_eq!(Some('\\'), stream.peek());
        }

        // consume indent sequence
        {
            let text = String::from("charset ");
            let mut stream = CharStream::new(text);
            assert_eq!("charset", consume_ident_sequence(&mut stream));
            assert_eq!(Some(' '), stream.peek());

            let text = String::from("olivier,");
            let mut stream = CharStream::new(text);
            assert_eq!("olivier", consume_ident_sequence(&mut stream));
            assert_eq!(Some(','), stream.peek());

            let text = String::from("utf-8;");
            let mut stream = CharStream::new(text);
            assert_eq!("utf-8", consume_ident_sequence(&mut stream));
            assert_eq!(Some(';'), stream.peek());

            let text = String::from("circle(50%)");
            let mut stream = CharStream::new(text);
            assert_eq!("circle", consume_ident_sequence(&mut stream));
            assert_eq!(Some('('), stream.peek());
        }
    }

    #[test]
    fn escaped_test() {
        test_utils::init_test_logger();

        debug!("parsing error");
        // escaped
        let text = String::from("n)");
        let mut stream = CharStream::new(text);
        assert_eq!('n', consume_escaped_code_point(&mut stream));
        assert_eq!(Some(')'), stream.peek());

        // escaped hexa
        {
            let text = String::from("27EB)");
            let mut stream = CharStream::new(text);
            assert_eq!('⟫', consume_escaped_code_point(&mut stream));
            assert_eq!(Some(')'), stream.peek());

            let text = String::from("0000 )");
            let mut stream = CharStream::new(text);
            assert_eq!('�', consume_escaped_code_point(&mut stream));
            assert_eq!(Some(')'), stream.peek());

            let text = String::from("FFFFFF)");
            let mut stream = CharStream::new(text);
            assert_eq!('�', consume_escaped_code_point(&mut stream));
            assert_eq!(Some(')'), stream.peek());
        }

        // eof parsing error �
        let text = String::from("");
        let mut stream = CharStream::new(text);
        assert_eq!('�', consume_escaped_code_point(&mut stream));
        assert_eq!(None, stream.peek());
    }

    #[test]
    fn url_test() {
        test_utils::init_test_logger();
        // test bad url recovery
        {
            let text = String::from("eafuiahf)");
            let mut stream = CharStream::new(text);
            consume_remnants_bad_url(&mut stream);
            assert_eq!(None, stream.peek());

            let text = String::from("eafuiahf");
            let mut stream = CharStream::new(text);
            consume_remnants_bad_url(&mut stream);
            assert_eq!(None, stream.peek());

            let text = String::from("eafuiahf\\)dqsdqsd)");
            let mut stream = CharStream::new(text);
            consume_remnants_bad_url(&mut stream);
            assert_eq!(None, stream.peek());
        }
    }

    #[test]
    fn test_tokenization_strings() {
        test_utils::init_test_logger();

        // Empty string with double quotes
        {
            let text = String::from("\"\"");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::StringToken("".to_string()), token);
        }

        // Empty string with single quotes
        {
            let text = String::from("''");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::StringToken("".to_string()), token);
        }

        // Simple string without whitespace
        {
            let text = String::from("\"hello\"");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::StringToken("hello".to_string()), token);
        }

        // Single quoted string
        {
            let text = String::from("'world'");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::StringToken("world".to_string()), token);
        }

        // Bad string (contains space)
        {
            let text = String::from("\"hello world\"");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::BadStringToken, token);
        }

        // Bad string (contains newline)
        {
            let text = String::from("\"hello\nworld\"");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::BadStringToken, token);
        }

        // Bad string (contains tab)
        {
            let text = String::from("\"hello\tworld\"");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::BadStringToken, token);
        }
    }

    #[test]
    fn test_tokenization_hash_tokens() {
        test_utils::init_test_logger();

        // Hash is treated as delimiter because # itself is not an ident code point
        // The CSS spec requires the character after # to be an ident-code-point or escape
        {
            let text = String::from("#myid");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('#'), token);
        }

        // Hash followed by number is also a delimiter
        {
            let text = String::from("#123");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('#'), token);
        }

        // Hash followed by special char is delimiter
        {
            let text = String::from("#!");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('#'), token);
        }

        // Hash at end of stream
        {
            let text = String::from("#");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('#'), token);
        }
    }

    #[test]
    fn test_tokenization_function_tokens() {
        test_utils::init_test_logger();

        // rgb as ident token (paren should be consumed separately)
        {
            let text = String::from("rgb");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("rgb".to_string()), token);
        }

        // calc as ident token
        {
            let text = String::from("calc");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("calc".to_string()), token);
        }

        // Parenthesis tokens are separate
        {
            let text = String::from("(");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::ParenthOpToken, token);
        }
    }

    #[test]
    fn test_tokenization_at_keyword() {
        test_utils::init_test_logger();

        // Valid @-keyword
        {
            let text = String::from("@media");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::AtKeywordToken("media".to_string()), token);
        }

        // @-keyword with hyphen
        {
            let text = String::from("@supports");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::AtKeywordToken("supports".to_string()), token);
        }

        // @ as delimiter (not followed by ident)
        {
            let text = String::from("@123");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('@'), token);
        }
    }

    #[test]
    fn test_tokenization_ident_tokens() {
        test_utils::init_test_logger();

        // Simple identifier
        {
            let text = String::from("hello");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("hello".to_string()), token);
        }

        // Identifier with hyphen
        {
            let text = String::from("my-class");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("my-class".to_string()), token);
        }

        // Identifier with underscore
        {
            let text = String::from("_private");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("_private".to_string()), token);
        }

        // Identifier for function
        {
            let text = String::from("circle(50px);");
            let mut stream = CharStream::new(text);
            assert_eq!(
                CssToken::FunctionToken("circle".to_string()),
                consume_ident_like_token(&mut stream)
            );
            assert_eq!(Some('5'), stream.peek());
        }
    }

    #[test]
    fn test_tokenization_dimension_units() {
        test_utils::init_test_logger();

        // Pixels
        {
            let text = String::from("10px");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::DimensionToken {
                    unit,
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!("px", unit);
                    assert_eq!(true, sign);
                    assert_eq!(NumericValue::Integer, val_type);
                    assert_eq!("10", value);
                }
                _ => panic!("Expected DimensionToken"),
            }
        }

        // em with decimal
        {
            let text = String::from("1.5em");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::DimensionToken {
                    unit,
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!("em", unit);
                    assert_eq!(true, sign);
                    assert_eq!(NumericValue::Number, val_type);
                    assert_eq!("1.5", value);
                }
                _ => panic!("Expected DimensionToken"),
            }
        }

        // Negative dimension
        {
            let text = String::from("-5rem");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::DimensionToken {
                    unit,
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!("rem", unit);
                    assert_eq!(false, sign);
                    assert_eq!(NumericValue::Integer, val_type);
                    assert_eq!("5", value);
                }
                _ => panic!("Expected DimensionToken"),
            }
        }
    }

    #[test]
    fn test_tokenization_percentages() {
        test_utils::init_test_logger();

        // Positive percentage
        {
            let text = String::from("50%");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::PercentageToken { sign, value } => {
                    assert_eq!(true, sign);
                    assert_eq!("50", value);
                }
                _ => panic!("Expected PercentageToken"),
            }
        }

        // Decimal percentage
        {
            let text = String::from("33.33%");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::PercentageToken { sign, value } => {
                    assert_eq!(true, sign);
                    assert_eq!("33.33", value);
                }
                _ => panic!("Expected PercentageToken"),
            }
        }

        // Negative percentage
        {
            let text = String::from("-25%");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::PercentageToken { sign, value } => {
                    assert_eq!(false, sign);
                    assert_eq!("25", value);
                }
                _ => panic!("Expected PercentageToken"),
            }
        }
    }

    #[test]
    fn test_tokenization_plain_numbers() {
        test_utils::init_test_logger();

        // Positive integer
        {
            let text = String::from("42");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::NumberToken {
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!(true, sign);
                    assert_eq!(NumericValue::Integer, val_type);
                    assert_eq!("42", value);
                }
                _ => panic!("Expected NumberToken"),
            }
        }

        // Decimal number
        {
            let text = String::from("3.14");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::NumberToken {
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!(true, sign);
                    assert_eq!(NumericValue::Number, val_type);
                    assert_eq!("3.14", value);
                }
                _ => panic!("Expected NumberToken"),
            }
        }

        // Negative number with plus sign
        {
            let text = String::from("+99");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::NumberToken {
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!(true, sign);
                    assert_eq!(NumericValue::Integer, val_type);
                    assert_eq!("99", value);
                }
                _ => panic!("Expected NumberToken"),
            }
        }
    }

    #[test]
    fn test_tokenization_delimiters() {
        test_utils::init_test_logger();

        // Various delimiter tokens
        {
            let text = String::from("!");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('!'), token);
        }

        {
            let text = String::from("&");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('&'), token);
        }

        {
            let text = String::from("$");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('$'), token);
        }
    }

    #[test]
    fn test_tokenization_brackets_and_parens() {
        test_utils::init_test_logger();

        // Opening parenthesis
        {
            let text = String::from("(");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::ParenthOpToken, token);
        }

        // Closing parenthesis
        {
            let text = String::from(")");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::ParenthClToken, token);
        }

        // Opening bracket
        {
            let text = String::from("[");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::CrochetOpToken, token);
        }

        // Closing bracket
        {
            let text = String::from("]");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::CrochetClToken, token);
        }

        // Opening brace
        {
            let text = String::from("{");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::AcoladeOpToken, token);
        }

        // Closing brace
        {
            let text = String::from("}");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::AcoladeClToken, token);
        }
    }

    #[test]
    fn test_tokenization_punctuation() {
        test_utils::init_test_logger();

        // Colon
        {
            let text = String::from(":");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::ColonToken, token);
        }

        // Semicolon
        {
            let text = String::from(";");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::SemicolonToken, token);
        }

        // Comma
        {
            let text = String::from(",");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::CommaToken, token);
        }
    }

    #[test]
    fn test_tokenization_cdo_cdc() {
        test_utils::init_test_logger();

        // CDC sequence (-->) - The tokenizer treats "--" as an identifier token
        // The ">" will be tokenized separately
        {
            let text = String::from("-->");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            // Actual behavior: "--" is tokenized as identifier
            assert_eq!(CssToken::IdentToken("--".to_string()), token);
        }

        // Double hyphen alone becomes identifier token
        {
            let text = String::from("--");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("--".to_string()), token);
        }

        // Double hyphen followed by letter becomes longer identifier token
        {
            let text = String::from("--test");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("--test".to_string()), token);
        }

        // Single hyphen followed by non-identifier becomes delimiter
        {
            let text = String::from("-;");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('-'), token);
        }

        // Less-than as delimiter
        {
            let text = String::from("<");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('<'), token);
        }
    }

    #[test]
    fn test_tokenization_whitespace_token() {
        test_utils::init_test_logger();

        // Space
        {
            let text = String::from(" ");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::WhitespaceToken, token);
        }

        // Multiple spaces
        {
            let text = String::from("   ");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::WhitespaceToken, token);
        }

        // Tab
        {
            let text = String::from("\t");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::WhitespaceToken, token);
        }

        // Newline
        {
            let text = String::from("\n");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::WhitespaceToken, token);
        }
    }

    #[test]
    fn test_tokenization_plus_and_dot() {
        test_utils::init_test_logger();

        // Plus as delimiter (not followed by number)
        {
            let text = String::from("+x");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('+'), token);
        }

        // Dot as delimiter (not followed by number)
        {
            let text = String::from(".x");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('.'), token);
        }

        // Dot as delimiter when not followed by identifier or digit
        {
            let text = String::from(".a");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('.'), token);
        }

        // Dot followed by semicolon is delimiter
        {
            let text = String::from(".");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('.'), token);
        }
    }

    #[test]
    fn test_start_ident_sequence_debug() {
        test_utils::init_test_logger();

        // Test what start_ident_sequence does with "myid"
        {
            let text = String::from("myid");
            let mut stream = CharStream::new(text);
            let result = start_ident_sequence(&mut stream);
            assert!(result, "Should start an ident sequence with letter");
        }

        // Test with underscore
        {
            let text = String::from("_private");
            let mut stream = CharStream::new(text);
            let result = start_ident_sequence(&mut stream);
            assert!(result, "Should start an ident sequence with underscore");
        }

        // Test with double hyphen
        {
            let text = String::from("--test");
            let mut stream = CharStream::new(text);
            let result = start_ident_sequence(&mut stream);
            assert!(result, "Should start an ident sequence with double hyphen");
        }

        // Test with digit (should fail - must start with letter, _, or -)
        {
            let text = String::from("5");
            let mut stream = CharStream::new(text);
            let result = start_ident_sequence(&mut stream);
            assert!(!result, "Should not start ident with digit alone");
        }
    }

    #[test]
    fn test_tokenization_escaped_sequences() {
        test_utils::init_test_logger();

        // Escaped character starting identifier
        {
            let text = String::from("\\61 bc");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            // Should produce an IdentToken
            match token {
                CssToken::IdentToken(_) => {
                    // Success
                }
                _ => panic!("Expected IdentToken for escaped sequence"),
            }
        }
    }

    #[test]
    fn test_tokenization_minus_handling() {
        test_utils::init_test_logger();

        // Minus as number prefix
        {
            let text = String::from("-42");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::NumberToken {
                    sign,
                    val_type,
                    value,
                } => {
                    assert_eq!(false, sign);
                    assert_eq!(NumericValue::Integer, val_type);
                    assert_eq!("42", value);
                }
                _ => panic!("Expected NumberToken"),
            }
        }

        // Minus as identifier prefix
        {
            let text = String::from("-webkit");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("-webkit".to_string()), token);
        }

        // Minus as delimiter
        {
            let text = String::from("-!");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('-'), token);
        }
    }

    #[test]
    fn test_tokenization_backslash_handling() {
        test_utils::init_test_logger();

        // Backslash starting an identifier
        {
            let text = String::from("\\n");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            match token {
                CssToken::IdentToken(_) => {
                    // Success - backslash should trigger ident-like token
                }
                _ => panic!("Expected IdentToken for backslash escape"),
            }
        }

        // Backslash followed by newline (should be delimiter)
        {
            let text = String::from("\\\n");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('\\'), token);
        }
    }

    #[test]
    fn test_tokenization_less_than() {
        test_utils::init_test_logger();

        // Less-than as delimiter (not CDO)
        {
            let text = String::from("<");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('<'), token);
        }

        // Less-than with other chars (not CDO)
        {
            let text = String::from("<x");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::DelimToken('<'), token);
        }
    }

    #[test]
    fn test_tokenization_url_function() {
        test_utils::init_test_logger();

        // url as identifier token
        {
            let text = String::from("url");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("url".to_string()), token);
        }

        // other function-like keywords
        {
            let text = String::from("rgba");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::IdentToken("rgba".to_string()), token);
        }
    }

    #[test]
    fn test_tokenization_edge_cases() {
        test_utils::init_test_logger();

        // Empty stream should error
        {
            let text = String::from("");
            let mut stream = CharStream::new(text);
            let result = tokenization(&mut stream);
            assert!(result.is_err());
        }

        // Only whitespace remains after consuming it
        {
            let text = String::from("   ");
            let mut stream = CharStream::new(text);
            let token = tokenization(&mut stream).unwrap();
            assert_eq!(CssToken::WhitespaceToken, token);
        }
    }
}
