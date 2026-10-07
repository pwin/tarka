//! H2 sources: the part of H2's SQL that reads a CSV file,
//! `SELECT [DISTINCT] columns FROM CSVREAD('file.csv' [, columns [, options]])`, with
//! H2's CSV dialect.
//!
//! The dialect, as H2 reads it: the first line names the columns (unless `CSVREAD` is
//! given them); fields are separated by `,` and quoted with `"`, a quote inside a
//! quoted field is doubled; an unquoted field is trimmed, and an empty one is NULL
//! (`""` is an empty string). Unquoted column names match case-insensitively.

use std::path::{Path, PathBuf};

/// One item of the select list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// `*`: every column, in file order.
    All,
    Column {
        name: String,
        quoted: bool,
    },
    /// A string constant (`'text'`).
    Const(String),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialect {
    pub field_separator: char,
    /// The quote; `None` when fields are never quoted.
    pub field_delimiter: Option<char>,
    pub escape: Option<char>,
    pub preserve_whitespace: bool,
    /// An unquoted value that means NULL.
    pub null: Option<String>,
    pub case_sensitive: bool,
    pub line_comment: Option<char>,
}

impl Default for Dialect {
    fn default() -> Self {
        Self {
            field_separator: ',',
            field_delimiter: Some('"'),
            escape: Some('"'),
            preserve_whitespace: false,
            null: None,
            case_sensitive: false,
            line_comment: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvQuery {
    pub distinct: bool,
    pub items: Vec<Item>,
    pub file: PathBuf,
    /// The column names `CSVREAD` is given (the file then has no header line).
    pub columns: Option<Vec<String>>,
    pub dialect: Dialect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Word(String),
    /// `"name"`
    Quoted(String),
    /// `'text'`
    Str(String),
    Punct(char),
}

fn tokenize(sql: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut chars = sql.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '-' && chars.clone().nth(1) == Some('-') {
            // a comment to the end of the line
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if c.is_alphanumeric() || c == '_' {
            let mut w = String::new();
            while let Some(&c) = chars.peek().filter(|c| c.is_alphanumeric() || **c == '_' || **c == '$') {
                w.push(c);
                chars.next();
            }
            out.push(Tok::Word(w));
        } else if c == '\'' || c == '"' {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next() {
                    None => return Err(format!("an unterminated {} in the query", if c == '\'' { "string" } else { "name" })),
                    Some(q) if q == c => {
                        // a doubled quote is the quote itself
                        if chars.peek() == Some(&c) {
                            chars.next();
                            s.push(c);
                        } else {
                            break;
                        }
                    }
                    Some(other) => s.push(other),
                }
            }
            out.push(if c == '\'' { Tok::Str(s) } else { Tok::Quoted(s) });
        } else {
            out.push(Tok::Punct(c));
            chars.next();
        }
    }
    Ok(out)
}

const SHAPE: &str = "tarka reads H2 queries of the form SELECT [DISTINCT] columns FROM CSVREAD('file.csv'[, columns[, options]])";

fn is(t: Option<&Tok>, keyword: &str) -> bool {
    matches!(t, Some(Tok::Word(w)) if w.eq_ignore_ascii_case(keyword))
}

/// Parses an H2 query over `CSVREAD`; a relative file is found from `dir`.
pub fn parse_query(sql: &str, dir: &Path) -> Result<CsvQuery, String> {
    let toks = tokenize(sql)?;
    let mut pos = 0;
    let next = |pos: &mut usize| {
        *pos += 1;
        toks.get(*pos - 1)
    };
    if !is(next(&mut pos), "SELECT") {
        return Err(format!("{SHAPE}: this one does not start with SELECT"));
    }
    let distinct = is(toks.get(pos), "DISTINCT");
    if distinct {
        pos += 1;
    }
    let mut items = Vec::new();
    loop {
        let item = match next(&mut pos) {
            Some(Tok::Punct('*')) => Item::All,
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("NULL") => Item::Null,
            Some(Tok::Word(w)) if !w.eq_ignore_ascii_case("FROM") => Item::Column { name: w.clone(), quoted: false },
            Some(Tok::Quoted(q)) => Item::Column { name: q.clone(), quoted: true },
            Some(Tok::Str(s)) => Item::Const(s.clone()),
            Some(Tok::Punct('(')) => return Err(format!("{SHAPE}: expressions in the select list are not supported")),
            other => return Err(format!("{SHAPE}: expected a column, not {}", describe(other))),
        };
        if matches!(toks.get(pos), Some(Tok::Punct('('))) {
            return Err(format!("{SHAPE}: functions in the select list are not supported"));
        }
        items.push(item);
        // an alias, with or without AS
        if is(toks.get(pos), "AS") {
            pos += 1;
            if !matches!(next(&mut pos), Some(Tok::Word(_) | Tok::Quoted(_))) {
                return Err(format!("{SHAPE}: AS needs a name"));
            }
        } else if matches!(toks.get(pos), Some(Tok::Word(w)) if !w.eq_ignore_ascii_case("FROM"))
            || matches!(toks.get(pos), Some(Tok::Quoted(_)))
        {
            pos += 1;
        }
        match next(&mut pos) {
            Some(Tok::Punct(',')) => continue,
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("FROM") => break,
            other => return Err(format!("{SHAPE}: expected , or FROM, not {}", describe(other))),
        }
    }
    if !is(next(&mut pos), "CSVREAD") {
        return Err(format!("{SHAPE}: the FROM clause must be CSVREAD(…)"));
    }
    if next(&mut pos) != Some(&Tok::Punct('(')) {
        return Err(format!("{SHAPE}: CSVREAD needs its arguments"));
    }
    let mut args: Vec<Option<String>> = Vec::new();
    loop {
        match next(&mut pos) {
            Some(Tok::Str(s)) => args.push(Some(s.clone())),
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("NULL") => args.push(None),
            other => return Err(format!("{SHAPE}: CSVREAD takes strings, not {}", describe(other))),
        }
        match next(&mut pos) {
            Some(Tok::Punct(',')) => continue,
            Some(Tok::Punct(')')) => break,
            other => return Err(format!("{SHAPE}: expected , or ) in CSVREAD, not {}", describe(other))),
        }
    }
    if matches!(toks.get(pos), Some(Tok::Punct(';'))) {
        pos += 1;
    }
    if let Some(rest) = toks.get(pos) {
        return Err(format!("{SHAPE}: this one goes on ({} …), which is not supported", describe(Some(rest))));
    }
    let (file, columns, options) = match args.as_slice() {
        [Some(f)] => (f, None, None),
        [Some(f), c] => (f, c.clone(), None),
        [Some(f), c, o] => (f, c.clone(), o.clone()),
        _ => return Err("CSVREAD takes a file name, and optionally the column names and options".into()),
    };
    let dialect = options.as_deref().map(parse_options).transpose()?.unwrap_or_default();
    let columns = columns.map(|c| c.split(dialect.field_separator).map(|n| n.trim().to_owned()).collect());
    let path = PathBuf::from(file);
    let file = if path.is_absolute() || file.starts_with('/') { path } else { dir.join(path) };
    Ok(CsvQuery { distinct, items, file, columns, dialect })
}

fn describe(t: Option<&Tok>) -> String {
    match t {
        None => "the end".into(),
        Some(Tok::Word(w)) => w.clone(),
        Some(Tok::Quoted(q)) => format!("\"{q}\""),
        Some(Tok::Str(s)) => format!("'{s}'"),
        Some(Tok::Punct(c)) => c.to_string(),
    }
}

/// `CSVREAD`'s options: `key=value` pairs separated by spaces.
fn parse_options(options: &str) -> Result<Dialect, String> {
    let mut d = Dialect::default();
    let one = |key: &str, v: &str| -> Result<Option<char>, String> {
        let mut c = v.chars();
        match (c.next(), c.next()) {
            (None, _) => Ok(None),
            (Some(ch), None) => Ok(Some(ch)),
            _ => Err(format!("CSVREAD option {key} must be one character, not {v:?}")),
        }
    };
    let flag = |key: &str, v: &str| match v.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("CSVREAD option {key} is true or false, not {v:?}")),
    };
    for pair in options.split_whitespace() {
        let (key, value) = pair.split_once('=').ok_or_else(|| format!("CSVREAD option {pair:?} is not key=value"))?;
        match key.to_ascii_lowercase().as_str() {
            "charset" if value.eq_ignore_ascii_case("UTF-8") || value.eq_ignore_ascii_case("UTF8") => {}
            "charset" => return Err(format!("tarka reads CSV files as UTF-8, not {value}")),
            "fieldseparator" => d.field_separator = one(key, value)?.ok_or("CSVREAD's fieldSeparator cannot be empty")?,
            "fielddelimiter" => d.field_delimiter = one(key, value)?,
            "escape" => d.escape = one(key, value)?,
            "null" => d.null = Some(value.to_owned()),
            "preservewhitespace" => d.preserve_whitespace = flag(key, value)?,
            "casesensitivecolumnnames" => d.case_sensitive = flag(key, value)?,
            "linecomment" => d.line_comment = one(key, value)?,
            _ => return Err(format!("CSVREAD option {key} is not supported")),
        }
    }
    Ok(d)
}

