//! docs/src/project/feature-candidates.md section L: `--output text` is flat
//! and linear — tab-separated rows, no headings, no boxes, no cursor control —
//! and that shape happens to be exactly what a screen reader needs. A sighted
//! reader scans a table by its borders and alignment; a screen reader has no
//! such shortcut and instead reads whatever bytes are on the line, in order.
//! Box-drawing characters and cursor-control escapes are two ways that shape
//! quietly breaks: the first reads out loud as a run of "box drawings light
//! vertical" or is skipped as noise depending on the reader's settings, and
//! the second can reposition or hide text the reader never gets to announce.
//!
//! Nothing here is new behavior. Every report renderer already builds its rows
//! from `println!("{col}\t{col}\t...")`, and the only escape any of them emits
//! is the SGR color `color_palette_contract.rs` already governs — this is
//! accidental, not designed in, which is exactly why nothing previously locked
//! it in. The property is worth pinning anyway: a future renderer reaching for
//! a "nicer" box-drawn summary table, or a progress spinner that moves the
//! cursor, would regress this silently, since neither breaks a single
//! `assert_cmd` test that only checks substrings. This test reads the
//! renderer sources as text — the same style as `color_palette_contract.rs`
//! and `report_json_key_casing_contract.rs` — so the check runs in the Nix
//! sandbox without executing any command, and so it also covers a report
//! whose fixture happens to produce no findings today.
//!
//! # The documented exception: `paredit tui`
//!
//! `src/presentation/tui/mod.rs` is deliberately excluded and must stay that
//! way. It implements `paredit tui`, an interactive terminal browser, and its
//! frame loop does `write!(stdout, "\x1b[2J\x1b[H{frame}")` — clear screen,
//! home the cursor — every redraw. That is not an oversight: an interactive
//! full-screen UI is a different contract than a report a script can pipe
//! through `grep`/`cut`, and repainting a screen in place is the only way it
//! can work at all. Making `paredit tui` itself screen-reader accessible is
//! its own, separate concern — tracked as H7 in
//! docs/src/project/feature-candidates.md — and is explicitly out of scope
//! here. This test does not read that file, and must never be changed to.
//!
//! # Scope
//!
//! The two renderers the shared report envelope funnels most `inspect`
//! command text output through:
//!
//! * `packages/core/cli/src/report/render.rs` — `print_report`'s `print_text`,
//!   shared by every report built on `Finding`/`FileFindings`. Of the 224
//!   `render.rs`/`text.rs` files under `packages/feature/` at the time of
//!   writing, 166 call `print_report` and so are covered by scanning this one
//!   file; the other 58 hand-roll their own `println!` rows and predate, or
//!   sit beside, the shared envelope — those are not covered by this bullet,
//!   see below.
//! * `src/presentation/cli/lint_report/render.rs` — the aggregated `inspect
//!   lint` report, which predates the shared envelope and prints its own rows.
//!
//! Plus every other `render.rs`/`render/text.rs` under `src/presentation/cli/`
//! that also predates or sits beside the shared envelope and still owns its
//! own `println!` text formatting rather than delegating to it entirely:
//!
//! * `src/presentation/cli/analysis_report/render.rs` (`inspect stats`/
//!   `outline`/`agent-report`/`dialect`)
//! * `src/presentation/cli/config/render.rs` (`config check`/`show`/`schema`/
//!   `init`)
//! * `src/presentation/cli/dependency_report/render.rs` (`inspect
//!   dependencies`)
//! * `src/presentation/cli/emacs_lisp_file_report/render.rs` (`inspect
//!   emacs-lisp-file`)
//! * `src/presentation/cli/semantic_coverage_report/render/text.rs` (`inspect
//!   semantic-coverage`)
//!
//! That is every `render.rs`/`render/text.rs` that exists under
//! `src/presentation/cli/` at the time of writing (verified in
//! `the_scanned_files_are_exactly_the_known_render_sources` below); a renderer
//! added later under that tree is not automatically covered and should be
//! added to [`REPORT_RENDER_SOURCES`] alongside it.
//!
//! ## The 58 hand-rolled `packages/feature/` renderers
//!
//! Unlike the six files above, this set is too large and too fast-growing to
//! hand-list and keep honest (it is nearly ten times the presentation-layer
//! list, in packages this project adds routinely). [`feature_renderer_sources`]
//! discovers it instead: walk every `render.rs`/`text.rs` under
//! `packages/feature/`, and keep the ones that do *not* contain the literal
//! `print_report` — those are the ones not already covered by scanning
//! `packages/core/cli/src/report/render.rs` once. A renderer added later that
//! calls `print_report` needs nothing added here; one that hand-rolls its own
//! rows is picked up automatically on the next test run, no list to update.

use std::fs;

