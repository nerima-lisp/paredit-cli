//! Every file in a corpus, through the parser, the formatter, and the selector.
//!
//! Section I item I10. The rest of the suite tests this tool against inputs
//! this tool's authors wrote, which is a systematic blind spot: a construct
//! nobody thought of is a construct nobody wrote a fixture for. A corpus test
//! closes it by asserting *invariants* rather than outputs, over code the
//! authors did not write.
//!
//! Five invariants, each of which holds regardless of what the file contains:
//!
//! 1. **Parsing terminates without panicking.** A parse *error* is fine — real
//!    Lisp carries reader macros this tool does not model — but a panic is a
//!    crash in a tool that agents run unattended.
//! 2. **Parsing is lossless.** `tree.source()` is the input, byte for byte. The
//!    whole edit layer rewrites spans of that string; a parser that dropped a
//!    byte would corrupt every rewrite built on it.
//! 3. **Formatting is idempotent.** `format(format(x)) == format(x)`. A
//!    formatter without this property produces diffs that never converge, which
//!    is worse than not formatting.
//! 4. **Every path the tree reports is selectable.** The tree hands out paths
//!    and the selector consumes them; a path the tree produced and the selector
//!    rejects is an internal contradiction.
//! 5. **No line of formatted output starts at or left of the column of its
//!    enclosing form's opening delimiter.** Invariants 1-4 are all round-trips
//!    through the tool's own tree, so a layout that is merely *wrong* satisfies
//!    every one of them as long as it is wrong consistently. This one is an
//!    oracle from outside the round-trip: a child rendered at or left of the
//!    delimiter that contains it no longer reads as being inside it, in any
//!    Lisp anyone writes.
//!
//! ## Where the corpus comes from
//!
//! `tests/fixtures/corpus` is vendored and always runs, so the invariants are
//! checked on every CI run without network access. It is small on purpose: it
//! holds the constructs that have historically been awkward, not a sample of
//! ordinary code.
//!
//! `PAREDIT_CORPUS_DIR` points at a real checkout — SBCL's source tree, a
//! Quicklisp dist, a company monorepo — for a run that means something at
//! scale. `scripts/fetch-corpus.sh` fetches one. Colon-separated for several.
//!
//! ```sh
//! PAREDIT_CORPUS_DIR=~/quicklisp/dists cargo test --test corpus -- --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use paredit_cli::dialect::Dialect;
use paredit_cli::sexpr::{ExpressionKind, ExpressionPath, Formatter, ReaderPrefix, SyntaxTree};
use paredit_core_cli::report::render::display_width;

/// Extensions the corpus walker treats as Lisp-family source.
///
/// Deliberately derived from `Dialect::from_extension` rather than listed, so a
/// dialect added to the tool is covered here without a second edit.
fn is_lisp_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(Dialect::from_extension)
        .is_some_and(|dialect| dialect != Dialect::Unknown)
}

/// Files that must not be read as source even when their extension matches.
///
/// A corpus checkout routinely contains build output and vendored copies; both
/// inflate the run without testing anything the originals do not.
fn is_skipped_directory(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "build" | "dist" | "vendor" | "node_modules" | ".paredit.cleanup"
    )
}

/// Every Lisp source file under `root`, sorted so a failure is reproducible.
///
/// Symlinks are not followed: a corpus checkout can contain a link to its own
/// parent, and the walk has to terminate.
fn corpus_files(root: &Path, budget: &mut usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                break;
            }
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_skipped_directory(&name) {
                    stack.push(path);
                }
            } else if metadata.is_file() && is_lisp_source(&path) {
                *budget -= 1;
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The corpus roots for this run.
fn corpus_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("tests/fixtures/corpus")];
    if let Ok(extra) = std::env::var("PAREDIT_CORPUS_DIR") {
        roots.extend(
            extra
                .split(':')
                .filter(|entry| !entry.trim().is_empty())
                .map(PathBuf::from),
        );
    }
    roots
}

/// How many files one run will read.
///
/// A bound rather than a sample: a corpus of a hundred thousand files would
/// turn `cargo test` into a batch job, and the point of this test is that it
/// runs every time. The count reached is printed, so a truncated run says so
/// instead of looking complete.
const MAX_CORPUS_FILES: usize = 4_000;

