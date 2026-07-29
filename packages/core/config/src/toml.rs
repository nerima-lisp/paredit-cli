//! A strict TOML subset that keeps the line every key came from.
//!
//! Not a general TOML implementation, and deliberately so. The configuration
//! schema this parses is closed (see [`crate::schema`]): tables, dotted keys,
//! strings, integers, booleans, and arrays of those. Floats, datetimes, inline
//! tables, arrays of tables, and multi-line strings are all *rejected with a
//! line number* rather than silently accepted, because a key this tool does
//! not understand is far more likely to be a typo than a feature request.
//!
//! The one property a `serde`-shaped parser could not give: every entry keeps
//! the 1-based line it was written on. `paredit config show` exists to answer
//! "which file and which line decided this", and that answer has to survive
//! parsing to be printable.

use std::collections::BTreeSet;
use std::fmt;

use serde_json::{Value as Json, json};

/// A parsed configuration file, flattened to dotted keys in file order.
///
/// Flattened rather than nested because every consumer here looks keys up by
/// their full dotted name — the schema is a flat table of those — and a nested
/// tree would only be walked back down to the same strings.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub entries: Vec<Entry>,
}

/// One `key = value`, with the line it was written on.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The full dotted key, table prefix included: `lint.fail-on`.
    pub key: String,
    pub value: Value,
    /// 1-based, counted in the file this entry was parsed from.
    pub line: usize,
}

/// Every value shape this subset admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    String(String),
    Integer(i64),
    Boolean(bool),
    Array(Vec<Value>),
}

impl Value {
    /// The name this shape goes by in a diagnostic.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Boolean(_) => "boolean",
            Self::Array(_) => "array",
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        match self {
            Self::String(text) => json!(text),
            Self::Integer(number) => json!(number),
            Self::Boolean(flag) => json!(flag),
            Self::Array(items) => Json::Array(items.iter().map(Self::to_json).collect()),
        }
    }
}

/// Rendered the way the value would be written back into a `paredit.toml`.
///
/// Used by `config show`, so a shown value can be copied into a file verbatim.
impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(text) => write!(formatter, "\"{}\"", escape_basic_string(text)),
            Self::Integer(number) => write!(formatter, "{number}"),
            Self::Boolean(flag) => write!(formatter, "{flag}"),
            Self::Array(items) => {
                formatter.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{item}")?;
                }
                formatter.write_str("]")
            }
        }
    }
}

fn escape_basic_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04X}", control as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

/// A syntax problem, located.
///
/// Carries a line rather than only a message because a configuration file is
/// edited by hand, and "line 12" is the difference between a fix and a hunt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for TomlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TomlError {}

fn error(line: usize, message: impl Into<String>) -> TomlError {
    TomlError {
        line,
        message: message.into(),
    }
}

/// Parses one configuration file.
///
/// # Errors
///
/// Returns the first syntax problem found, with the line it sits on. Parsing
/// stops there: a file with a broken line is not partially applied, because a
/// half-read configuration is the worst of both outcomes.
pub fn parse(text: &str) -> Result<Document, TomlError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut entries = Vec::new();
    let mut seen_keys = BTreeSet::new();
    let mut seen_tables = BTreeSet::new();
    let mut table_prefix = String::new();
    let mut index = 0;

    while index < lines.len() {
        let line_number = index + 1;
        let raw = lines[index];
        let trimmed = raw.trim();
        index += 1;

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("[[") {
            return Err(error(
                line_number,
                "arrays of tables ([[name]]) are not part of the paredit configuration schema",
            ));
        }

        if trimmed.starts_with('[') {
            let name = parse_table_header(trimmed, line_number)?;
            if !seen_tables.insert(name.clone()) {
                return Err(error(
                    line_number,
                    format!("table [{name}] is declared twice"),
                ));
            }
            table_prefix = name;
            continue;
        }

        // A value that opens an array may continue over following lines. Join
        // them before scanning so the scanner never has to know about lines,
        // and keep `line_number` as the entry's provenance: the key is where
        // the reader will look, not the closing bracket.
        let (statement, consumed) = join_continuation(&lines, index - 1, line_number)?;
        index = index - 1 + consumed;

        let mut scanner = Scanner::new(&statement);
        let key = scanner
            .parse_key_path()
            .map_err(|message| error(line_number, message))?;
        scanner.skip_inline_space();
        if !scanner.eat('=') {
            return Err(error(
                line_number,
                format!("expected `=` after key `{key}`, found `{}`", scanner.rest()),
            ));
        }
        scanner.skip_space_comments_and_newlines();
        let value = scanner
            .parse_value()
            .map_err(|message| error(line_number, message))?;
        scanner.skip_inline_space();
        if !scanner.at_end_of_statement() {
            return Err(error(
                line_number,
                format!("unexpected trailing text `{}`", scanner.rest().trim()),
            ));
        }

        let full_key = if table_prefix.is_empty() {
            key
        } else {
            format!("{table_prefix}.{key}")
        };
        if !seen_keys.insert(full_key.clone()) {
            return Err(error(
                line_number,
                format!("key `{full_key}` is set twice in this file"),
            ));
        }
        entries.push(Entry {
            key: full_key,
            value,
            line: line_number,
        });
    }

    Ok(Document { entries })
}

