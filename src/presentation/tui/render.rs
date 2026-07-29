//! Building the screen `paredit tui` draws for the current path.
//!
//! A pure `(tree, path, width) -> String` function, kept apart from
//! `raw_mode`'s actual terminal I/O for the same reason `navigate` is: a
//! frame's content is testable without a pty, and the loop in `super` is
//! left with nothing more than "compute a frame, clear the screen, print it".

use std::fmt::Write as _;

use paredit_core_syntax::sexpr::{ExpressionPath, ExpressionView, SyntaxTree};

use super::navigate;

/// How much of a node's own source a preview line shows before clipping,
/// separate from [`paredit_core_cli::terminal::width`] because a preview
/// line also carries an index prefix (`"  [3] "`) that eats into the same
/// budget; this stays a fixed, conservative width rather than
/// `terminal_width - prefix_len` so a very narrow terminal cannot drive it
/// negative.
const PREVIEW_CHARS: usize = 72;

const HELP_LINE: &str =
    "j/\u{2193}=next  k/\u{2191}=prev  l/\u{2192}/enter=in  h/\u{2190}=out  q=quit";

/// The full frame for `path`'s current position in `tree`, clipped to fit a
/// terminal `terminal_width` columns wide.
///
/// # Panics
///
/// Never, in normal use: `path` is only ever a value this module or
/// `navigate` produced, and both keep it pointing at a view that exists.
pub(super) fn frame(tree: &SyntaxTree, path: &ExpressionPath, terminal_width: usize) -> String {
    let view =
        navigate::view_at(tree, path).expect("paredit tui never advances to a path with no view");
    let mut out = String::new();

    let path_label = if path.to_raw_indexes().is_empty() {
        "(root)".to_owned()
    } else {
        path.to_string()
    };
    writeln!(out, "path: {path_label}   kind: {:?}", view.kind).expect("String writes cannot fail");
    writeln!(
        out,
        "text: {}",
        clip(
            &single_line(node_source(tree, &view)),
            terminal_width.saturating_sub("text: ".len())
        )
    )
    .expect("String writes cannot fail");
    writeln!(out).expect("String writes cannot fail");

    writeln!(out, "children ({}):", view.children.len()).expect("String writes cannot fail");
    for (index, child) in view.children.iter().enumerate() {
        let prefix = format!("  [{index}] ");
        let budget = terminal_width
            .saturating_sub(prefix.chars().count())
            .min(PREVIEW_CHARS);
        writeln!(
            out,
            "{prefix}{}",
            clip(&single_line(node_source(tree, child)), budget)
        )
        .expect("String writes cannot fail");
    }

    writeln!(out).expect("String writes cannot fail");
    out.push_str(HELP_LINE);
    out
}

fn node_source<'a>(tree: &'a SyntaxTree, view: &ExpressionView) -> &'a str {
    &tree.source()[view.span.as_range()]
}

/// Collapses interior whitespace — including the newlines a multi-line form
/// like a `defun` body always has — to single spaces, so one node's preview
/// never spans more than the one screen line it is drawn on.
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars || max_chars < 2 {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(max_chars - 1).collect();
    kept.push('\u{2026}');
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_collapses_newlines_and_runs_of_whitespace() {
        assert_eq!(
            single_line("(defun f (x)\n   (+ x  1))"),
            "(defun f (x) (+ x 1))"
        );
    }

    #[test]
    fn clip_leaves_a_short_line_alone() {
        assert_eq!(clip("short", 80), "short");
    }

    #[test]
    fn clip_marks_a_cut_line_with_an_ellipsis_and_holds_the_budget() {
        let clipped = clip("this is a longer line than the budget allows", 10);
        assert_eq!(clipped.chars().count(), 10);
        assert!(clipped.ends_with('\u{2026}'));
    }

    #[test]
    fn frame_names_the_root_and_lists_every_top_level_form() {
        let tree = SyntaxTree::parse("(a)\n(b 1)\n").expect("fixture parses");
        let path = ExpressionPath::from_indexes(Vec::new());
        let rendered = frame(&tree, &path, 80);

        assert!(rendered.contains("path: (root)"));
        assert!(rendered.contains("children (2):"));
        assert!(rendered.contains("[0] (a)"));
        assert!(rendered.contains("[1] (b 1)"));
        assert!(rendered.contains("q=quit"));
    }

    #[test]
    fn frame_at_a_form_shows_its_own_text_and_its_children() {
        let tree = SyntaxTree::parse("(a)\n(b 1 2)\n").expect("fixture parses");
        let path = ExpressionPath::from_indexes(vec![1]);
        let rendered = frame(&tree, &path, 80);

        assert!(rendered.contains("path: 1"));
        assert!(rendered.contains("text: (b 1 2)"));
        assert!(rendered.contains("children (3):"));
    }

    #[test]
    fn frame_clips_a_preview_to_the_terminal_width() {
        let source = format!("({})\n", "x ".repeat(100).trim());
        let tree = SyntaxTree::parse(&source).expect("fixture parses");
        let path = ExpressionPath::from_indexes(Vec::new());
        let rendered = frame(&tree, &path, 20);

        // Only the lines this module actually clips — the `text:` line and
        // each child preview — are bound by the requested width; the header
        // lines around them (`path: ...`, `children (N):`) are metadata, not
        // reflowed source, and are short regardless.
        let clipped_lines = rendered
            .lines()
            .filter(|line| line.starts_with("text: ") || line.trim_start().starts_with('['));
        for line in clipped_lines {
            assert!(line.chars().count() <= 20, "{line:?} exceeds width 20");
        }
        assert!(rendered.contains('\u{2026}'));
    }
}
