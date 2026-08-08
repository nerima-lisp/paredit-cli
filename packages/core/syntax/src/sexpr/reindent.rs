//! Re-indentation that keeps the author's line breaks.
//!
//! This is deliberately not `edit format`. The formatter decides *where the
//! line breaks go*; this decides only *how far each existing line is pushed
//! in*, which is what `C-M-q` does in Emacs and what `paredit-newline` needs
//! after it inserts a break. Running the formatter instead would rewrap the
//! whole definition, so a one-character insertion would arrive in review as a
//! twenty-line diff.
//!
//! The convention implemented here is `lisp-indent-function`'s, and it is one
//! rule plus a table:
//!
//! - A form whose head is a **body form** — `defun`, `let`, `when`, … —
//!   indents its body one unit past the form's own column, and the
//!   distinguished arguments that precede the body two units past it.
//! - Any other form aligns its arguments under its **first argument** when
//!   that argument shares the head's line, and one column past the open
//!   delimiter when it does not.
//!
//! [`crate::sexpr::body_form_distinguished`] is the table, published so the
//! `inspect indentation` report measures deviation from the same convention
//! this command produces. Two copies of it would be two conventions.

use super::navigation::ContextKind;
use super::tree::{NodeKind, SyntaxTree};
use super::types::{ByteOffset, ByteSpan, NodeId};

/// Heads whose body Emacs indents by one unit, paired with how many
/// distinguished arguments precede that body.
///
/// The count is what `lisp-indent-function` calls the form's "number of
/// distinguished arguments": `defun` has two (name and lambda list), `when` has
/// one (the test), `progn` has none.
const BODY_FORMS: [(&str, usize); 70] = [
    // ── Definition forms ──
    ("defun", 2),
    ("defmacro", 2),
    ("defmethod", 2),
    ("defgeneric", 2),
    ("defclass", 3),
    ("define-condition", 3),
    // ── Lambda ──
    ("lambda", 1),
    ("named-lambda", 2),
    // ── Binding forms ──
    ("let", 1),
    ("let*", 1),
    ("flet", 1),
    ("labels", 1),
    ("generic-flet", 1),
    ("generic-labels", 1),
    ("macrolet", 1),
    ("compiler-macrolet", 1),
    ("symbol-macrolet", 1),
    ("handler-bind", 1),
    ("restart-bind", 1),
    // ── Control flow ──
    ("when", 1),
    ("unless", 1),
    ("block", 1),
    ("catch", 1),
    ("unwind-protect", 1),
    ("eval-when", 1),
    ("locally", 1),
    // ── Iteration ──
    ("dolist", 1),
    ("dotimes", 1),
    ("do", 2),
    ("do*", 2),
    ("prog", 2),
    ("prog*", 2),
    // ── Clause forms ──
    ("case", 1),
    ("ecase", 1),
    ("ccase", 1),
    ("typecase", 1),
    ("etypecase", 1),
    ("ctypecase", 1),
    ("handler-case", 1),
    // ── progn-like ──
    ("progn", 0),
    ("prog1", 1),
    ("prog2", 2),
    // ── with-* macros ──
    ("with-open-file", 1),
    ("with-open-stream", 1),
    ("with-input-from-string", 1),
    ("with-output-to-string", 1),
    ("with-hash-table-iterator", 1),
    ("with-package-iterator", 1),
    ("with-slots", 2),
    ("with-accessors", 2),
    ("with-compilation-unit", 1),
    ("with-standard-io-syntax", 1),
    ("with-condition-restarts", 2),
    // ── Value binding ──
    ("multiple-value-bind", 2),
    ("destructuring-bind", 2),
    ("multiple-value-call", 1),
    ("multiple-value-prog1", 1),
    ("progv", 2),
    // ── Emacs Lisp ──
    ("defvar", 2),
    ("defconst", 2),
    ("defcustom", 2),
    ("defgroup", 2),
    ("defalias", 2),
    ("defvaralias", 2),
    ("save-excursion", 0),
    ("save-restriction", 0),
    ("save-current-buffer", 0),
    ("track-mouse", 0),
    ("while", 1),
    ("condition-case", 2),
];

/// How many distinguished arguments `head` takes before its body, or `None`
/// when the Emacs convention gives it no special layout.
#[must_use]
pub fn body_form_distinguished(head: &str) -> Option<usize> {
    BODY_FORMS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(head))
        .map(|(_, distinguished)| *distinguished)
}

