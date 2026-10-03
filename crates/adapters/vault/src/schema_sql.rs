//! Compare the closed CREATE statements used by this store. This is lexical
//! comparison, not a SQL parser or permission to accept a different schema.

const MAX_SQL_BYTES: usize = 65_536;
const MAX_TOKENS: usize = 4096;
const MAX_DEPTH: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Comparison {
    Equivalent,
    Different,
    InvalidStored,
    InvalidExpected,
}

#[derive(Debug, Eq, PartialEq)]
enum Token {
    Word(String),
    Number(String),
    Quoted(String),
    Blob(String),
    Symbol(String),
}

pub(crate) fn compare(stored: &str, expected: &str) -> Comparison {
    let Some(expected) = tokenize(expected) else {
        return Comparison::InvalidExpected;
    };
    let Some(stored) = tokenize(stored) else {
        return Comparison::InvalidStored;
    };
    if stored == expected {
        Comparison::Equivalent
    } else {
        Comparison::Different
    }
}

fn tokenize(sql: &str) -> Option<Vec<Token>> {
    if sql.is_empty() || sql.len() > MAX_SQL_BYTES {
        return None;
    }
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if tokens.len() == MAX_TOKENS
            || bytes[index..].starts_with(b"--")
            || bytes[index..].starts_with(b"/*")
        {
            return None;
        }
        let start = index;
        let token = match bytes[index] {
            b'x' | b'X' if bytes.get(index + 1) == Some(&b'\'') => {
                index = quoted_end(bytes, index + 1, b'\'')?;
                let content = &bytes[start + 2..index - 1];
                if content.len() % 2 != 0 || !content.iter().all(u8::is_ascii_hexdigit) {
                    return None;
                }
                Token::Blob(sql[start..index].to_owned())
            }
            quote @ (b'\'' | b'"' | b'`') => {
                index = quoted_end(bytes, index, quote)?;
                Token::Quoted(sql[start..index].to_owned())
            }
            b'[' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b']' {
                    index += 1;
                }
                if index == bytes.len() {
                    return None;
                }
                index += 1;
                Token::Quoted(sql[start..index].to_owned())
            }
            value
                if value.is_ascii_digit()
                    || (value == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                index = number_end(bytes, index)?;
                Token::Number(sql[start..index].to_owned())
            }
            value if word_start(value) => {
                index += 1;
                while index < bytes.len() && word_continue(bytes[index]) {
                    index += 1;
                }
                // SQLite's unquoted identifiers/keywords are ASCII-insensitive.
                // Quoted values and non-ASCII bytes are never case-normalized.
                Token::Word(sql[start..index].to_ascii_lowercase())
            }
            b'(' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return None;
                }
                index += 1;
                Token::Symbol("(".into())
            }
            b')' => {
                depth = depth.checked_sub(1)?;
                index += 1;
                Token::Symbol(")".into())
            }
            b',' | b'.' | b';' | b'=' | b'<' | b'>' | b'!' | b'|' | b'&' | b'+' | b'-' | b'*'
            | b'/' | b'%' | b'~' => {
                let rest = &bytes[index..];
                let width = if rest.starts_with(b"->>") {
                    3
                } else if [
                    b"==", b"<=", b">=", b"!=", b"<>", b"||", b"<<", b">>", b"->",
                ]
                .iter()
                .any(|operator| rest.starts_with(*operator))
                {
                    2
                } else if bytes[index] == b'!' {
                    return None;
                } else {
                    1
                };
                index += width;
                Token::Symbol(sql[start..index].to_owned())
            }
            _ => return None,
        };
        tokens.push(token);
    }
    if depth != 0 {
        return None;
    }
    if symbol(tokens.last(), ";") {
        tokens.pop();
    }
    if tokens.iter().any(|token| symbol(Some(token), ";")) {
        return None;
    }
    normalize_create_prefix(&mut tokens)?;
    Some(tokens)
}

fn normalize_create_prefix(tokens: &mut Vec<Token>) -> Option<()> {
    if !word(tokens.first(), "create") {
        return None;
    }
    let unique = word(tokens.get(1), "unique");
    let kind_index = if unique { 2 } else { 1 };
    let table = word(tokens.get(kind_index), "table");
    if (!table && !word(tokens.get(kind_index), "index")) || (table && unique) {
        return None;
    }
    let name_index = kind_index + 1;
    if word(tokens.get(name_index), "if") {
        if !word(tokens.get(name_index + 1), "not") || !word(tokens.get(name_index + 2), "exists") {
            return None;
        }
        drop(tokens.drain(name_index..name_index + 3));
    }
    if !identifier(tokens.get(name_index)) {
        return None;
    }
    let body_index = if table {
        name_index + 1
    } else {
        if !word(tokens.get(name_index + 1), "on") || !identifier(tokens.get(name_index + 2)) {
            return None;
        }
        name_index + 3
    };
    if !symbol(tokens.get(body_index), "(") {
        return None;
    }
    Some(())
}

fn quoted_end(bytes: &[u8], mut index: usize, quote: u8) -> Option<usize> {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
            } else {
                return Some(index + 1);
            }
        } else {
            index += 1;
        }
    }
    None
}

fn number_end(bytes: &[u8], mut index: usize) -> Option<usize> {
    if bytes[index] == b'0' && matches!(bytes.get(index + 1), Some(b'x' | b'X')) {
        index += 2;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_hexdigit) {
            index += 1;
        }
        if index == start {
            return None;
        }
    } else {
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        if matches!(bytes.get(index), Some(b'e' | b'E')) {
            index += 1;
            if matches!(bytes.get(index), Some(b'+' | b'-')) {
                index += 1;
            }
            let start = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == start {
                return None;
            }
        }
    }
    if bytes.get(index).is_some_and(|value| word_continue(*value)) {
        return None;
    }
    Some(index)
}

fn word_start(value: u8) -> bool {
    value.is_ascii_alphabetic() || value == b'_' || value >= 0x80
}
fn word_continue(value: u8) -> bool {
    word_start(value) || value.is_ascii_digit() || value == b'$'
}
fn word(token: Option<&Token>, expected: &str) -> bool {
    matches!(token, Some(Token::Word(value)) if value == expected)
}
fn symbol(token: Option<&Token>, expected: &str) -> bool {
    matches!(token, Some(Token::Symbol(value)) if value == expected)
}
fn identifier(token: Option<&Token>) -> bool {
    match token {
        Some(Token::Word(_)) => true,
        Some(Token::Quoted(value)) => !value.starts_with('\''),
        _ => false,
    }
}
