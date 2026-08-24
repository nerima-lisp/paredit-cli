//! The one printer every report in this package goes through.

use crate::args::ReportFormat;
use crate::error::{CliError, CliResult};
use crate::runtime::Verbosity;
use crate::shared::terminal_safe;
use std::collections::BTreeMap;
use std::io::IsTerminal;

use serde_json::{Value, json};

use super::interop::{self, Flattened};
use super::{FileFindings, Finding, ReportPolicy};

/// Prints a report in the requested format.
///
/// `command` names the report in its own gate message and JSON, so a consumer
/// aggregating several of these can tell which produced a finding without
/// tracking which process wrote it.
///
/// The two native formats read the `FileFindings` tree directly, since both
/// print per-file structure the interop formats have no place for (the summary
/// aggregation, the per-file dialect notice). Everything else goes through
/// [`Flattened`], which is the same findings with the file grouping dissolved.
///
/// `verbosity` reaches the text format only. A machine format is asked for by
/// a consumer that parses all of it, so `--output json` and the interop
/// formats stay at full detail whatever the dial says.
pub fn print_report<F: Finding>(
    command: &'static str,
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
    output: ReportFormat,
    verbosity: Verbosity,
) -> CliResult<()> {
    match output {
        ReportFormat::Text => print_text(reports, policy, verbosity),
        ReportFormat::Json => print_json(command, reports, policy)?,
        other => {
            let flat = Flattened::new(command, reports, policy);
            match other {
                ReportFormat::Sarif => {
                    println!("{}", render_json(command, &interop::sarif(&flat))?);
                }
                ReportFormat::CodeClimate => {
                    println!("{}", render_json(command, &interop::code_climate(&flat))?);
                }
                ReportFormat::Junit => print!("{}", interop::junit(&flat)),
                ReportFormat::Csv => print!("{}", interop::delimited(&flat, true)),
                ReportFormat::Tsv => print!("{}", interop::delimited(&flat, false)),
                ReportFormat::Html => print!("{}", interop::html(&flat)),
                ReportFormat::Markdown => print!("{}", interop::markdown(&flat)),
                ReportFormat::Github => print!("{}", interop::github(&flat)),
                // Handled above; repeating them here rather than using a
                // wildcard keeps a format added later a compile error.
                ReportFormat::Text | ReportFormat::Json => unreachable!(),
            }
        }
    }
    Ok(())
}

fn print_text<F: Finding>(
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
    verbosity: Verbosity,
) {
    println!("files\t{}", reports.len());
    println!("finding_count\t{}", policy.finding_count);
    for (name, value) in aggregate(reports) {
        println!("{name}\t{value}");
    }
    if let Some(gate) = policy.gate {
        println!("policy\t{gate}\tpassed={}", policy.passed);
    }

    // Quiet is the summary block and nothing else: the counts and the gate
    // above are what a caller who is only checking whether to act needs, and
    // the per-finding rows below are exactly the part they would discard.
    if verbosity == Verbosity::Quiet {
        return;
    }

    // Tab-separated rows are the contract every script and every one of this
    // workspace's `assert_cmd` tests reads — `cargo test` never runs with a
    // terminal on the other end, so that path is unconditionally preserved.
    // A human at a live terminal gets an aligned, width-clamped table
    // instead, built from the exact same rows.
    let human = std::io::stdout().is_terminal();
    let mut finding_rows = Vec::new();
    // Parallel to `finding_rows`: `aligned_table_lines` needs every row before
    // it can size a column, so a row's detail lines cannot be printed as they
    // are produced and are held here to print under their own aligned line.
    let mut finding_details: Vec<Vec<String>> = Vec::new();

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthis report covers Common Lisp only",
                terminal_safe(&report.path.display()),
                report.dialect.label(),
            );
            continue;
        }
        let path_str = terminal_safe(&report.path.display()).to_string();
        for finding in &report.findings {
            let mut row = vec![
                finding.kind().to_owned(),
                path_str.clone(),
                report.line_of(finding).to_string(),
            ];
            row.extend(
                finding
                    .text_columns()
                    .into_iter()
                    .map(|column| terminal_safe(&column).to_string()),
            );
            let details = if verbosity == Verbosity::Detailed {
                detail_lines(finding)
            } else {
                Vec::new()
            };
            if human {
                finding_rows.push(row);
                finding_details.push(details);
            } else {
                println!("{}", row.join("\t"));
                for line in details {
                    println!("{line}");
                }
            }
        }
    }

    if human {
        for (line, details) in aligned_table_lines(&finding_rows, crate::terminal::width())
            .into_iter()
            .zip(finding_details)
        {
            println!("{line}");
            for detail in details {
                println!("{detail}");
            }
        }
    }
}