fn parse_table_header(trimmed: &str, line_number: usize) -> Result<String, TomlError> {
    let Some(close) = trimmed.find(']') else {
        return Err(error(
            line_number,
            "unterminated table header: expected `]`",
        ));
    };
    let after = trimmed[close + 1..].trim();
    if !after.is_empty() && !after.starts_with('#') {
        return Err(error(
            line_number,
            format!("unexpected text after table header: `{after}`"),
        ));
    }

    let mut scanner = Scanner::new(&trimmed[1..close]);
    let name = scanner
        .parse_key_path()
        .map_err(|message| error(line_number, message))?;
    scanner.skip_inline_space();
    if !scanner.rest().is_empty() {
        return Err(error(
            line_number,
            format!("unexpected text in table header: `{}`", scanner.rest()),
        ));
    }
    Ok(name)
}

/// Collects the physical lines that make up one logical `key = value`.
///
/// Only an unclosed `[` continues a statement; nothing else in this subset
/// spans lines. Returns the joined text and how many lines it consumed.
fn join_continuation(
    lines: &[&str],
    start: usize,
    line_number: usize,
) -> Result<(String, usize), TomlError> {
    let mut joined = String::new();
    let mut depth = 0i32;
    let mut consumed = 0;

    for line in &lines[start..] {
        depth += bracket_delta(line);
        if !joined.is_empty() {
            joined.push('\n');
        }
        joined.push_str(line);
        consumed += 1;
        if depth <= 0 {
            break;
        }
    }

    if depth > 0 {
        return Err(error(line_number, "unterminated array: expected `]`"));
    }
    Ok((joined, consumed))
}

