//! The stOTTR tokenizer.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Iri(String),
    /// A prefixed name: (prefix, local).
    PName(String, String),
    Var(String),
    BNode(String),
    Str(String),
    LangTag(String),
    /// A number and its XSD type local name (integer, decimal, double).
    Number(String, &'static str),
    Name(String),
    /// `List<`, `NEList<` or `LUB<`.
    TypeOpen(&'static str),
    /// `@prefix` or `@base`.
    Directive(&'static str),
    DType,
    Punct(&'static str),
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iri(i) => write!(f, "<{i}>"),
            Self::PName(p, l) => write!(f, "{p}:{l}"),
            Self::Var(v) => write!(f, "?{v}"),
            Self::BNode(b) => write!(f, "_:{b}"),
            Self::Str(s) => write!(f, "{s:?}"),
            Self::LangTag(l) => write!(f, "@{l}"),
            Self::Number(n, _) => f.write_str(n),
            Self::Name(n) => f.write_str(n),
            Self::TypeOpen(t) => f.write_str(t),
            Self::Directive(d) => f.write_str(d),
            Self::DType => f.write_str("^^"),
            Self::Punct(p) => f.write_str(p),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LexError {
    pub line: usize,
    pub message: String,
}

pub fn tokenize(text: &str) -> Result<Vec<Token>, LexError> {
    Lexer { chars: text.chars().collect(), pos: 0, line: 1 }.run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Lexer {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn starts(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.peek(i) == Some(c))
    }

    fn err(&self, message: impl Into<String>) -> LexError {
        LexError { line: self.line, message: message.into() }
    }

    fn bump(&mut self, n: usize) {
        for _ in 0..n {
            if self.peek(0) == Some('\n') {
                self.line += 1;
            }
            self.pos += 1;
        }
    }

    fn take_while(&mut self, f: impl Fn(char) -> bool) -> String {
        let start = self.pos;
        while self.peek(0).is_some_and(&f) {
            self.bump(1);
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        let mut out: Vec<Token> = Vec::new();
        while let Some(c) = self.peek(0) {
            let line = self.line;
            let tok = if c.is_whitespace() {
                self.bump(1);
                continue;
            } else if c == '#' {
                self.take_while(|c| c != '\n');
                continue;
            } else if self.starts("/***") {
                self.bump(4);
                while !self.starts("***/") {
                    if self.peek(0).is_none() {
                        return Err(self.err("unterminated /*** comment"));
                    }
                    self.bump(1);
                }
                self.bump(4);
                continue;
            } else if c == '<' && self.iri_ahead() {
                self.bump(1);
                let iri = self.take_while(|c| c != '>');
                self.bump(1);
                Tok::Iri(iri)
            } else if c == '"' || c == '\'' {
                Tok::Str(self.string(c)?)
            } else if c == '@'
                && matches!(out.last(), Some(Token { tok: Tok::Str(_), .. }))
                && self.peek(1).is_some_and(|c| c.is_ascii_alphabetic())
            {
                self.bump(1);
                Tok::LangTag(self.take_while(|c| c.is_ascii_alphanumeric() || c == '-'))
            } else if self.starts("@prefix") && !self.peek(7).is_some_and(is_word) {
                self.bump(7);
                Tok::Directive("@prefix")
            } else if self.starts("@base") && !self.peek(5).is_some_and(is_word) {
                self.bump(5);
                Tok::Directive("@base")
            } else if (c == '?' || c == '$') && self.peek(1).is_some_and(is_word) {
                self.bump(1);
                Tok::Var(self.take_while(is_word))
            } else if self.starts("_:") {
                self.bump(2);
                Tok::BNode(self.local())
            } else if self.starts("^^") {
                self.bump(2);
                Tok::DType
            } else if let Some(t) = ["NEList<", "List<", "LUB<"].into_iter().find(|t| self.starts(t)) {
                self.bump(t.len());
                Tok::TypeOpen(t)
            } else if let Some(n) = self.number() {
                n
            } else if let Some(p) = ["::", "@@", "++"].into_iter().find(|p| self.starts(p)) {
                self.bump(2);
                Tok::Punct(p)
            } else if c == ':' || c.is_alphabetic() || c == '_' {
                let prefix = if c == ':' { String::new() } else { self.take_while(|c| is_word(c) || c == '-' || c == '.') };
                if self.peek(0) == Some(':') && self.peek(1) != Some(':') {
                    // a prefixed name; a prefix cannot end with '.'
                    if prefix.ends_with('.') {
                        return Err(self.err(format!("invalid prefix {prefix:?}")));
                    }
                    self.bump(1);
                    Tok::PName(prefix, self.local())
                } else if c == ':' {
                    return Err(self.err("unexpected ':'"));
                } else {
                    // a name ends at the first '.' (statement end)
                    let name: String = prefix.split('.').next().unwrap_or_default().to_owned();
                    self.pos -= prefix.chars().count() - name.chars().count();
                    Tok::Name(name)
                }
            } else if let Some(p) =
                ["{", "}", "(", ")", "[", "]", ".", ",", ";", "|", "!", "?", "=", "<", ">", "@"].into_iter().find(|p| self.starts(p))
            {
                self.bump(1);
                Tok::Punct(p)
            } else {
                return Err(self.err(format!("unexpected character {c:?}")));
            };
            out.push(Token { tok, line });
        }
        Ok(out)
    }

    fn iri_ahead(&self) -> bool {
        let mut k = 1;
        while let Some(c) = self.peek(k) {
            match c {
                '>' => return true,
                c if c <= ' ' || "<\"{}|^`\\".contains(c) => return false,
                _ => k += 1,
            }
        }
        false
    }

    /// The local part of a prefixed name or blank node label: no trailing '.'.
    fn local(&mut self) -> String {
        let start = self.pos;
        while self.peek(0).is_some_and(|c| is_word(c) || "-.%:".contains(c)) {
            self.bump(1);
        }
        while self.pos > start && self.chars[self.pos - 1] == '.' {
            self.pos -= 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn number(&mut self) -> Option<Tok> {
        let mut k = 0;
        if matches!(self.peek(0), Some('+' | '-')) {
            k += 1;
        }
        let digits = |lexer: &Self, from: usize| (from..).take_while(|&i| lexer.peek(i).is_some_and(|c| c.is_ascii_digit())).count();
        let int = digits(self, k);
        let mut end = k + int;
        let mut kind = "integer";
        if self.peek(end) == Some('.') && digits(self, end + 1) > 0 {
            end += 1 + digits(self, end + 1);
            kind = "decimal";
        } else if int == 0 {
            return None;
        }
        if matches!(self.peek(end), Some('e' | 'E')) {
            let mut e = end + 1;
            if matches!(self.peek(e), Some('+' | '-')) {
                e += 1;
            }
            if digits(self, e) > 0 {
                end = e + digits(self, e);
                kind = "double";
            }
        }
        let text: String = self.chars[self.pos..self.pos + end].iter().collect();
        self.bump(end);
        Some(Tok::Number(text, kind))
    }

    fn string(&mut self, quote: char) -> Result<String, LexError> {
        let long: String = [quote; 3].iter().collect();
        let is_long = self.starts(&long);
        self.bump(if is_long { 3 } else { 1 });
        let mut out = String::new();
        loop {
            let Some(c) = self.peek(0) else { return Err(self.err("unterminated string")) };
            if is_long && self.starts(&long) {
                self.bump(3);
                return Ok(out);
            }
            if !is_long && c == quote {
                self.bump(1);
                return Ok(out);
            }
            if !is_long && (c == '\n' || c == '\r') {
                return Err(self.err("line break in a short string"));
            }
            if c == '\\' {
                let e = self.peek(1).ok_or_else(|| self.err("unterminated escape"))?;
                self.bump(2);
                match e {
                    't' => out.push('\t'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    '"' | '\'' | '\\' => out.push(e),
                    'u' | 'U' => {
                        let n = if e == 'u' { 4 } else { 8 };
                        let hex: String = (0..n).filter_map(|i| self.peek(i)).collect();
                        let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).ok_or_else(|| self.err("bad \\u escape"))?;
                        out.push(ch);
                        self.bump(n);
                    }
                    other => return Err(self.err(format!("unknown escape \\{other}"))),
                }
                continue;
            }
            out.push(c);
            self.bump(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        tokenize(text).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn tokens() {
        assert_eq!(
            toks(
                "@prefix ex: <http://ex/> .\nex:T [ !? ottr:IRI ?x, ? List<xsd:int> ?l = (1, -2.5, 3e2) ] :: { cross | ex:U(++?l, \"a\"@en-GB, 'b'^^xsd:string, _:b1, none) } ."
            ),
            vec![
                Tok::Directive("@prefix"),
                Tok::PName("ex".into(), String::new()),
                Tok::Iri("http://ex/".into()),
                Tok::Punct("."),
                Tok::PName("ex".into(), "T".into()),
                Tok::Punct("["),
                Tok::Punct("!"),
                Tok::Punct("?"),
                Tok::PName("ottr".into(), "IRI".into()),
                Tok::Var("x".into()),
                Tok::Punct(","),
                Tok::Punct("?"),
                Tok::TypeOpen("List<"),
                Tok::PName("xsd".into(), "int".into()),
                Tok::Punct(">"),
                Tok::Var("l".into()),
                Tok::Punct("="),
                Tok::Punct("("),
                Tok::Number("1".into(), "integer"),
                Tok::Punct(","),
                Tok::Number("-2.5".into(), "decimal"),
                Tok::Punct(","),
                Tok::Number("3e2".into(), "double"),
                Tok::Punct(")"),
                Tok::Punct("]"),
                Tok::Punct("::"),
                Tok::Punct("{"),
                Tok::Name("cross".into()),
                Tok::Punct("|"),
                Tok::PName("ex".into(), "U".into()),
                Tok::Punct("("),
                Tok::Punct("++"),
                Tok::Var("l".into()),
                Tok::Punct(","),
                Tok::Str("a".into()),
                Tok::LangTag("en-GB".into()),
                Tok::Punct(","),
                Tok::Str("b".into()),
                Tok::DType,
                Tok::PName("xsd".into(), "string".into()),
                Tok::Punct(","),
                Tok::BNode("b1".into()),
                Tok::Punct(","),
                Tok::Name("none".into()),
                Tok::Punct(")"),
                Tok::Punct("}"),
                Tok::Punct("."),
            ]
        );
    }

    #[test]
    fn comments_strings_and_lines() {
        let t = tokenize("# c\n/*** block\n comment ***/ \"\"\"long\n\"q\"\"\" \"\" ex:a.\n'\\u00e5'").unwrap();
        let toks: Vec<&Tok> = t.iter().map(|t| &t.tok).collect();
        assert_eq!(
            toks,
            [
                &Tok::Str("long\n\"q".into()),
                &Tok::Str(String::new()),
                &Tok::PName("ex".into(), "a".into()),
                &Tok::Punct("."),
                &Tok::Str("\u{e5}".into())
            ]
        );
        assert_eq!(t[0].line, 3);
        assert_eq!(t[4].line, 5);
        assert!(tokenize("/* plain */").is_err(), "like Lutra, only /*** ***/ comments");
    }
}