/// The rows of a CSV file in H2's dialect, as fields (None for NULL).
pub fn read_csv(text: &str, d: &Dialect) -> Vec<Vec<Option<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut chars = text.chars().peekable();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut at_line_start = true;
    loop {
        if at_line_start {
            // a comment line, or an empty one, is skipped
            match chars.peek() {
                None => break,
                Some(&c) if Some(c) == d.line_comment => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                    continue;
                }
                Some('\n') => {
                    chars.next();
                    continue;
                }
                Some('\r') => {
                    chars.next();
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    continue;
                }
                _ => {}
            }
            at_line_start = false;
        }
        // one field
        let mut raw = String::new();
        let mut quoted = false;
        // whitespace before a quote is not part of the field
        while let Some(&c) = chars.peek() {
            if c == ' ' || c == '\t' {
                raw.push(c);
                chars.next();
            } else {
                break;
            }
        }
        if let Some(q) = d.field_delimiter.filter(|q| chars.peek() == Some(q)) {
            quoted = true;
            raw.clear();
            chars.next();
            loop {
                match chars.next() {
                    None => break,
                    Some(c) if Some(c) == d.escape && c != q && chars.peek() == Some(&q) => {
                        raw.push(q);
                        chars.next();
                    }
                    Some(c) if c == q => {
                        if d.escape == Some(q) && chars.peek() == Some(&q) {
                            raw.push(q);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    Some(c) => raw.push(c),
                }
            }
            // anything after the closing quote, up to the separator, is dropped
            while let Some(&c) = chars.peek() {
                if c == d.field_separator || c == '\n' || c == '\r' {
                    break;
                }
                chars.next();
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == d.field_separator || c == '\n' || c == '\r' {
                    break;
                }
                raw.push(c);
                chars.next();
            }
        }
        let value = if quoted {
            Some(raw)
        } else {
            let v = if d.preserve_whitespace { raw } else { raw.trim().to_owned() };
            if v.is_empty() || d.null.as_deref() == Some(v.as_str()) { None } else { Some(v) }
        };
        row.push(value);
        match chars.next() {
            Some(c) if c == d.field_separator => {}
            Some('\r') => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                rows.push(std::mem::take(&mut row));
                at_line_start = true;
            }
            Some('\n') => {
                rows.push(std::mem::take(&mut row));
                at_line_start = true;
            }
            None => {
                rows.push(std::mem::take(&mut row));
                break;
            }
            Some(_) => unreachable!("a field ends at a separator or a line end"),
        }
    }
    rows
}