/// Net bracket depth a line contributes, ignoring brackets inside strings and
/// after a comment marker.
fn bracket_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '#' => break,
            '[' => delta += 1,
            ']' => delta -= 1,
            '"' => {
                // Skip the string body, honouring backslash escapes so that a
                // trailing `\"` does not look like the closing quote.
                while let Some(inner) = characters.next() {
                    match inner {
                        '\\' => {
                            characters.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '\'' => {
                for inner in characters.by_ref() {
                    if inner == '\'' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    delta
}

struct Scanner<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Scanner<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.input[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.position += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_inline_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.position += 1;
        }
    }

    /// Whitespace, newlines, and comments — the interior of a multi-line array.
    fn skip_space_comments_and_newlines(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r') => {
                    self.position += 1;
                }
                Some('#') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn at_end_of_statement(&self) -> bool {
        let rest = self.rest().trim_start_matches([' ', '\t', '\n', '\r']);
        rest.is_empty() || rest.starts_with('#')
    }

    /// A bare or quoted key, possibly dotted. Returns the normalised dotted
    /// form so `a.b` and `[a] b` reach the schema as the same string.
    fn parse_key_path(&mut self) -> Result<String, String> {
        let mut segments = Vec::new();
        loop {
            self.skip_inline_space();
            segments.push(self.parse_key_segment()?);
            self.skip_inline_space();
            if !self.eat('.') {
                return Ok(segments.join("."));
            }
        }
    }

    fn parse_key_segment(&mut self) -> Result<String, String> {
        if matches!(self.peek(), Some('"' | '\'')) {
            let Value::String(text) = self.parse_value()? else {
                unreachable!("a quoted key parses as a string");
            };
            if text.is_empty() {
                return Err("empty quoted key".to_owned());
            }
            return Ok(text);
        }

        let start = self.position;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(match self.peek() {
                Some(character) => format!("expected a key, found `{character}`"),
                None => "expected a key, found end of line".to_owned(),
            });
        }
        Ok(self.input[start..self.position].to_owned())
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('"') => self.parse_basic_string().map(Value::String),
            Some('\'') => self.parse_literal_string().map(Value::String),
            Some('[') => self.parse_array(),
            Some('{') => Err(
                "inline tables ({ ... }) are not part of the paredit configuration schema"
                    .to_owned(),
            ),
            Some(character)
                if character.is_ascii_digit() || character == '+' || character == '-' =>
            {
                self.parse_integer()
            }
            Some(_) => self.parse_bare_word(),
            None => Err("expected a value, found end of line".to_owned()),
        }
    }

    fn parse_bare_word(&mut self) -> Result<Value, String> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
        {
            self.position += 1;
        }
        match &self.input[start..self.position] {
            "true" => Ok(Value::Boolean(true)),
            "false" => Ok(Value::Boolean(false)),
            "" => Err(format!(
                "expected a value, found `{}`",
                self.rest().trim_end()
            )),
            word => Err(format!(
                "`{word}` is not a value: quote it as \"{word}\" if it is a string"
            )),
        }
    }

    fn parse_basic_string(&mut self) -> Result<String, String> {
        if self.rest().starts_with("\"\"\"") {
            return Err(
                "multi-line strings are not part of the paredit configuration schema".to_owned(),
            );
        }
        self.bump();
        let mut text = String::new();
        loop {
            match self.bump() {
                None | Some('\n') => return Err("unterminated string: expected `\"`".to_owned()),
                Some('"') => return Ok(text),
                Some('\\') => text.push(self.parse_escape()?),
                Some(character) => text.push(character),
            }
        }
    }

    fn parse_escape(&mut self) -> Result<char, String> {
        match self.bump() {
            Some('"') => Ok('"'),
            Some('\\') => Ok('\\'),
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),
            Some('b') => Ok('\u{8}'),
            Some('f') => Ok('\u{c}'),
            Some('0') => Ok('\0'),
            Some(marker @ ('u' | 'U')) => {
                let digits = if marker == 'u' { 4 } else { 8 };
                let start = self.position;
                for _ in 0..digits {
                    if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        return Err(format!("`\\{marker}` needs {digits} hex digits"));
                    }
                    self.position += 1;
                }
                let code = u32::from_str_radix(&self.input[start..self.position], 16)
                    .map_err(|_| format!("`\\{marker}` escape is out of range"))?;
                char::from_u32(code)
                    .ok_or_else(|| format!("`\\{marker}{code:04X}` is not a Unicode scalar value"))
            }
            Some(other) => Err(format!("unknown escape `\\{other}`")),
            None => Err("unterminated escape at end of line".to_owned()),
        }
    }

    fn parse_literal_string(&mut self) -> Result<String, String> {
        if self.rest().starts_with("'''") {
            return Err(
                "multi-line strings are not part of the paredit configuration schema".to_owned(),
            );
        }
        self.bump();
        let start = self.position;
        loop {
            match self.bump() {
                None | Some('\n') => return Err("unterminated string: expected `'`".to_owned()),
                Some('\'') => return Ok(self.input[start..self.position - 1].to_owned()),
                Some(_) => {}
            }
        }
    }

    fn parse_integer(&mut self) -> Result<Value, String> {
        let start = self.position;
        if matches!(self.peek(), Some('+' | '-')) {
            self.position += 1;
        }
        let digits_start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
            self.position += 1;
        }
        let raw = &self.input[start..self.position];
        let digits = self.input[digits_start..self.position].replace('_', "");

        if matches!(self.peek(), Some('.' | 'e' | 'E')) {
            return Err(format!(
                "`{raw}...` looks like a float; the paredit configuration schema has no float keys"
            ));
        }
        // `2026-07-29` and `10:30:00` would otherwise be reported as an
        // integer followed by mystery trailing text, which names the symptom
        // rather than what was actually written.
        if matches!(self.peek(), Some('-' | ':' | 'T')) {
            return Err(format!(
                "`{raw}...` looks like a date or time; \
                 the paredit configuration schema has no datetime keys"
            ));
        }
        if digits.is_empty() {
            return Err(format!("`{raw}` is not an integer"));
        }
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(format!("`{raw}` has a leading zero"));
        }

        let signed = if raw.starts_with('-') {
            format!("-{digits}")
        } else {
            digits
        };
        signed
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| format!("`{raw}` does not fit in a 64-bit integer"))
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_space_comments_and_newlines();
            if self.eat(']') {
                return Ok(Value::Array(items));
            }
            if self.peek().is_none() {
                return Err("unterminated array: expected `]`".to_owned());
            }
            items.push(self.parse_value()?);
            self.skip_space_comments_and_newlines();
            if self.eat(',') {
                continue;
            }
            if self.eat(']') {
                return Ok(Value::Array(items));
            }
            return Err(match self.peek() {
                Some(character) => format!("expected `,` or `]` in array, found `{character}`"),
                None => "unterminated array: expected `]`".to_owned(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(document: &Document) -> Vec<(String, String, usize)> {
        document
            .entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.value.to_string(), entry.line))
            .collect()
    }

    #[test]
    fn a_table_prefixes_the_keys_under_it() {
        let document = parse("[lint]\nfail-on = \"error\"\n").expect("parses");
        assert_eq!(
            keys(&document),
            vec![("lint.fail-on".to_owned(), "\"error\"".to_owned(), 2)]
        );
    }

    /// The property the whole package exists for: the line survives parsing.
    #[test]
    fn every_entry_keeps_the_line_it_was_written_on() {
        let document = parse("# a comment\n\n[format]\n\nindent = 4\n").expect("parses");
        assert_eq!(document.entries[0].line, 5);
    }

    #[test]
    fn a_dotted_key_normalises_to_the_same_string_as_a_table() {
        let dotted = parse("lint.fail-on = \"error\"\n").expect("parses");
        let tabled = parse("[lint]\nfail-on = \"error\"\n").expect("parses");
        assert_eq!(dotted.entries[0].key, tabled.entries[0].key);
    }

    #[test]
    fn an_array_may_span_lines_and_carry_comments() {
        let document =
            parse("[paths]\nexclude = [\n  \"a\", # first\n  \"b\",\n]\n").expect("parses");
        assert_eq!(
            document.entries[0].value,
            Value::Array(vec![
                Value::String("a".to_owned()),
                Value::String("b".to_owned())
            ])
        );
        // Provenance is the key's line, not the closing bracket's.
        assert_eq!(document.entries[0].line, 2);
    }

    #[test]
    fn a_bracket_inside_a_string_does_not_open_an_array() {
        let document = parse("[paths]\nexclude = [\"a]b\"]\n").expect("parses");
        assert_eq!(
            document.entries[0].value,
            Value::Array(vec![Value::String("a]b".to_owned())])
        );
    }

    #[test]
    fn integers_booleans_and_escapes_round_trip() {
        let document =
            parse("[format]\nindent = 4\nstrict = true\nname = \"a\\tb\"\n").expect("parses");
        assert_eq!(document.entries[0].value, Value::Integer(4));
        assert_eq!(document.entries[1].value, Value::Boolean(true));
        assert_eq!(document.entries[2].value, Value::String("a\tb".to_owned()));
        assert_eq!(document.entries[2].value.to_string(), "\"a\\tb\"");
    }

    #[test]
    fn a_negative_integer_parses() {
        let document = parse("depth = -1\n").expect("parses");
        assert_eq!(document.entries[0].value, Value::Integer(-1));
    }

    /// Each of these is a shape a general TOML parser would accept and this
    /// schema has no meaning for. Accepting them silently would let a typo
    /// look like a working setting.
    #[test]
    fn unsupported_toml_shapes_are_refused_with_their_line() {
        for (source, needle) in [
            ("[[rule]]\n", "arrays of tables"),
            ("a = { b = 1 }\n", "inline tables"),
            ("a = 1.5\n", "float"),
            ("a = \"\"\"x\"\"\"\n", "multi-line strings"),
            ("a = 2026-07-29\n", "datetime"),
            ("a = 007\n", "leading zero"),
        ] {
            let error = parse(source).unwrap_err();
            assert!(
                error.message.contains(needle),
                "{source:?} should be refused for {needle}, got {error}"
            );
        }
    }

    #[test]
    fn an_error_reports_the_line_it_is_on() {
        let error = parse("[lint]\nfail-on = \"error\"\nbroken\n").unwrap_err();
        assert_eq!(error.line, 3);
    }

    #[test]
    fn a_duplicate_key_is_an_error_rather_than_a_last_one_wins() {
        let error = parse("[lint]\nfail-on = \"error\"\nfail-on = \"warning\"\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("set twice"));
    }

    #[test]
    fn a_duplicate_table_is_an_error() {
        let error = parse("[lint]\na = 1\n[lint]\nb = 2\n").unwrap_err();
        assert!(error.message.contains("declared twice"));
    }

    #[test]
    fn a_bare_word_value_suggests_quoting_it() {
        let error = parse("format = json\n").unwrap_err();
        assert!(error.message.contains("quote it as \"json\""), "{error}");
    }

    #[test]
    fn an_unterminated_array_names_the_key_line() {
        let error = parse("[paths]\nexclude = [\n  \"a\",\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("unterminated array"));
    }

    #[test]
    fn an_empty_document_has_no_entries() {
        assert!(parse("").expect("parses").entries.is_empty());
        assert!(
            parse("# only a comment\n")
                .expect("parses")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn a_quoted_key_keeps_characters_a_bare_key_cannot_hold() {
        let document = parse("[lint]\n\"rule.with.dots\" = true\n").expect("parses");
        assert_eq!(document.entries[0].key, "lint.rule.with.dots");
    }

    #[test]
    fn control_characters_in_a_shown_value_are_escaped_not_printed() {
        assert_eq!(
            Value::String("a\u{1b}b".to_owned()).to_string(),
            "\"a\\u001Bb\""
        );
    }
}