/// A finding's JSON fields as indented `key\tvalue` lines, the extra context
/// `--verbosity detailed` adds under the finding's own row.
///
/// A string field is unwrapped to its contents rather than printed as the
/// quoted JSON literal `Value`'s `Display` would give, so it reads like the
/// text columns beside it; everything else keeps its JSON rendering, which is
/// already free of anything a terminal would act on.
fn detail_lines<F: Finding>(finding: &F) -> Vec<String> {
    finding
        .json_fields()
        .into_iter()
        .map(|(name, value)| {
            let rendered = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
            format!("\t\t{name}\t{}", terminal_safe(&rendered))
        })
        .collect()
}

/// Column-aligns `rows` (padding every column but the last, which is left
/// ragged since it is usually a message and padding it would only add
/// trailing whitespace) and clips each resulting line to `terminal_width`.
///
/// Clips rather than wraps: a wrapped row would need to keep collapsing back
/// to "which finding is this" for whatever comes after it, and a `Finding`'s
/// text columns are a summary already truncated by [`bounded_preview`]-style
/// limits upstream — the rare line still too wide for the terminal is
/// exactly the one a human least needs every last byte of on screen.
///
/// [`bounded_preview`]: crate::shared::bounded_preview
fn aligned_table_lines(rows: &[Vec<String>], terminal_width: usize) -> Vec<String> {
    let Some(column_count) = rows.iter().map(Vec::len).max() else {
        return Vec::new();
    };
    let mut widths = vec![0usize; column_count];
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }

    rows.iter()
        .map(|row| {
            let mut line = String::new();
            let last = row.len().saturating_sub(1);
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    line.push_str("  ");
                }
                if index == last {
                    line.push_str(cell);
                } else {
                    // `{cell:<width$}` pads by `chars().count()`, which is
                    // exactly the miscount `display_width` exists to fix, so
                    // the padding is spelled out by hand: the cell itself,
                    // then however many single-column spaces make up the
                    // difference between its display width and the column's.
                    line.push_str(cell);
                    let pad = widths[index].saturating_sub(display_width(cell));
                    for _ in 0..pad {
                        line.push(' ');
                    }
                }
            }
            clip_to_width(&line, terminal_width)
        })
        .collect()
}

/// Clips `line` to `width` display columns, marking the cut with `…`.
///
/// Below four columns there is no sensible line left once the marker is
/// accounted for, so an unreasonably narrow terminal gets the unclipped line
/// rather than something even less readable.
fn clip_to_width(line: &str, width: usize) -> String {
    if display_width(line) <= width || width < 4 {
        return line.to_owned();
    }
    // Walk whole `char`s and stop before the running display width would
    // exceed the budget the `…` marker leaves behind (`width - 1`). Deciding
    // per whole `char` rather than per byte or per fixed char count means a
    // wide character is either kept in full or dropped in full — it is never
    // possible to include half of one.
    let budget = width - 1;
    let mut kept = String::new();
    let mut used = 0usize;
    for ch in line.chars() {
        let ch_width = display_width_of_char(ch);
        if used + ch_width > budget {
            break;
        }
        used += ch_width;
        kept.push(ch);
    }
    format!("{kept}…")
}