/// Every report-text renderer this contract reads. See the module doc for why
/// each one is here.
const REPORT_RENDER_SOURCES: &[&str] = &[
    "packages/core/cli/src/report/render.rs",
    "src/presentation/cli/lint_report/render.rs",
    "src/presentation/cli/analysis_report/render.rs",
    "src/presentation/cli/config/render.rs",
    "src/presentation/cli/dependency_report/render.rs",
    "src/presentation/cli/emacs_lisp_file_report/render.rs",
    "src/presentation/cli/semantic_coverage_report/render/text.rs",
];

/// The documented, intentional exception: an interactive full-screen UI, not
/// a report. Named so a reader auditing this file sees exactly what is
/// deliberately left out and why, rather than having to wonder whether it was
/// forgotten.
const TUI_EXCEPTION: &str = "src/presentation/tui/mod.rs";

/// Every `render.rs`/`text.rs` under `packages/feature/` that does not call
/// `print_report` — i.e. the hand-rolled renderers not already covered by
/// scanning `packages/core/cli/src/report/render.rs` once. See the module doc
/// for why this is discovered by walking the tree rather than hand-listed.
fn feature_renderer_sources() -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![std::path::PathBuf::from("packages/feature")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name == "render.rs" || name == "text.rs"
            }) {
                let path_string = path.to_string_lossy().replace('\\', "/");
                let source = read(&path_string);
                if !source.contains("print_report") {
                    found.push(path_string);
                }
            }
        }
    }
    found.sort();
    found
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("read {path}"))
}

/// The Unicode box-drawing block, U+2500 through U+257F: light/heavy/double
/// lines, corners, and junctions used to hand-draw tables and borders. Written
/// as a codepoint range rather than pasting the glyphs, so this file's own
/// source never carries the very characters it exists to forbid elsewhere.
fn box_drawing_chars(source: &str) -> Vec<char> {
    source
        .chars()
        .filter(|&character| ('\u{2500}'..='\u{257F}').contains(&character))
        .collect()
}

/// Every ANSI CSI escape spelled out in `source`, as `\x1b[<params><final>`,
/// paired with its final byte.
///
/// Matched on the literal source text `\x1b[` — four ASCII characters,
/// backslash/x/1/b, followed by `[` — rather than on the control byte itself.
/// A Rust source file spells the escape that way; the runtime control
/// character only exists once the string literal is compiled. Building the
/// marker via `"\\x1b["` (an escaped backslash) rather than writing the escape
/// directly keeps this test file itself free of the real ESC byte, so a
/// scan that also covered this file would find nothing to flag.
fn csi_escapes(source: &str) -> Vec<(String, char)> {
    let marker = "\\x1b[";
    let mut found = Vec::new();
    for (index, _) in source.match_indices(marker) {
        let rest = &source[index + marker.len()..];
        let params_end = rest
            .find(|character: char| !(character.is_ascii_digit() || character == ';'))
            .unwrap_or(rest.len());
        let Some(terminator) = rest[params_end..].chars().next() else {
            continue;
        };
        if terminator.is_ascii_alphabetic() {
            found.push((rest[..params_end].to_owned(), terminator));
        }
    }
    found
}

/// CSI escapes whose final byte is not `m` — i.e. not an SGR color/style
/// escape, but a cursor-movement, screen-clear, or other terminal-control
/// sequence a screen reader would have no useful way to announce.
fn non_sgr_escapes(source: &str) -> Vec<(String, char)> {
    csi_escapes(source)
        .into_iter()
        .filter(|(_, terminator)| *terminator != 'm')
        .collect()
}

/// [`REPORT_RENDER_SOURCES`] plus every dynamically-discovered
/// `packages/feature/` renderer — the full set both scanning tests check.
fn all_report_render_sources() -> Vec<String> {
    let mut all: Vec<String> = REPORT_RENDER_SOURCES
        .iter()
        .map(|&path| path.to_owned())
        .collect();
    all.extend(feature_renderer_sources());
    all
}

#[test]
fn no_report_renderer_spells_out_a_box_drawing_character() {
    for path in all_report_render_sources() {
        let source = read(&path);
        let found = box_drawing_chars(&source);
        assert!(
            found.is_empty(),
            "{path} contains Unicode box-drawing character(s) {found:?}; \
             `--output text` is read by screen readers a line at a time and \
             must stay a flat, tab-separated table with no hand-drawn borders. \
             `paredit tui` is the one place a boxed layout belongs, and it is \
             a separate, out-of-scope command (see this file's module doc)."
        );
    }
}