/// The largest file the corpus test will read, in bytes.
const MAX_CORPUS_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// The largest number of column violations one file will report.
///
/// A file that breaks the invariant usually breaks it on every line of one
/// form, and a corpus run over a real checkout would otherwise produce a
/// failure message measured in megabytes. The first few are enough to name
/// the form.
const MAX_COLUMN_VIOLATIONS_PER_FILE: usize = 5;

/// A byte range in the formatted text, half open.
type Region = (usize, usize);

/// The lists and the opaque tokens of one parsed document.
///
/// Both are collected from the parse tree rather than by re-scanning the text,
/// which is the whole reason this check can be trusted. A hand-written
/// delimiter scanner has to re-implement the reader to know that the `(` in
/// `#\(`, `?\(`, `\(`, `"("` and `|a (b|` is not a delimiter — and getting any
/// one of those wrong reports dozens of violations that are not there. The
/// dialect reader already made every one of those decisions; asking the tree
/// for the answer inherits all of them, for all ten dialects at once, and
/// cannot drift from the parser the formatter itself ran on.
#[derive(Default)]
struct Layout {
    /// `content_span` of every list: the range from its opening delimiter
    /// through its closing one, reader prefixes excluded, so the recorded
    /// column is the delimiter's and not the quote's in front of it.
    lists: Vec<Region>,
    /// Every atom and every comment, i.e. every run of bytes whose interior is
    /// text rather than structure. A line that begins inside one of these is a
    /// continuation line of a multi-line string, a `|bar symbol|` or a
    /// `#| block comment |#`; its indentation is the author's data, not the
    /// formatter's layout, and it is exempt.
    opaque: Vec<Region>,
}

impl Layout {
    fn of(tree: &SyntaxTree) -> Self {
        let mut layout = Self::default();
        // An explicit stack, not recursion: corpus files reach nesting depths
        // that overflow a test thread's stack.
        let root = tree.root_view();
        let mut stack = vec![&root];
        while let Some(view) = stack.pop() {
            match view.kind {
                ExpressionKind::List => layout.lists.push((
                    view.content_span.start().get(),
                    view.content_span.end().get(),
                )),
                ExpressionKind::Atom => layout
                    .opaque
                    .push((view.span.start().get(), view.span.end().get())),
                ExpressionKind::Root => (),
            }
            stack.extend(view.children.iter());
        }
        layout.opaque.extend(
            tree.comments()
                .map(|comment| (comment.span().start().get(), comment.span().end().get())),
        );
        // Sorted so both can be swept alongside the lines in one pass. Lists
        // are properly nested, so ordering by start alone puts every enclosing
        // list before the ones it contains.
        layout.lists.sort_unstable();
        layout.opaque.sort_unstable();
        layout
    }
}

