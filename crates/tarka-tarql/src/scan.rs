//! A light scan of SPARQL text: the prologue's prefixes and base, and every variable
//! name used. spargebra does not keep the prologue, and `tarql:expandPrefixedName`
//! resolves against it at run time.

use tarka_core::PrefixMap;

#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    Iri(&'a str),
    Var(&'a str),
    Other,
}

/// Tokens of `text`, skipping comments and strings. Good enough for the prologue and
/// for variable names; the real parse is spargebra's.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'"' || c == b'\'' {
            let long = bytes[i..].starts_with(if c == b'"' { b"\"\"\"" } else { b"'''" });
            i += if long { 3 } else { 1 };
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if long && bytes[i..].starts_with(if c == b'"' { b"\"\"\"" } else { b"'''" }) {
                    i += 3;
                    break;
                } else if !long && bytes[i] == c {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            out.push(Token::Other);
        } else if c == b'<' {
            // an IRI if it closes before any character an IRI cannot hold; else `<`
            let end = bytes[i + 1..].iter().position(|&b| b == b'>' || b <= b' ' || b"<\"{}|^`".contains(&b)).map(|p| i + 1 + p);
            match end {
                Some(e) if bytes[e] == b'>' => {
                    out.push(Token::Iri(&text[i + 1..e]));
                    i = e + 1;
                }
                _ => {
                    out.push(Token::Other);
                    i += 1;
                }
            }
        } else if (c == b'?' || c == b'$') && i + 1 < bytes.len() && is_name_char(text[i + 1..].chars().next().unwrap_or(' ')) {
            let start = i + 1;
            let len: usize = text[start..].chars().take_while(|&ch| is_name_char(ch)).map(char::len_utf8).sum();
            out.push(Token::Var(&text[start..start + len]));
            i = start + len;
        } else if is_name_char(text[i..].chars().next().unwrap_or(' ')) || c == b':' {
            let len: usize =
                text[i..].chars().take_while(|&ch| is_name_char(ch) || matches!(ch, ':' | '.' | '-')).map(char::len_utf8).sum();
            // a prefixed name cannot end with '.'
            let word = text[i..i + len].trim_end_matches('.');
            out.push(Token::Word(word));
            i += word.len().max(1);
        } else {
            out.push(Token::Other);
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The prefixes and base IRI declared in the prologue.
pub fn prologue(text: &str) -> (PrefixMap, Option<String>) {
    let mut prefixes = PrefixMap::new();
    let mut base = None;
    let toks = tokens(text);
    let mut i = 0;
    while i < toks.len() {
        match toks[i] {
            Token::Word(w) if w.eq_ignore_ascii_case("PREFIX") => {
                if let (Some(Token::Word(p)), Some(Token::Iri(ns))) = (toks.get(i + 1), toks.get(i + 2))
                    && let Some(prefix) = p.strip_suffix(':')
                {
                    prefixes.insert(prefix, resolve(ns, base.as_deref()));
                }
                i += 3;
            }
            Token::Word(w) if w.eq_ignore_ascii_case("BASE") => {
                if let Some(Token::Iri(iri)) = toks.get(i + 1) {
                    base = Some(resolve(iri, base.as_deref()));
                }
                i += 2;
            }
            _ => break,
        }
    }
    (prefixes, base)
}

fn resolve(iri: &str, base: Option<&str>) -> String {
    base.and_then(|b| oxiri::Iri::parse(b.to_owned()).ok())
        .and_then(|b| b.resolve(iri).ok())
        .map_or_else(|| iri.to_owned(), oxiri::Iri::into_inner)
}

/// Every variable name used in `text`, in order of first use.
pub fn variables(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tokens(text) {
        if let Token::Var(v) = t
            && !out.iter().any(|o| o == v)
        {
            out.push(v.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prologue_and_variables() {
        let q = "# comment PREFIX no: <http://no/>\nBASE <http://base/dir/>\nprefix ex: <http://example.com/>\nPREFIX :<rel#>\n\
                 CONSTRUCT { ?s ex:p \"?notvar #x\" } WHERE { BIND(IRI(?id) AS ?s) FILTER(?n < 3 && $m >2) } # ?gone";
        let (pm, base) = prologue(q);
        assert_eq!(base.as_deref(), Some("http://base/dir/"));
        assert_eq!(pm.get("ex"), Some("http://example.com/"));
        assert_eq!(pm.get(""), Some("http://base/dir/rel#"));
        assert_eq!(pm.get("no"), None);
        assert_eq!(variables(q), ["s", "id", "n", "m"]);
    }
}