#[test]
fn no_report_renderer_spells_out_a_non_sgr_csi_escape() {
    for path in all_report_render_sources() {
        let source = read(&path);
        let found = non_sgr_escapes(&source);
        assert!(
            found.is_empty(),
            "{path} spells out a CSI escape ending in {found:?}, which is not \
             the SGR form (`m`) `packages/core/cli/src/color.rs`'s `Painter` \
             already legitimately emits. A cursor-movement or screen-clear \
             escape in a report renderer would move or hide text a screen \
             reader has no way to recover; that belongs only in `paredit \
             tui` (see this file's module doc for why that command is exempt)."
        );
    }
}

/// A discovery walk that silently found zero files would make both scans
/// above pass vacuously, exactly the failure mode this whole file exists to
/// avoid. Pinned to a range rather than an exact count so an unrelated
/// package added or removed under `packages/feature/` does not fail this
/// test on every unrelated PR — 58 hand-rolled renderers existed at the time
/// of writing, out of 224 total `render.rs`/`text.rs` files there.
#[test]
fn feature_renderer_discovery_finds_a_plausible_number_of_files() {
    let found = feature_renderer_sources();
    assert!(
        found.len() > 20,
        "only found {} hand-rolled packages/feature renderers; the walk in \
         `feature_renderer_sources` may be reading the wrong directory or \
         its `print_report` filter may be over-broad",
        found.len()
    );
}

/// A scan that quietly stopped reading its files would pass both checks
/// above having found nothing at all, so the file list itself is checked to
/// still point at real, non-empty files.
#[test]
fn every_scanned_source_is_a_real_nonempty_file() {
    for path in REPORT_RENDER_SOURCES {
        let source = read(path);
        assert!(
            !source.trim().is_empty(),
            "{path} is empty; the scan is reading the wrong file"
        );
    }
}

/// [`REPORT_RENDER_SOURCES`] is a hand-picked list, so it can silently miss a
/// renderer added later. This pins today's actual set of `render.rs`/
/// `render/text.rs` files under `src/presentation/cli/` (plus the shared
/// envelope printer in `packages/core/cli`) so that set growing is a visible,
/// deliberate decision — add the new file to the list and this test — rather
/// than a file this contract never looked at.
#[test]
fn the_scanned_files_are_exactly_the_known_render_sources() {
    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from("src/presentation/cli")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name == "render.rs" || name == "text.rs"
            }) {
                found.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();

    let mut expected: Vec<&str> = REPORT_RENDER_SOURCES
        .iter()
        .copied()
        .filter(|path| path.starts_with("src/presentation/cli"))
        .collect();
    expected.sort_unstable();

    assert_eq!(
        found, expected,
        "src/presentation/cli now has a different set of render.rs/text.rs \
         files than this contract scans. Add or remove the file in \
         REPORT_RENDER_SOURCES (and, if it is a new report renderer, read it \
         for box-drawing/cursor-control escapes before doing so)."
    );
}

/// The one place this property is deliberately *not* true, so the exception
/// stays real rather than a stale claim in a doc comment: `paredit tui`
/// really does use cursor-control escapes, and this contract really does
/// leave it unscanned.
#[test]
fn the_tui_exception_is_real_and_left_unscanned() {
    assert!(
        !REPORT_RENDER_SOURCES.contains(&TUI_EXCEPTION),
        "{TUI_EXCEPTION} must stay out of REPORT_RENDER_SOURCES; it is an \
         interactive full-screen UI (H7), not a report"
    );
    let source = read(TUI_EXCEPTION);
    assert!(
        !non_sgr_escapes(&source).is_empty(),
        "{TUI_EXCEPTION} no longer spells out a non-SGR CSI escape; if the \
         cursor-control redraw was removed, this file's module doc should be \
         updated to stop citing it as the reason the command is exempt"
    );
}

#[test]
fn box_drawing_chars_finds_a_planted_character_and_ignores_plain_ascii() {
    let clean = "kind\tpath\tline\tsome message with -> an arrow, not a box";
    assert!(box_drawing_chars(clean).is_empty());

    let planted = format!("left{}right", '\u{2502}');
    assert_eq!(box_drawing_chars(&planted), vec!['\u{2502}']);
}

#[test]
fn non_sgr_escapes_finds_a_cursor_escape_and_allows_an_sgr_one() {
    let sgr_only = "a\\x1b[31mb\\x1b[0m";
    assert!(non_sgr_escapes(sgr_only).is_empty());

    let cursor_move = "clear the screen: \\x1b[2J then home: \\x1b[H";
    let found = non_sgr_escapes(cursor_move);
    assert_eq!(found, vec![("2".to_owned(), 'J'), (String::new(), 'H')]);
}

#[test]
fn csi_escapes_ignores_a_backslash_x1b_bracket_with_no_letter_after_it() {
    // Not a real escape: nothing terminates it, so nothing should be reported
    // rather than the scan panicking on an out-of-range index.
    let dangling = "\\x1b[";
    assert!(csi_escapes(dangling).is_empty());
    assert!(non_sgr_escapes(dangling).is_empty());
}