/// Every top-level item the formatter reproduced verbatim rather than laid
/// out, as byte ranges in the formatted text.
///
/// A mirror of `Formatter::plan_top_level`
/// (`packages/core/syntax/src/sexpr/formatter/core.rs`), which decides two
/// things this has to agree with exactly:
///
/// * **What a top-level item is.** Usually one root child, but a child whose
///   reader prefixes include `Metadata` (Clojure's `^{:doc "x"} (defn ...)`)
///   is merged with the root child after it, repeatedly, into a single item
///   spanning both. Modelling those as separate forms exempts the wrong range
///   when a comment sits in the second member and misses the case below
///   entirely.
/// * **When that item renders verbatim.** Either because it *is* a merged
///   metadata chain (`verbatim = node_id != end_node_id`, set before any
///   comment is considered, so a chain is verbatim in a file with no comments
///   at all), or because a comment starts anywhere inside the merged span.
///   One comment is enough for the whole item — every line of it, including
///   sibling subforms with no comment of their own.
///
/// `a_comment_anywhere_makes_a_whole_top_level_form_verbatim` and
/// `a_metadata_prefix_chain_is_verbatim_as_one_merged_item` pin both halves
/// against the real formatter, so this exemption stops applying the moment
/// either changes.
///
/// The formatter also attaches a same-line trailing comment to the item it
/// follows. That is deliberately not modelled: such a comment starts at or
/// after the item's end, so it can neither make this item verbatim nor be
/// mistaken for an interior comment of the next one, which skips every
/// comment starting before its own `start` regardless.
///
/// Those lines have to be exempt from invariant 5, and not because the
/// invariant is inconvenient: their indentation was chosen by whoever wrote
/// the file, and this tool copied it. Real code makes that distinction matter.
/// `seq.el` wraps six hundred lines in `(seq--when-emacs-25-p` and leaves the
/// body at column 0; Emacs Lisp indents with tabs. Asserting invariant 5 over
/// those lines would be asserting it against GNU ELPA's house style, which
/// this tool neither chose nor can fix — and the failures would drown the ones
/// that are about the formatter.
fn verbatim_regions(tree: &SyntaxTree) -> Vec<Region> {
    let mut comment_starts: Vec<usize> = tree
        .comments()
        .map(|comment| comment.span().start().get())
        .collect();
    comment_starts.sort_unstable();

    let root = tree.root_view();
    let children = &root.children;
    let mut regions: Vec<Region> = Vec::new();
    let mut index = 0usize;
    let mut cursor = 0usize;

    while index < children.len() {
        let start = children[index].span.start().get();
        let mut last = index;
        while children[last]
            .reader_prefixes
            .contains(&ReaderPrefix::Metadata)
            && last + 1 < children.len()
        {
            last += 1;
        }
        let end = children[last].span.end().get();

        // Comments before this item's start are the formatter's `leading`
        // group: they are emitted above the item and leave it laid out.
        while cursor < comment_starts.len() && comment_starts[cursor] < start {
            cursor += 1;
        }
        let mut verbatim = last != index;
        while cursor < comment_starts.len() && comment_starts[cursor] < end {
            verbatim = true;
            cursor += 1;
        }
        if verbatim {
            regions.push((start, end));
        }

        index = last + 1;
    }

    regions.sort_unstable();
    regions
}

/// Every line of `formatted` that starts at or left of the column of the
/// opening delimiter that encloses it.
///
/// `tree` must be the parse of `formatted`, not of the original source.
///
/// Three kinds of line are exempt, each because the formatter did not choose
/// its indentation:
///
/// * A line whose first character closes a list. `))` belongs to the form it
///   closes, and every convention in the family puts it at the column of that
///   form's *parent*.
/// * A line that begins inside an atom or a comment, i.e. the second and later
///   lines of a multi-line string or block comment. Their indentation is
///   content.
/// * A line inside a top-level form the formatter reproduced verbatim; see
///   [`verbatim_regions`].
fn column_violations(path: &Path, formatted: &str, tree: &SyntaxTree) -> ColumnScan {
    let layout = Layout::of(tree);
    let verbatim = verbatim_regions(tree);
    let mut scan = ColumnScan::default();

    let mut open: Vec<Region> = Vec::new();
    let mut next_list = 0usize;
    let mut next_opaque = 0usize;
    let mut last_opaque: Option<Region> = None;
    let mut next_verbatim = 0usize;
    let mut last_verbatim: Option<Region> = None;

    for (line_start, line) in line_offsets(formatted) {
        let Some(indent_bytes) = line.find(|character: char| !character.is_whitespace()) else {
            continue;
        };
        let first = line_start + indent_bytes;

        // Advance the opaque sweep to this line. The regions are disjoint and
        // sorted, so only the last one that started before `first` can still
        // be open across it.
        while let Some(region) = layout.opaque.get(next_opaque) {
            if region.0 >= first {
                break;
            }
            last_opaque = Some(*region);
            next_opaque += 1;
        }
        if last_opaque.is_some_and(|(_, end)| end > first) {
            continue;
        }

        // The same sweep over verbatim top-level forms, which are disjoint and
        // sorted for the same reason.
        while let Some(region) = verbatim.get(next_verbatim) {
            if region.0 >= first {
                break;
            }
            last_verbatim = Some(*region);
            next_verbatim += 1;
        }
        if last_verbatim.is_some_and(|(_, end)| end > first) {
            scan.verbatim += 1;
            continue;
        }

        // Advance the list sweep, closing each list as the next one that
        // starts after it opens. Proper nesting makes this exact.
        while let Some(region) = layout.lists.get(next_list) {
            if region.0 >= first {
                break;
            }
            while open.last().is_some_and(|(_, end)| *end <= region.0) {
                open.pop();
            }
            open.push(*region);
            next_list += 1;
        }
        while open.last().is_some_and(|(_, end)| *end <= first) {
            open.pop();
        }

        let Some((enclosing_start, _)) = open.last().copied() else {
            continue;
        };
        if line[indent_bytes..].starts_with([')', ']', '}']) {
            continue;
        }

        scan.checked += 1;
        let indent = display_width(&line[..indent_bytes]);
        let enclosing_column = column_of(formatted, enclosing_start);
        if indent > enclosing_column {
            continue;
        }
        if scan.violations.len() < MAX_COLUMN_VIOLATIONS_PER_FILE {
            scan.violations.push(format!(
                "{}: line {:?} starts at column {indent}, at or left of the delimiter \
                 that encloses it at column {enclosing_column}",
                path.display(),
                line.trim_end(),
            ));
        }
    }
    scan
}

