//! Characters before grammar, with tokens bounded by the text's length.

use super::{SchemaError, Span, Wrong, add};
use skein_lib::List;

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) at: Span,
    pub(super) line: u32,
}

pub(super) fn scan(text: &[u8], mut tokens: Option<&mut List<Token>>) -> Result<(u32, u32), SchemaError> {
    let mut count = 0;
    let mut line = 1;
    let mut comment = false;
    let mut word = None;
    for (offset, &byte) in text.iter().enumerate() {
        let at = u32::try_from(offset).expect("text passed its ceiling");
        if comment {
            if byte == b'\n' {
                comment = false;
                line = add(line, 1);
            }
            continue;
        }
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
            if word.is_none() {
                word = Some(at);
            }
            continue;
        }
        if let Some(start) = word.take() {
            emit(&mut tokens, Token { at: Span { start, end: at }, line });
            count = add(count, 1);
        }
        if byte == b'#' {
            comment = true;
        } else if byte.is_ascii_whitespace() {
            if byte == b'\n' {
                line = add(line, 1);
            }
        } else if byte == b'{' || byte == b'}' || byte == b':' || byte == b',' {
            emit(&mut tokens, Token { at: Span { start: at, end: add(at, 1) }, line });
            count = add(count, 1);
        } else {
            return Err(SchemaError { line, at: Span { start: at, end: add(at, 1) }, wrong: Wrong::Character });
        }
    }
    if let Some(start) = word {
        let end = u32::try_from(text.len()).expect("text passed its ceiling");
        emit(&mut tokens, Token { at: Span { start, end }, line });
        count = add(count, 1);
    }
    if text.last() == Some(&b'\n') {
        line = line.checked_sub(1).expect("a newline raised the line").max(1);
    }
    Ok((count, line))
}

fn emit(tokens: &mut Option<&mut List<Token>>, token: Token) {
    if let Some(tokens) = tokens {
        tokens.push(token).expect("token count was measured before allocating");
    }
}
