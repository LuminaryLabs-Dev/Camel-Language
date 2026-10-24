use super::{
    syntax::{Error, Result, Site},
    MAX_SOURCE_BYTES,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Word(String),
    Number(String),
    Text(String, bool),
    Symbol(String),
    End,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub site: Site,
}

fn through(site: &Site, line: usize, column: usize, offset: usize) -> Site {
    let mut ranged = site.clone();
    ranged.end_line = line;
    ranged.end_column = column;
    ranged.end_offset = offset;
    ranged
}

pub fn lex(file: &str, source: &str) -> Result<Vec<Token>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::at(
            &Site {
                file: file.into(),
                line: 1,
                column: 1,
                offset: 0,
                end_line: 1,
                end_column: 1,
                end_offset: 0,
            },
            "source file exceeds 8 MiB",
        ));
    }
    let chars: Vec<char> = source.chars().collect();
    let (mut i, mut line, mut column, mut byte_offset) = (0, 1, 1, 0);
    let mut tokens = Vec::new();
    macro_rules! advance {
        () => {{
            let c = chars[i];
            i += 1;
            byte_offset += c.len_utf8();
            if c == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
            c
        }};
    }
    while i < chars.len() {
        let mut site = Site {
            file: file.into(),
            line,
            column,
            offset: byte_offset,
            end_line: line,
            end_column: column,
            end_offset: byte_offset,
        };
        let c = chars[i];
        if c.is_whitespace() {
            advance!();
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                advance!();
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            advance!();
            advance!();
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                advance!();
            }
            if i == chars.len() {
                return Err(Error::at(
                    &through(&site, line, column, byte_offset),
                    "unterminated comment",
                ));
            }
            advance!();
            advance!();
            continue;
        }
        let kind = if c == '"' || (c == '$' && chars.get(i + 1) == Some(&'"')) {
            let interpolation = c == '$';
            if interpolation {
                advance!();
            }
            advance!();
            let mut value = String::new();
            while i < chars.len() && chars[i] != '"' {
                let ch = advance!();
                if ch == '\\' {
                    if i == chars.len() {
                        return Err(Error::at(
                            &through(&site, line, column, byte_offset),
                            "unterminated escape",
                        ));
                    }
                    value.push(match advance!() {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => {
                            return Err(Error::at(
                                &through(&site, line, column, byte_offset),
                                format!("invalid escape \\{other}"),
                            ))
                        }
                    });
                } else {
                    value.push(ch);
                }
            }
            if i == chars.len() {
                return Err(Error::at(
                    &through(&site, line, column, byte_offset),
                    "unterminated string",
                ));
            }
            advance!();
            Kind::Text(value, interpolation)
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                advance!();
            }
            if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                advance!();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    advance!();
                }
            }
            Kind::Number(chars[start..i].iter().collect())
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            advance!();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                advance!();
            }
            Kind::Word(chars[start..i].iter().collect())
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if ["==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "++", "--"].contains(&two.as_str())
            {
                advance!();
                advance!();
                Kind::Symbol(two)
            } else if "{}()[];,.=+-*/%<>!:".contains(c) {
                advance!();
                Kind::Symbol(c.to_string())
            } else {
                return Err(Error::at(
                    &through(&site, line, column + 1, byte_offset + c.len_utf8()),
                    format!("unexpected character '{c}'"),
                ));
            }
        };
        site.end_line = line;
        site.end_column = column;
        site.end_offset = byte_offset;
        if tokens.len() >= 100_000 {
            return Err(Error::at(&site, "source token budget exhausted"));
        }
        tokens.push(Token { kind, site });
    }
    tokens.push(Token {
        kind: Kind::End,
        site: Site {
            file: file.into(),
            line,
            column,
            offset: byte_offset,
            end_line: line,
            end_column: column,
            end_offset: byte_offset,
        },
    });
    Ok(tokens)
}