impl SyntaxTree {
    /// Returns the document with every line inside `span` re-indented.
    ///
    /// The span's own first line is left where it is: its column is set by
    /// whatever encloses the span, which is outside what this is being asked to
    /// change.
    #[must_use]
    pub fn reindent(&self, span: ByteSpan, indent: usize) -> String {
        let source = self.source();
        let indent = indent.max(1);
        let lines = LineIndex::new(source);
        let mut deltas = vec![0isize; lines.count()];
        let mut edits: Vec<(usize, usize, String)> = Vec::new();

        for line in 0..lines.count() {
            let start = lines.start(line);
            let Some(content) = content_offset(source, start) else {
                continue;
            };
            // The first line of the span keeps its position; later lines that
            // begin past the span belong to whatever follows it.
            if content <= span.start().get() || content >= span.end().get() {
                continue;
            }
            let Some(column) = self.target_column(content, indent, &lines, &deltas) else {
                continue;
            };

            let current = source[start..content].chars().count();
            if column != current {
                edits.push((start, content, " ".repeat(column)));
            }
            deltas[line] = column as isize - current as isize;
        }

        let mut output = source.to_owned();
        for (start, end, replacement) in edits.into_iter().rev() {
            output.replace_range(start..end, &replacement);
        }
        output
    }

    /// Returns the document with the top-level form containing `offset`
    /// re-indented, or the document unchanged when the offset is between forms.
    #[must_use]
    pub fn reindent_form_at(&self, offset: usize, indent: usize) -> String {
        match self.top_level_span_containing(offset) {
            Some(span) => self.reindent(span, indent),
            None => self.source().to_owned(),
        }
    }

    /// The span of the root-level form containing `offset`.
    #[must_use]
    pub fn top_level_span_containing(&self, offset: usize) -> Option<ByteSpan> {
        self.root_children()
            .iter()
            .map(|node_id| self.node(*node_id).span)
            .find(|span| span.start().get() <= offset && offset <= span.end().get())
    }

    /// The column the Emacs convention puts the line beginning at `content` in,
    /// or `None` when the line is not the convention's to place.
    fn target_column(
        &self,
        content: usize,
        indent: usize,
        lines: &LineIndex,
        deltas: &[isize],
    ) -> Option<usize> {
        match self.context_at(content).ok()?.kind {
            // A line *continuing* a multi-line string is that string's own
            // content, and moving it would change the value. A line that opens
            // one is placed like any other form.
            ContextKind::String => {
                let literal = self.innermost_node_at(content)?;
                if self.node(literal).span.start().get() < content {
                    return None;
                }
            }
            ContextKind::Comment => {
                let comment = self
                    .comments()
                    .find(|comment| comment.span().contains(ByteOffset::new(content)))?;
                // Inside a block comment the text is data; and `;;;` is the
                // column-zero convention for a section heading.
                if comment.span().start().get() != content || comment.text().starts_with(";;;") {
                    return None;
                }
            }
            _ => {}
        }

        let list = self.enclosing_open_list(content)?;
        let node = self.node(list);
        let open = node.open?;
        let base = column_at(open.get(), lines, deltas)?;

        let head_child = node.children.first().copied();
        let head = head_child.filter(|child| {
            let child = self.node(*child);
            child.kind == NodeKind::Atom
                && lines.line_of(child.span.start().get()) == lines.line_of(open.get())
        });
        let head_text = head.and_then(|child| {
            self.node(child)
                .span
                .slice(self.source())
                .get(self.node(child).symbol_offset as usize..)
        });

        if let Some(distinguished) = head_text.and_then(body_form_distinguished) {
            let position = node
                .children
                .iter()
                .position(|child| self.node(*child).span.start().get() == content);
            return Some(match position {
                Some(index) if index >= 1 && index <= distinguished => base + indent * 2,
                _ => base + indent,
            });
        }

        // A form the table does not know aligns under its first argument, but
        // only when that argument shares the head's line; otherwise everything
        // sits one column past the open delimiter.
        if head.is_some() {
            if let Some(first_argument) = node.children.get(1) {
                let start = self.node(*first_argument).span.start().get();
                if lines.line_of(start) == lines.line_of(open.get()) {
                    return column_at(start, lines, deltas);
                }
            }
        }
        Some(base + 1)
    }