/// What one file's column scan looked at, and what it found.
///
/// `checked` is reported by the run rather than merely asserted on, because a
/// check that examines nothing passes exactly as loudly as one that examines
/// everything. A corpus run that prints a plausible line count has done work;
/// one that prints zero has silently stopped being a test.
#[derive(Default)]
struct ColumnScan {
    checked: usize,
    /// Lines skipped because the formatter copied them out of the input rather
    /// than laying them out. Reported next to `checked` so the size of the
    /// blind spot is visible instead of inferred.
    verbatim: usize,
    violations: Vec<String>,
}

/// Each line of `text` with the byte offset it starts at, newline excluded.
fn line_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0;
    text.split_inclusive('\n').map(move |line| {
        let offset = start;
        start += line.len();
        (offset, line.trim_end_matches(['\n', '\r']))
    })
}

/// The display column `offset` sits at, counting from zero.
///
/// Display width rather than bytes or `char`s: the formatter lines a
/// continuation up under a column it measured the same way, and a line
/// carrying `束縛` is four columns wide and six bytes long. Measuring in bytes
/// here would invent violations in every CJK-bearing file in a corpus.
fn column_of(text: &str, offset: usize) -> usize {
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    display_width(&text[line_start..offset])
}

#[derive(Default)]
struct Tally {
    files: usize,
    parsed: usize,
    parse_errors: usize,
    skipped_non_utf8: usize,
    skipped_large: usize,
    /// Lines of formatted output that invariant 5 actually compared against an
    /// enclosing delimiter, i.e. excluding blank lines, top-level lines, and
    /// the exempt ones.
    enclosed_lines: usize,
    /// Lines invariant 5 could not speak for because the formatter reproduced
    /// their top-level form verbatim.
    verbatim_lines: usize,
}

