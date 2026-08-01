//! Every JSON key the shared report envelope emits is spelled in snake_case.
//!
//! The reports that print through `print_report` each hand-write their own
//! field names as `&'static str` literals. There is no `serde` derive anywhere
//! in that path and no key built at runtime, so the spelling of every key in
//! every report is a string literal sitting in the source. Nothing has ever
//! checked those literals. They are uniformly snake_case today by habit, and
//! habit is not a contract: a consumer that has written `report.file_count`
//! has no way to know the next report will not arrive with `findingCount`.
//!
//! This reads the source rather than running the commands. Running them would
//! need a fixture that provokes a finding out of each of ~167 separate
//! analyses, and a command whose fixture happened to produce nothing would be
//! silently unchecked — which is the failure this exists to prevent. Reading
//! the literals covers every report including the ones no fixture reaches, and
//! covers a report added tomorrow the moment it implements the trait.
//!
//! # Why this is scoped to the envelope rather than the workspace
//!
//! The workspace is *not* uniformly snake_case, and a test that assumed it was
//! would fail on around 180 pre-existing keys. Three groups stay out:
//!
//! * The interop formats (`report/interop.rs`) and the LSP and MCP servers
//!   spell keys the way SARIF, Code Climate, LSP, and MCP spell them —
//!   `ruleId`, `physicalLocation`, `inputSchema`. Those are foreign specs.
//! * `inspect capabilities --emit schema` emits JSON Schema, whose keywords
//!   are `$defs`, `$ref`, and `additionalProperties`.
//! * The renderers that predate the shared envelope — `rename`, several
//!   `project-analysis` reports, `query replace`, `migrate run` — use
//!   camelCase. Whether they should is a separate question from whether the
//!   envelope holds its line, and answering it would change published output.
//!
//! None of that needs an exemption list. It falls out of the regions scanned
//! below: none of those three groups implements `Finding` or builds a
//! `FileFindings`, so none of them is read in the first place.

use std::fs;
use std::path::{Path, PathBuf};

/// The envelope's own printer, which spells `schema_version`, `file_count`,
/// `policy`, and `span` on behalf of every report at once.
const ENVELOPE_PRINTER: &str = "packages/core/cli/src/report/render.rs";

/// A Rust token, carrying only what deciding "is this literal a JSON key"
/// needs: the punctuation around it and whether an identifier precedes a
/// paren.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Punct(char),
    Str(String),
}

/// Splits `source` into words, punctuation, and string literals, dropping
/// comments, whitespace, char literals, and lifetimes.
///
/// Tokenizing rather than pattern-matching the text is what makes the rest of
/// this file exact. Both jobs below — deciding whether a literal sits in key
/// position, and finding the end of a brace-delimited body — are wrong on raw
/// text the moment a string or a comment contains a brace, a paren, or a
/// quote. All three occur in these reports, which quote Lisp source.
fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        if current.is_whitespace() {
            index += 1;
        } else if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && chars.get(index + 1) == Some(&'*') {
            index = skip_block_comment(&chars, index);
        } else if current == '\'' {
            index = skip_char_or_lifetime(&chars, index);
        } else if current == '"' {
            let (literal, next) = read_string(&chars, index + 1);
            tokens.push(Token::Str(literal));
            index = next;
        } else if current.is_alphanumeric() || current == '_' {
            let mut end = index;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let word: String = chars[index..end].iter().collect();
            if let Some(hashes) = raw_string_hashes(&word, &chars, end) {
                let (literal, next) = read_raw_string(&chars, end + hashes + 1, hashes);
                tokens.push(Token::Str(literal));
                index = next;
            } else if word == "b" && chars.get(end) == Some(&'"') {
                let (literal, next) = read_string(&chars, end + 1);
                tokens.push(Token::Str(literal));
                index = next;
            } else {
                tokens.push(Token::Word(word));
                index = end;
            }
        } else {
            tokens.push(Token::Punct(current));
            index += 1;
        }
    }

    tokens
}

/// Rust block comments nest, so this counts depth rather than looking for the
/// first `*/`.
fn skip_block_comment(chars: &[char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut index = start;
    while index < chars.len() {
        if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
            depth += 1;
            index += 2;
        } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return index;
            }
        } else {
            index += 1;
        }
    }
    index
}