    /// The innermost list whose opening delimiter precedes `offset`.
    fn enclosing_open_list(&self, offset: usize) -> Option<NodeId> {
        let mut current = self.innermost_node_at(offset)?;
        loop {
            let node = self.node(current);
            if node.kind == NodeKind::List && node.open.is_some_and(|open| open.get() < offset) {
                return Some(current);
            }
            current = node.parent?;
        }
    }
}

/// The offset of the first non-whitespace byte on the line beginning at
/// `start`, or `None` when the line is blank.
fn content_offset(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    let line = rest.split('\n').next().unwrap_or(rest);
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    (indent < line.trim_end_matches('\r').len()).then_some(start + indent)
}

/// The column of `offset` after the indentation changes recorded so far.
fn column_at(offset: usize, lines: &LineIndex, deltas: &[isize]) -> Option<usize> {
    let line = lines.line_of(offset);
    let original = lines.column_of(offset);
    usize::try_from(original as isize + deltas.get(line).copied().unwrap_or(0)).ok()
}

/// Line starts, so a column can be found without rescanning the document.
struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(source: &'a str) -> Self {
        let mut starts = vec![0usize];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(index, _)| index + 1),
        );
        Self { source, starts }
    }

    // Not `const`: `Vec::len` is only const from 1.87 and the MSRV is 1.85.
    fn count(&self) -> usize {
        self.starts.len()
    }

    fn start(&self, line: usize) -> usize {
        self.starts[line]
    }

    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset) - 1
    }

    fn column_of(&self, offset: usize) -> usize {
        let start = self.start(self.line_of(offset));
        self.source[start..offset].chars().count()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn reindent(source: &str) -> String {
        let tree = SyntaxTree::parse(source).unwrap();
        tree.reindent_form_at(0, 2)
    }

    #[test]
    fn a_defun_body_lands_at_two_spaces() {
        assert_eq!(
            reindent("(defun f (x)\n        (list x))\n"),
            "(defun f (x)\n  (list x))\n"
        );
    }

    #[test]
    fn a_distinguished_argument_lands_at_four_spaces() {
        assert_eq!(
            reindent("(defun f\n  (x)\n      (list x))\n"),
            "(defun f\n    (x)\n  (list x))\n"
        );
    }

    #[test]
    fn a_call_aligns_under_its_first_argument() {
        assert_eq!(
            reindent("(list alpha\n  beta)\n"),
            "(list alpha\n      beta)\n"
        );
    }

    #[test]
    fn a_call_whose_head_stands_alone_indents_by_one() {
        assert_eq!(reindent("(list\n     alpha)\n"), "(list\n alpha)\n");
    }

    #[test]
    fn nesting_composes_because_the_outer_line_moves_first() {
        assert_eq!(
            reindent("(defun f (x)\n      (when x\n            (list x)))\n"),
            "(defun f (x)\n  (when x\n    (list x)))\n"
        );
    }

    #[test]
    fn an_already_conventional_form_is_returned_byte_identical() {
        let source = "(defun f (x)\n  (when x\n    (list x)))\n";
        assert_eq!(reindent(source), source);
    }

    #[test]
    fn a_multi_line_string_keeps_its_own_layout() {
        let source = "(defun f ()\n      \"line one\nline two\")\n";
        assert_eq!(reindent(source), "(defun f ()\n  \"line one\nline two\")\n");
    }

    #[test]
    fn an_own_line_comment_moves_with_the_body_but_a_heading_does_not() {
        assert_eq!(
            reindent("(defun f ()\n        ;; note\n  (list))\n"),
            "(defun f ()\n  ;; note\n  (list))\n"
        );
        let heading = "(defun f ()\n;;; heading\n  (list))\n";
        assert_eq!(reindent(heading), heading);
    }

    #[test]
    fn a_binding_list_aligns_under_its_first_element() {
        assert_eq!(
            reindent("(let ((x 1)\n            (y 2))\n  x)\n"),
            "(let ((x 1)\n      (y 2))\n  x)\n"
        );
    }

    #[test]
    fn reindent_is_idempotent() {
        let once = reindent("(defun f (x)\n   (when x\n       (list x)))\n");
        assert_eq!(reindent(&once), once);
    }

    #[test]
    fn a_form_the_offset_misses_leaves_the_document_alone() {
        let source = "(a)\n\n(b)\n";
        let tree = SyntaxTree::parse(source).unwrap();
        assert_eq!(tree.reindent_form_at(4, 2), source);
    }
}