/// Checks the five invariants on one file, returning failures rather than
/// panicking so one run reports every offender.
fn check_file(path: &Path, tally: &mut Tally) -> Vec<String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if metadata.len() > MAX_CORPUS_FILE_BYTES {
        tally.skipped_large += 1;
        return Vec::new();
    }
    let Ok(source) = fs::read(path) else {
        return Vec::new();
    };
    let Ok(source) = String::from_utf8(source) else {
        // Legacy Lisp sources are routinely Shift_JIS or Latin-1. Reading one
        // as UTF-8 is not this test's subject.
        tally.skipped_non_utf8 += 1;
        return Vec::new();
    };

    tally.files += 1;
    let dialect = Dialect::detect_in_source(Some(path), None, &source);

    // Invariant 1: parsing terminates. A panic here fails the test through the
    // harness rather than through an assertion, which is the correct outcome.
    let Ok(tree) = SyntaxTree::parse_with_dialect(&source, dialect) else {
        tally.parse_errors += 1;
        return Vec::new();
    };
    tally.parsed += 1;

    let mut failures = Vec::new();

    // Invariant 2: the parse is lossless. Every rewrite in this tool is a span
    // replacement over exactly this string.
    if tree.source() != source {
        failures.push(format!(
            "{}: the parsed source is not the input ({} bytes in, {} bytes retained)",
            path.display(),
            source.len(),
            tree.source().len()
        ));
    }

    // Invariant 3: formatting converges.
    let formatter = Formatter::with_dialect(2, dialect);
    let once = formatter.format(&tree);
    match SyntaxTree::parse_with_dialect(&once, dialect) {
        Ok(reparsed) => {
            let twice = formatter.format(&reparsed);
            if once != twice {
                failures.push(format!(
                    "{}: formatting is not idempotent; a second pass changed {} bytes",
                    path.display(),
                    once.len().abs_diff(twice.len()),
                ));
            }

            // Invariant 5: every line sits inside the form that encloses it.
            // Checked on the formatted text, not on the input: the input's
            // layout is whoever wrote it, and this is an assertion about what
            // the formatter produces.
            let scan = column_violations(path, &once, &reparsed);
            tally.enclosed_lines += scan.checked;
            tally.verbatim_lines += scan.verbatim;
            failures.extend(scan.violations);
        }
        Err(error) => failures.push(format!(
            "{}: formatted output does not reparse: {error}",
            path.display()
        )),
    }

    // Invariant 4: a path the tree produced resolves.
    for index in 0..tree.root_children().len() {
        let path_to_form = ExpressionPath::from_indexes(vec![index]);
        if tree.select_path(&path_to_form).is_err() {
            failures.push(format!(
                "{}: top-level path {index} is reported by the tree and rejected by the selector",
                path.display()
            ));
        }
    }

    failures
}