/// The rows a query gives: the column labels, and one value per item and row.
pub fn rows(q: &CsvQuery) -> Result<crate::Table<String>, String> {
    let text = std::fs::read_to_string(&q.file).map_err(|e| format!("{}: {e}", q.file.display()))?;
    let mut records = read_csv(&text, &q.dialect);
    let header: Vec<String> = match &q.columns {
        Some(c) => c.clone(),
        None if records.is_empty() => Vec::new(),
        None => records.remove(0).into_iter().map(Option::unwrap_or_default).collect(),
    };
    let find = |name: &str, quoted: bool| -> Result<usize, String> {
        let exact = quoted && q.dialect.case_sensitive;
        header
            .iter()
            .position(|h| if exact { h == name } else { h.eq_ignore_ascii_case(name) })
            .ok_or_else(|| format!("column {name} is not in {} (its columns: {})", q.file.display(), header.join(", ")))
    };
    // what each output column reads: a field of the file, or a constant
    enum Get {
        Field(usize),
        Const(Option<String>),
    }
    let mut gets = Vec::new();
    let mut labels = Vec::new();
    for item in &q.items {
        match item {
            Item::All => {
                for (i, h) in header.iter().enumerate() {
                    gets.push(Get::Field(i));
                    labels.push(h.clone());
                }
            }
            Item::Column { name, quoted } => {
                gets.push(Get::Field(find(name, *quoted)?));
                labels.push(name.clone());
            }
            Item::Const(s) => {
                gets.push(Get::Const(Some(s.clone())));
                labels.push(format!("'{s}'"));
            }
            Item::Null => {
                gets.push(Get::Const(None));
                labels.push("NULL".into());
            }
        }
    }
    let mut out: Vec<Vec<Option<String>>> = records
        .into_iter()
        .map(|r| {
            gets.iter()
                .map(|g| match g {
                    Get::Field(i) => r.get(*i).cloned().flatten(),
                    Get::Const(c) => c.clone(),
                })
                .collect()
        })
        .collect();
    if q.distinct {
        let mut seen = std::collections::HashSet::new();
        out.retain(|r| seen.insert(r.clone()));
    }
    Ok((labels, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries() {
        let q = parse_query(
            "SELECT DISTINCT id, \"Name\" AS n, 'const' c, NULL FROM CSVREAD('@dir/x.csv', 'a;b', 'fieldSeparator=; null=NA');",
            Path::new("/d"),
        )
        .unwrap();
        assert!(q.distinct);
        assert_eq!(
            q.items,
            [
                Item::Column { name: "id".into(), quoted: false },
                Item::Column { name: "Name".into(), quoted: true },
                Item::Const("const".into()),
                Item::Null
            ]
        );
        assert_eq!(q.columns, Some(vec!["a".into(), "b".into()]), "split on the field separator, as H2 does");
        assert_eq!(q.dialect.field_separator, ';');
        assert_eq!(q.dialect.null.as_deref(), Some("NA"));
        assert_eq!(parse_query("select * from csvread('x.csv')", Path::new("/d")).unwrap().items, [Item::All]);
        for bad in [
            "SELECT a FROM t",
            "SELECT a FROM CSVREAD('x.csv') WHERE a = 1",
            "SELECT UPPER(a) FROM CSVREAD('x.csv')",
            "SELECT a, FROM CSVREAD('x.csv')",
            "DELETE FROM x",
        ] {
            assert!(parse_query(bad, Path::new("/d")).is_err(), "{bad}");
        }
    }

    /// What H2 made of the same text, through Lutra.
    #[test]
    fn the_dialect() {
        let rows =
            read_csv("Id,Plain\r\nex:a,  spaced  \nex:b,\"\"\nex:c,\nex:d,\"q \"\"x\"\" q\"\n\nex:e, \"a,b\" \n", &Dialect::default());
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(
            rows,
            [
                vec![s("Id"), s("Plain")],
                vec![s("ex:a"), s("spaced")],
                vec![s("ex:b"), s("")],
                vec![s("ex:c"), None],
                vec![s("ex:d"), s("q \"x\" q")],
                vec![s("ex:e"), s("a,b")],
            ]
        );
        let d = Dialect { field_separator: ';', null: Some("NA".into()), ..Dialect::default() };
        assert_eq!(read_csv("a;NA;\"NA\"", &d), [vec![s("a"), None, s("NA")]]);
    }
}