/// The terminal display width of `s`: 2 columns for a character in the
/// common East Asian Wide/Fullwidth ranges, 1 for everything else.
///
/// This is a hand-rolled subset of UAX #11, not a full implementation — this
/// workspace has no terminal-width crate dependency by convention (see
/// `color.rs`'s hand-rolled SGR strings and `terminal.rs`'s hand-rolled ioctl
/// parsing), and the ranges below cover the scripts this tool's own
/// `Language::Japanese` runtime option and its diagnostic text actually use:
/// CJK Unified Ideographs, Hiragana, Katakana, Hangul Syllables, Fullwidth
/// Forms, and CJK punctuation/symbols.
///
/// `pub` rather than private to this module: `src/presentation/tui/render.rs`
/// (a different crate) has the identical `.chars().count()`-based miscount in
/// its own `clip()`, and already depends on this crate for
/// `paredit_core_cli::terminal::width`. Fixing that sibling is out of scope
/// here — `paredit tui`'s accessibility is tracked separately as H7 in
/// `docs/src/project/feature-candidates.md` — but there is no reason to make
/// the fix unreachable for whoever picks that up.
#[must_use]
pub fn display_width(s: &str) -> usize {
    s.chars().map(display_width_of_char).sum()
}

/// The display width of a single `char`; see [`display_width`].
const fn display_width_of_char(ch: char) -> usize {
    let is_wide = matches!(ch,
        '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{AC00}'..='\u{D7A3}' // Hangul Syllables
        | '\u{FF00}'..='\u{FFEF}' // Halfwidth and Fullwidth Forms
    );
    if is_wide { 2 } else { 1 }
}

fn print_json<F: Finding>(
    command: &'static str,
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
) -> CliResult<()> {
    let mut report = json!({
        "schema_version": 1,
        "report": command,
        "file_count": reports.len(),
        "finding_count": policy.finding_count,
        "policy": {
            "gate": policy.gate,
            "passed": policy.passed,
            "violations": &policy.violations,
        },
        "files": reports.iter().map(file_json::<F>).collect::<Vec<_>>(),
    });
    for (name, value) in aggregate(reports) {
        report[name] = value;
    }
    println!("{}", render_json(command, &report)?);
    Ok(())
}

/// Serializes a document this module just built.
///
/// Every caller passes a [`Value`] assembled here from `json!`, so the only
/// documented failure of `to_string_pretty` — a map with non-string keys —
/// cannot arise. It stays a typed error rather than an `unwrap()` because
/// [`CliError::Json`] already means "a JSON artifact this tool owns did not
/// render", and an impossible case is a bad reason to introduce a panic into
/// the one function every report in the workspace prints through.
fn render_json(command: &'static str, document: &Value) -> CliResult<String> {
    serde_json::to_string_pretty(document).map_err(|source| CliError::Json {
        context: format!("failed to render the {command} report"),
        source,
    })
}

fn file_json<F: Finding>(report: &FileFindings<F>) -> Value {
    let mut object = json!({
        "path": report.path.display().to_string(),
        "dialect": report.dialect.label(),
        "dialect_modelled": report.dialect_modelled,
        "findings": report
            .findings
            .iter()
            .map(|finding| {
                let mut entry = json!({
                    "kind": finding.kind(),
                    "line": report.line_of(finding),
                    "span": {
                        "start": finding.span().start().get(),
                        "end": finding.span().end().get(),
                    },
                });
                for (name, value) in finding.json_fields() {
                    entry[name] = value;
                }
                entry
            })
            .collect::<Vec<_>>(),
    });
    for (name, value) in &report.summary {
        object[*name] = value.clone();
    }
    object
}