/// Steps over `'a'`, `'\n'`, or the `'static` in `&'static str`.
///
/// The three share an opening character and only the first two end in one.
/// Treating a lifetime as a char literal would swallow the `"` that starts the
/// next key — and `&'static str` is in the signature of every `json_fields`
/// this file reads.
fn skip_char_or_lifetime(chars: &[char], start: usize) -> usize {
    if chars.get(start + 1) == Some(&'\\') {
        let mut index = start + 2;
        while index < chars.len() && chars[index] != '\'' {
            index += 1;
        }
        return index + 1;
    }
    if chars.get(start + 2) == Some(&'\'') {
        return start + 3;
    }
    start + 1
}

/// The `#` count of a raw string opening at `after_word`, if one does.
fn raw_string_hashes(word: &str, chars: &[char], after_word: usize) -> Option<usize> {
    if word != "r" && word != "br" {
        return None;
    }
    let mut hashes = 0;
    while chars.get(after_word + hashes) == Some(&'#') {
        hashes += 1;
    }
    (chars.get(after_word + hashes) == Some(&'"')).then_some(hashes)
}

/// Reads a string literal whose opening quote is at `start - 1`.
///
/// An escape's payload is dropped and the backslash kept. A key never contains
/// one — it would not be snake_case — so the only thing that matters here is
/// not mistaking an escaped quote for the closing quote.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let mut literal = String::new();
    let mut index = start;
    while index < chars.len() {
        match chars[index] {
            '\\' => {
                literal.push('\\');
                index += 2;
            }
            '"' => return (literal, index + 1),
            other => {
                literal.push(other);
                index += 1;
            }
        }
    }
    (literal, chars.len())
}

fn read_raw_string(chars: &[char], start: usize, hashes: usize) -> (String, usize) {
    let mut index = start;
    while index < chars.len() {
        if chars[index] == '"' && (1..=hashes).all(|step| chars.get(index + step) == Some(&'#')) {
            return (chars[start..index].iter().collect(), index + hashes + 1);
        }
        index += 1;
    }
    (chars[start..].iter().collect(), chars.len())
}

/// The half-open token range of the `open`..`close` pair opening at or after
/// `from`, or `None` if there is no such pair.
fn balanced(tokens: &[Token], from: usize, open: char, close: char) -> Option<(usize, usize)> {
    let start = (from..tokens.len()).find(|index| tokens[*index] == Token::Punct(open))?;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if *token == Token::Punct(open) {
            depth += 1;
        } else if *token == Token::Punct(close) {
            depth -= 1;
            if depth == 0 {
                return Some((start, index + 1));
            }
        }
    }
    None
}

/// The index of the first token of a `#[cfg(test)]` attribute, or the end.
///
/// A test module's `json!` literals are assertions *about* output rather than
/// output, and pinning their spelling would stop a test from constructing a
/// deliberately odd document. Matching on tokens rather than on the text keeps
/// the same six characters inside a doc comment from truncating a whole file's
/// worth of keys away.
fn test_module_start(tokens: &[Token]) -> usize {
    let pattern = [
        Token::Punct('#'),
        Token::Punct('['),
        Token::Word("cfg".to_owned()),
        Token::Punct('('),
        Token::Word("test".to_owned()),
        Token::Punct(')'),
        Token::Punct(']'),
    ];
    tokens
        .windows(pattern.len())
        .position(|window| window == pattern)
        .unwrap_or(tokens.len())
}

/// Whether `tokens` starting at `index` spell `words` as consecutive
/// identifiers separated only by the punctuation in `glue`.
fn matches_words(tokens: &[Token], index: usize, words: &[&str]) -> bool {
    words.iter().enumerate().all(|(offset, word)| {
        matches!(tokens.get(index + offset), Some(Token::Word(found)) if found == word)
    })
}