#[test]
fn every_corpus_file_upholds_the_parser_and_formatter_invariants() {
    let mut budget = MAX_CORPUS_FILES;
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let mut roots_seen = Vec::new();

    for root in corpus_roots() {
        if !root.is_dir() {
            // A missing `PAREDIT_CORPUS_DIR` entry is the caller's typo and
            // worth saying so; a missing vendored corpus is a broken checkout.
            assert!(
                root != Path::new("tests/fixtures/corpus"),
                "the vendored corpus at tests/fixtures/corpus is missing"
            );
            println!("corpus root {} does not exist; skipping", root.display());
            continue;
        }
        roots_seen.push(root.display().to_string());
        for file in corpus_files(&root, &mut budget) {
            failures.extend(check_file(&file, &mut tally));
        }
    }

    println!(
        "corpus roots: {}\n  files read      {}\n  parsed          {}\n  parse errors    {}\n  \
         skipped non-UTF-8 {}\n  skipped large   {}\n  enclosed lines  {}\n  \
         verbatim lines  {}",
        roots_seen.join(", "),
        tally.files,
        tally.parsed,
        tally.parse_errors,
        tally.skipped_non_utf8,
        tally.skipped_large,
        tally.enclosed_lines,
        tally.verbatim_lines,
    );
    if budget == 0 {
        println!(
            "  NOTE: stopped at the {MAX_CORPUS_FILES}-file ceiling; \
             this run did not cover the whole corpus"
        );
    }

    assert!(
        tally.files > 0,
        "the corpus produced no readable files; the vendored fixtures are missing or empty"
    );
    assert!(
        failures.is_empty(),
        "{} corpus invariant failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The vendored corpus exists to be awkward. A run in which most of it fails to
/// parse would mean the fixtures had drifted into testing the error path.
#[test]
fn the_vendored_corpus_is_mostly_parseable() {
    let root = PathBuf::from("tests/fixtures/corpus");
    let mut budget = MAX_CORPUS_FILES;
    let mut tally = Tally::default();

    for file in corpus_files(&root, &mut budget) {
        let _ = check_file(&file, &mut tally);
    }

    assert!(
        tally.files >= 3,
        "the vendored corpus is too small to mean anything"
    );
    assert_eq!(
        tally.parse_errors, 0,
        "every vendored corpus file is expected to parse; \
         a file that should not parse belongs in a parser test, not here"
    );
}

/// Checks one hand-written document against invariant 5, for the controls
/// below. Not used by the corpus walk, which always has a formatted string and
/// its reparse already in hand.
fn violations_of(source: &str, dialect: Dialect) -> Vec<String> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("the controls all parse");
    column_violations(Path::new("<control>"), source, &tree).violations
}

/// The column check fires on a document that breaks it.
///
/// The corpus reports zero violations, and a check that reports zero because
/// it never looks is indistinguishable from one that reports zero because the
/// formatter is correct. This is the difference: the same function, over text
/// the formatter would never emit.
#[test]
fn the_column_check_catches_an_outdented_line() {
    let violations = violations_of("(defun f ()\n(list 1\n2))\n", Dialect::CommonLisp);
    assert_eq!(
        violations.len(),
        2,
        "expected both outdented lines to be reported, got: {violations:#?}"
    );
    assert!(
        violations[0].contains("(list 1"),
        "the first violation should name the line it is about: {violations:#?}"
    );
}

/// A character literal that contains a delimiter is not a delimiter.
///
/// This is the trap that makes a hand-written delimiter scanner unusable here.
/// `(list #\()` is balanced, but a scanner that counts raw parentheses sees
/// two opens and one close, believes a list is still open at column 0 for the
/// rest of the file, and reports every subsequent top-level form as outdented.
/// Reading the extents off the parse tree cannot make that mistake, in any of
/// the five spellings this covers.
#[test]
fn delimiters_inside_tokens_are_not_delimiters() {
    for (source, dialect) in [
        (
            "(list #\\( #\\))\n(defun g ()\n  :ok)\n",
            Dialect::CommonLisp,
        ),
        ("(list #\\()\n(defun g ()\n  :ok)\n", Dialect::CommonLisp),
        (
            "(list ?\\( ?\\))\n(defun g ()\n  :ok)\n",
            Dialect::EmacsLisp,
        ),
        ("(list \\( \\))\n(defn g []\n  :ok)\n", Dialect::Clojure),
        (
            "(list \"(\" \")\")\n(defun g ()\n  :ok)\n",
            Dialect::CommonLisp,
        ),
        (
            "(list |a (b| |c)d|)\n(defun g ()\n  :ok)\n",
            Dialect::CommonLisp,
        ),
        (
            "#| a ( comment |#\n(defun g ()\n  :ok)\n",
            Dialect::CommonLisp,
        ),
        ("(list #\\( ; a ( comment\n      1)\n", Dialect::CommonLisp),
    ] {
        let violations = violations_of(source, dialect);
        assert!(
            violations.is_empty(),
            "{dialect:?} reported a violation on balanced input: {violations:#?}\n{source}"
        );
    }
}

/// The exemption for lines that begin inside a multi-line token covers exactly
/// those lines, and does not swallow the outdented code after one.
#[test]
fn a_multi_line_token_exempts_only_its_own_continuation_lines() {
    let source = "(defun f ()\n  (format nil \"a\nb\")\n(oops))\n";
    let violations = violations_of(source, Dialect::CommonLisp);
    assert_eq!(
        violations.len(),
        1,
        "the string's own second line is content and the `(oops)` after it is not: \
         {violations:#?}"
    );
    assert!(violations[0].contains("(oops)"), "{violations:#?}");
}

/// One comment anywhere inside a top-level form makes the formatter reproduce
/// that entire form verbatim.
///
/// Pinned here because invariant 5 exempts those lines, and an exemption whose
/// premise is not checked is an exemption that quietly grows. The sibling
/// `(other-call ...)` in this input carries no comment of its own and is still
/// left exactly as written, which is what makes the exempt region the *whole*
/// top-level form rather than the comment's immediate parent.
///
/// This is also the reason a real-world corpus run reports far more verbatim
/// lines than checked ones: most real Lisp comments its function bodies.
#[test]
fn a_comment_anywhere_makes_a_whole_top_level_form_verbatim() {
    let source = "(defun f (a b)\n(list a\n\n;; note\nb)\n(other-call a b c d e))\n";
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::EmacsLisp).expect("valid");
    assert_eq!(
        Formatter::with_dialect(2, Dialect::EmacsLisp).format(&tree),
        source,
        "the exemption in `verbatim_regions` assumes this; if the formatter has \
         started laying comment-bearing forms out, narrow or drop it"
    );

    let without_comment = "(defun f (a b)\n(list a\nb)\n(other-call a b c d e))\n";
    let tree = SyntaxTree::parse_with_dialect(without_comment, Dialect::EmacsLisp).expect("valid");
    assert_ne!(
        Formatter::with_dialect(2, Dialect::EmacsLisp).format(&tree),
        without_comment,
        "the same form without the comment must be laid out, or the exemption \
         would be indistinguishable from the formatter never reindenting"
    );
}