/// Sums each per-file summary key across the whole run.
///
/// Numeric keys add; anything else is dropped rather than concatenated, since
/// there is no meaningful whole-run value for a per-file string. Key order
/// follows the first file that declared each key, so the top of the output does
/// not reshuffle when a later file happens to carry an extra count.
fn aggregate<F: Finding>(reports: &[FileFindings<F>]) -> Vec<(&'static str, Value)> {
    let mut order: Vec<&'static str> = Vec::new();
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for report in reports {
        for (name, value) in &report.summary {
            let Some(number) = value.as_u64() else {
                continue;
            };
            if !totals.contains_key(*name) {
                order.push(name);
            }
            *totals.entry((*name).to_owned()).or_default() += number;
        }
    }
    order
        .into_iter()
        .map(|name| (name, json!(totals[name])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
    use std::path::PathBuf;

    #[derive(Debug)]
    struct Probe;

    impl Finding for Probe {
        fn kind(&self) -> &'static str {
            "probe"
        }
        fn span(&self) -> ByteSpan {
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(1))
        }
        fn text_columns(&self) -> Vec<String> {
            Vec::new()
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            Vec::new()
        }
    }

    fn file(summary: Vec<(&'static str, Value)>) -> FileFindings<Probe> {
        FileFindings::new(
            PathBuf::from("t.lisp"),
            Dialect::CommonLisp,
            true,
            "(a)\n(b)\n",
            Vec::new(),
            summary,
        )
    }

    #[derive(Debug)]
    struct Annotated(Vec<(&'static str, Value)>);

    impl Finding for Annotated {
        fn kind(&self) -> &'static str {
            "annotated"
        }
        fn span(&self) -> ByteSpan {
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(1))
        }
        fn text_columns(&self) -> Vec<String> {
            Vec::new()
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            self.0.clone()
        }
    }

    #[test]
    fn detail_lines_indent_each_json_field_and_unwrap_strings() {
        let finding = Annotated(vec![
            ("name", json!("foo")),
            ("depth", json!(3)),
            ("nested", json!({"a": 1})),
        ]);
        assert_eq!(
            detail_lines(&finding),
            vec![
                "\t\tname\tfoo".to_owned(),
                "\t\tdepth\t3".to_owned(),
                "\t\tnested\t{\"a\":1}".to_owned(),
            ]
        );
    }

    /// A detail line is emitted straight to a terminal, so a control character
    /// smuggled through a finding's own JSON must not survive as one.
    #[test]
    fn detail_lines_escape_a_terminal_control_sequence() {
        let finding = Annotated(vec![("name", json!("a\x1b[31mb"))]);
        let lines = detail_lines(&finding);
        assert!(!lines[0].contains('\x1b'), "unescaped escape in {lines:?}");
    }

    #[test]
    fn a_finding_with_no_json_fields_has_no_detail_lines() {
        assert!(detail_lines(&Probe).is_empty());
    }

    #[test]
    fn numeric_summary_keys_add_across_files() {
        let reports = [
            file(vec![("widgets", json!(2))]),
            file(vec![("widgets", json!(3))]),
        ];
        assert_eq!(aggregate(&reports), vec![("widgets", json!(5))]);
    }

    #[test]
    fn a_non_numeric_summary_key_is_dropped_rather_than_concatenated() {
        let reports = [file(vec![("note", json!("hello"))])];
        assert!(aggregate(&reports).is_empty());
    }

    #[test]
    fn aggregating_no_files_yields_no_keys() {
        let reports: [FileFindings<Probe>; 0] = [];
        assert!(aggregate(&reports).is_empty());
    }

    #[test]
    fn aligned_table_lines_pads_every_column_but_the_last() {
        let rows = vec![
            vec!["short".to_owned(), "a.lisp".to_owned(), "hi".to_owned()],
            vec![
                "much-longer".to_owned(),
                "b.lisp".to_owned(),
                "yo".to_owned(),
            ],
        ];
        let lines = aligned_table_lines(&rows, 80);
        assert_eq!(lines.len(), 2);
        // The first column is padded to the width of "much-longer" (11) plus
        // the two-space separator; "a.lisp"/"b.lisp" are already at their
        // column's max width, so only the separator follows them. The last
        // column ("hi"/"yo") is never padded, since nothing follows it.
        assert_eq!(
            lines[0],
            format!("short{}a.lisp  hi", " ".repeat(11 - 5 + 2))
        );
        assert_eq!(lines[1], "much-longer  b.lisp  yo");
    }

    #[test]
    fn aligned_table_lines_of_no_rows_is_empty() {
        assert!(aligned_table_lines(&[], 80).is_empty());
    }

    #[test]
    fn aligned_table_lines_tolerates_ragged_rows() {
        let rows = vec![vec!["a".to_owned(), "b".to_owned()], vec!["c".to_owned()]];
        // Neither row indexes past its own length; nothing panics.
        assert_eq!(aligned_table_lines(&rows, 80).len(), 2);
    }

    #[test]
    fn clip_to_width_leaves_a_short_line_alone() {
        assert_eq!(clip_to_width("short", 80), "short");
        assert_eq!(clip_to_width("exact", 5), "exact");
    }

    #[test]
    fn clip_to_width_marks_a_cut_line_with_an_ellipsis() {
        let clipped = clip_to_width("this line is much too long to fit", 10);
        assert_eq!(clipped, "this line…");
        assert_eq!(clipped.chars().count(), 10);
    }

    #[test]
    fn clip_to_width_leaves_a_line_alone_below_the_sensible_minimum() {
        // Nothing useful is left of a line clipped to fewer than four
        // columns, so an absurdly narrow terminal gets the line as-is.
        assert_eq!(clip_to_width("still too long", 3), "still too long");
    }

    #[test]
    fn display_width_of_ascii_matches_char_count() {
        assert_eq!(display_width("short"), "short".chars().count());
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("hi there!"), "hi there!".chars().count());
    }

    #[test]
    fn display_width_counts_japanese_characters_as_two_columns_each() {
        // "日本語" is three characters, each occupying two terminal columns.
        assert_eq!(display_width("日本語"), 6);
        assert_ne!(display_width("日本語"), "日本語".chars().count());
    }

    #[test]
    fn aligned_table_lines_aligns_a_mix_of_ascii_and_japanese_rows() {
        let rows = vec![
            vec!["ascii".to_owned(), "a.lisp".to_owned(), "hi".to_owned()],
            vec!["日本語".to_owned(), "b.lisp".to_owned(), "yo".to_owned()],
        ];
        let lines = aligned_table_lines(&rows, 80);
        assert_eq!(lines.len(), 2);
        // Both first-column cells are padded to the same *display* width
        // (6 columns: "ascii" is 5 chars/5 columns, "日本語" is 3
        // chars/6 columns), plus the two-space separator, so the second
        // column starts at the same screen position on both lines.
        assert_eq!(lines[0], "ascii   a.lisp  hi");
        assert_eq!(lines[1], "日本語  b.lisp  yo");
    }

    #[test]
    fn clip_to_width_truncates_japanese_text_without_splitting_a_character() {
        // Ten 2-column characters (20 columns of display width) clipped to a
        // width of 11 leaves room for 5 whole characters (10 columns) plus
        // the `…` marker; a half-rendered character must never appear.
        let clipped = clip_to_width("日本語日本語日本語日", 11);
        assert!(clipped.ends_with('…'));
        assert!(display_width(&clipped) <= 11);
        // Every char in the clipped output round-trips through `char`
        // parsing untouched, i.e. nothing was split at the byte level.
        assert!(
            clipped
                .chars()
                .all(|ch| ch.len_utf8() == ch.to_string().len())
        );
    }

    #[test]
    fn clip_to_width_handles_an_all_wide_char_string_right_at_the_boundary() {
        // "日本語" is 3 characters at 2 columns each: display width exactly
        // 6. Landing exactly on the width threshold (not comfortably under
        // or over it, unlike the 20-vs-11 margin above) is the case an
        // off-by-one in the `char`-vs-column budget math would most likely
        // miss.
        let exact = "日本語";
        assert_eq!(display_width(exact), 6);
        assert_eq!(
            clip_to_width(exact, 6),
            exact,
            "a string whose display width exactly equals the target width \
             must be returned unclipped, not truncated for being one column \
             over"
        );

        // One more 2-column character pushes display width to 8, one column
        // past the same width-6 budget used above. Losing a single column of
        // budget cannot be paid for by dropping half a wide character, so
        // the whole trailing character must go, not just enough of it to fit
        // exactly.
        let one_over = "日本語日";
        assert_eq!(display_width(one_over), 8);
        let clipped = clip_to_width(one_over, 6);
        assert_eq!(clipped, "日本…");
        assert!(display_width(&clipped) <= 6);
    }
}