/// The token ranges of `source` whose string literals become JSON keys.
///
/// Three shapes, which between them are every key a report emits:
///
/// * a `json_fields` body — the finding's own fields;
/// * a `FileFindings::new(..)` argument list — the per-file `summary`, which
///   every report passes inline at the call rather than binding first;
/// * a `-> Value` helper beside a `json_fields`, which is where the repeated
///   `span_json` pattern builds a nested `{ start, end }` its caller never
///   spells out.
///
/// Plus the envelope printer itself, whose keys belong to no single report.
///
/// These ranges are the scope. A module that emits JSON some other way — the
/// SARIF writer, the LSP server, the renderers older than this envelope — is
/// not read, and does not need to be listed anywhere as an exception.
fn scanned_regions(path: &Path, tokens: &[Token]) -> Vec<(usize, usize)> {
    let end = test_module_start(tokens);
    let tokens = &tokens[..end];
    let mut regions = Vec::new();
    let mut has_findings = false;

    for index in 0..tokens.len() {
        if matches_words(tokens, index, &["fn", "json_fields"]) {
            has_findings = true;
            regions.extend(balanced(tokens, index, '{', '}'));
        } else if matches_words(tokens, index, &["FileFindings"])
            && tokens.get(index + 1) == Some(&Token::Punct(':'))
            && tokens.get(index + 2) == Some(&Token::Punct(':'))
            && matches_words(tokens, index + 3, &["new"])
        {
            regions.extend(balanced(tokens, index + 4, '(', ')'));
        }
    }

    if has_findings {
        for index in 0..tokens.len() {
            let arrow = tokens.get(index) == Some(&Token::Punct('-'))
                && tokens.get(index + 1) == Some(&Token::Punct('>'));
            if arrow
                && matches_words(tokens, index + 2, &["Value"])
                && tokens.get(index + 3) == Some(&Token::Punct('{'))
            {
                regions.extend(balanced(tokens, index + 3, '{', '}'));
            }
        }
    }

    if path == Path::new(ENVELOPE_PRINTER) {
        regions.push((0, tokens.len()));
    }

    regions
}

/// The JSON keys the tokens in `region` emit.
///
/// Two positions, which is all this envelope uses:
///
/// * `"name":` — a key in a `json!({ .. })` object literal.
/// * `("name", value)` — an entry in a `json_fields` list or a per-file
///   `summary`, both of which are `Vec<(&'static str, Value)>`.
///
/// The second has to tell a tuple from a call, since `format!("{a}", b)` and
/// `helper("name", x)` are the same three tokens. What precedes the paren
/// decides it: a call's paren follows an identifier or a macro's `!`, and a
/// tuple's follows punctuation — `[`, `,`, `(`, `=`.
fn emitted_keys(region: &[Token]) -> Vec<String> {
    let mut keys = Vec::new();
    for (index, token) in region.iter().enumerate() {
        let Token::Str(literal) = token else {
            continue;
        };
        match region.get(index + 1) {
            Some(Token::Punct(':')) => keys.push(literal.clone()),
            Some(Token::Punct(',')) if index >= 1 => {
                if region[index - 1] != Token::Punct('(') {
                    continue;
                }
                let is_call =
                    index >= 2 && matches!(region[index - 2], Token::Word(_) | Token::Punct('!'));
                if !is_call {
                    keys.push(literal.clone());
                }
            }
            _ => {}
        }
    }
    keys
}