/// A Clojure metadata prefix merges its form into one top-level item, and the
/// merged item renders verbatim — with or without a comment in it.
///
/// This is the second half of the formatter's verbatim rule and the half a
/// comment-only model misses entirely. `^{:doc ...}` parses as a root child of
/// its own carrying a `Metadata` reader prefix, and `plan_top_level` merges it
/// with the root child that follows; `verbatim` is then set from
/// `node_id != end_node_id` *before* any comment is looked at, so the pair is
/// copied out of the source even in a file with no comments at all.
///
/// The three cases below are what a comment-only exemption gets wrong, in
/// order: with no comment it exempts nothing and reports both copied lines;
/// with a comment inside the *second* member it exempts only that member and
/// still reports the first one's line. The last case is the control that keeps
/// the exemption from being indistinguishable from the formatter never
/// reindenting Clojure — drop the `^` and both forms are laid out.
#[test]
fn a_metadata_prefix_chain_is_verbatim_as_one_merged_item() {
    let formatter = Formatter::with_dialect(2, Dialect::Clojure);

    for source in [
        "^{:doc\n\"x\"}\n(defn f [a]\n(inc a))\n",
        "^{:doc\n\"x\"}\n(defn f [a]\n;; note\n(inc a))\n",
    ] {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("valid");
        assert_eq!(
            formatter.format(&tree),
            source,
            "the exemption in `verbatim_regions` assumes this; if the formatter has \
             started laying metadata chains out, narrow or drop it"
        );
        assert!(
            violations_of(source, Dialect::Clojure).is_empty(),
            "every line of a merged metadata chain is the author's, not the formatter's: {:#?}",
            violations_of(source, Dialect::Clojure)
        );
    }

    let unprefixed = "{:doc\n\"x\"}\n(defn f [a]\n(inc a))\n";
    let tree = SyntaxTree::parse_with_dialect(unprefixed, Dialect::Clojure).expect("valid");
    assert_ne!(
        formatter.format(&tree),
        unprefixed,
        "the same two forms without the `^` must be laid out, or the exemption \
         would be indistinguishable from the formatter never reindenting"
    );
}

/// The vendored corpus carries idiomatic code for every dialect this tool
/// reads.
///
/// The invariants above are only as broad as the shapes they run over, and no
/// two of these ten languages lay a definition, a binding list or a
/// conditional out the same way: Clojure puts its parameters in a vector,
/// Racket brackets its `cond` arms, Janet and Fennel spell their conditionals
/// as flat pairs, LFE nests a clause list per arity. A corpus missing a
/// dialect is a corpus that says nothing about that dialect's layout, and
/// nothing in the suite would have said so.
///
/// `Dialect::Unknown` is excluded on purpose. It is the reader for a file with
/// no recognised extension, so there is no filename a fixture could carry that
/// would reach it; `tests/parser_robustness.rs` covers it through
/// `Dialect::ALL` instead.
#[test]
fn the_vendored_corpus_covers_every_dialect() {
    let mut budget = MAX_CORPUS_FILES;
    let covered: Vec<Dialect> = corpus_files(Path::new("tests/fixtures/corpus"), &mut budget)
        .iter()
        .filter_map(|file| file.extension().and_then(|extension| extension.to_str()))
        .map(Dialect::from_extension)
        .collect();

    let missing: Vec<&str> = Dialect::ALL
        .into_iter()
        .filter(|dialect| *dialect != Dialect::Unknown)
        .filter(|dialect| !covered.contains(dialect))
        .map(Dialect::label)
        .collect();

    assert!(
        missing.is_empty(),
        "the vendored corpus has no fixture for: {}. \
         Add one under tests/fixtures/corpus with that dialect's extension.",
        missing.join(", ")
    );
}