fn is_snake_case(key: &str) -> bool {
    key.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && key.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Every `.rs` file the workspace builds from.
fn workspace_sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![PathBuf::from("packages"), PathBuf::from("src")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every key the scan finds, paired with the file it came from.
fn report_keys() -> Vec<(PathBuf, String)> {
    let mut keys = Vec::new();
    for path in workspace_sources() {
        let source = fs::read_to_string(&path).expect("read workspace source");
        let tokens = tokenize(&source);
        for (start, end) in scanned_regions(&path, &tokens) {
            for key in emitted_keys(&tokens[start..end]) {
                keys.push((path.clone(), key));
            }
        }
    }
    keys
}

/// The contract itself.
#[test]
fn every_report_json_key_is_snake_case() {
    let offenders = report_keys()
        .into_iter()
        .filter(|(_, key)| !is_snake_case(key))
        .map(|(path, key)| format!("{}: {key:?}", path.display()))
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "these report keys are not snake_case:\n{}\n\nEvery other key in the shared \
         report envelope is, and a consumer that has written `report.file_count` cannot \
         tell that this one will not be. Rename the key, or — if it belongs to a foreign \
         schema whose spelling is not ours to pick — emit it somewhere this scan does not \
         read, the way SARIF and LSP already do.",
        offenders.join("\n")
    );
}

/// A scan that quietly stops finding anything turns the test above green, so
/// the size of what it finds is pinned too.
///
/// Floors rather than exact counts: a report added next week must not have to
/// come back and edit a number, and only a report *disappearing* from the scan
/// is worth failing over.
#[test]
fn the_scan_still_reaches_every_report() {
    let keys = report_keys();
    let files = keys
        .iter()
        .map(|(path, _)| path)
        .collect::<std::collections::BTreeSet<_>>();

    assert!(
        files.len() >= 150,
        "the scan found keys in only {} files; it reached 169 when written, and one \
         report per file. Something stopped matching — check that `json_fields` and \
         `FileFindings::new` are still spelled that way.",
        files.len()
    );
    assert!(
        keys.len() >= 650,
        "the scan found only {} keys, down from 773. See above.",
        keys.len()
    );
}

/// The envelope's own keys are the ones every report shares, so losing sight
/// of them would leave the most-published names in the tool unchecked.
///
/// A subset rather than an exact list: a format that grows a field should not
/// have to come back here, and its casing is checked by the contract above
/// either way.
#[test]
fn the_scan_still_sees_the_shared_envelope_keys() {
    let path = PathBuf::from(ENVELOPE_PRINTER);
    let source = fs::read_to_string(&path).expect("read the envelope printer");
    let tokens = tokenize(&source);
    let found = scanned_regions(&path, &tokens)
        .into_iter()
        .flat_map(|(start, end)| emitted_keys(&tokens[start..end]))
        .collect::<Vec<_>>();

    for key in [
        "schema_version",
        "report",
        "file_count",
        "finding_count",
        "policy",
        "gate",
        "passed",
        "violations",
        "files",
        "path",
        "dialect",
        "dialect_modelled",
        "findings",
        "kind",
        "line",
        "span",
        "start",
        "end",
    ] {
        assert!(
            found.contains(&key.to_owned()),
            "the scan no longer sees the envelope's {key:?} key, so it is no longer \
             checking the keys every report publishes: {found:?}"
        );
    }
}

/// The scanner finds a bad key, and does not find the shapes that only look
/// like one.
///
/// Without this the whole file could pass by matching nothing at all.
#[test]
fn the_scanner_reads_keys_and_not_the_shapes_that_resemble_them() {
    let region = r#"
        // a comment with a "quoted" word and an unbalanced ( in it
        fn probe(text: &str) -> Vec<(&'static str, Value)> {
            let quote = '"';
            let brace = '}';
            let label = format!("not_a_key", text);
            let called = helper("also_not_a_key", text);
            vec![
                ("good_key", json!(1)),
                ("badKey", json!(2)),
                ("nested", json!({ "inner_key": 3, "innerBad": 4 })),
            ]
        }
    "#;
    let keys = emitted_keys(&tokenize(region));
    assert_eq!(
        keys.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["good_key", "badKey", "nested", "inner_key", "innerBad"],
    );
}

/// A lookup table of Lisp operator names is the same tokens as a `summary`
/// entry, and several of these reports hold one. Region scoping is what tells
/// them apart, so it is checked rather than assumed.
#[test]
fn a_lookup_table_outside_a_scanned_region_is_not_read_as_keys() {
    let source = r#"
        const OPERATORS: &[(&str, Arity)] = &[("make-instance", Arity::Any)];

        impl Finding for Probe {
            fn json_fields(&self) -> Vec<(&'static str, Value)> {
                vec![("operator", json!(self.operator))]
            }
        }
    "#;
    let tokens = tokenize(source);
    let keys = scanned_regions(Path::new("probe.rs"), &tokens)
        .into_iter()
        .flat_map(|(start, end)| emitted_keys(&tokens[start..end]))
        .collect::<Vec<_>>();
    assert_eq!(keys, vec!["operator".to_owned()]);
}

/// Nothing in a `#[cfg(test)]` module is part of the output contract.
#[test]
fn a_test_module_is_not_read_as_output() {
    let source = r#"
        impl Finding for Probe {
            fn json_fields(&self) -> Vec<(&'static str, Value)> {
                vec![("real_key", json!(1))]
            }
        }

        #[cfg(test)]
        mod tests {
            fn probe() -> Value {
                json!({ "deliberatelyOdd": 1 })
            }
        }
    "#;
    let tokens = tokenize(source);
    let keys = scanned_regions(Path::new("probe.rs"), &tokens)
        .into_iter()
        .flat_map(|(start, end)| emitted_keys(&tokens[start..end]))
        .collect::<Vec<_>>();
    assert_eq!(keys, vec!["real_key".to_owned()]);
}
