//! The first-line file-variable header, and the `lexical-binding` flag in it.
//!
//! Emacs reads `lexical-binding` from the first line of a file and nowhere
//! else. A `Local Variables:` block at the end of the file sets every other
//! file variable, but not this one — by the time the reader gets there the
//! whole file has already been read under the default, which is dynamic
//! binding. That asymmetry is why this module looks at one line rather than
//! reusing a general file-variable parser.

use crate::sexpr::{ByteOffset, ByteSpan};

/// What a file's first line says about `lexical-binding`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmacsLispLexicalBinding {
    /// `lexical-binding: t` — the file is lexically scoped.
    Enabled,
    /// `lexical-binding: nil` — dynamically scoped, and said so on purpose.
    DisabledExplicitly,
    /// No `lexical-binding` setting at all. Emacs falls back to dynamic
    /// binding and, since Emacs 27, warns while byte-compiling.
    Absent,
}

impl EmacsLispLexicalBinding {
    /// Whether a plain `let` in this file binds lexically.
    #[must_use]
    pub const fn is_lexical(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// The first-line `-*- … -*-` header of an Emacs Lisp file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmacsLispFileHeader {
    lexical_binding: EmacsLispLexicalBinding,
    prop_line: Option<ByteSpan>,
    lexical_binding_span: Option<ByteSpan>,
}

impl EmacsLispFileHeader {
    #[must_use]
    pub const fn lexical_binding(&self) -> EmacsLispLexicalBinding {
        self.lexical_binding
    }

    /// The span of the `-*- … -*-` region, when the file has one.
    #[must_use]
    pub const fn prop_line(&self) -> Option<ByteSpan> {
        self.prop_line
    }

    /// The span of the `lexical-binding: VALUE` setting itself, for a
    /// diagnostic that wants to point at the value rather than the line.
    #[must_use]
    pub const fn lexical_binding_span(&self) -> Option<ByteSpan> {
        self.lexical_binding_span
    }
}

/// Reads the header Emacs itself would read.
///
/// The line scanned is the first, unless it starts with `#!`, in which case it
/// is the second — Emacs applies the same shebang exception so that a script
/// with `#!/usr/bin/emacs --script` can still declare its file variables.
#[must_use]
pub fn emacs_lisp_file_header(input: &str) -> EmacsLispFileHeader {
    let (line_start, line) = prop_line_candidate(input);
    let Some(prop_line) = prop_line_span(line, line_start) else {
        return EmacsLispFileHeader {
            lexical_binding: EmacsLispLexicalBinding::Absent,
            prop_line: None,
            lexical_binding_span: None,
        };
    };

    let body = &input[prop_line.start().get()..prop_line.end().get()];
    let (lexical_binding, lexical_binding_span) =
        lexical_binding_setting(body, prop_line.start().get());

    EmacsLispFileHeader {
        lexical_binding,
        prop_line: Some(prop_line),
        lexical_binding_span,
    }
}

/// The line whose `-*- … -*-` header counts, and its offset in the file.
fn prop_line_candidate(input: &str) -> (usize, &str) {
    let first = input.split('\n').next().unwrap_or("");
    if !first.starts_with("#!") {
        return (0, first);
    }

    let start = first.len() + usize::from(input.len() > first.len());
    let second = input
        .get(start..)
        .unwrap_or("")
        .split('\n')
        .next()
        .unwrap_or("");
    (start, second)
}

/// The span of the text *between* the `-*-` delimiters on `line`.
fn prop_line_span(line: &str, line_start: usize) -> Option<ByteSpan> {
    let open = line.find("-*-")?;
    let after_open = open + "-*-".len();
    let close = line[after_open..].find("-*-")? + after_open;
    ByteSpan::try_new(
        ByteOffset::new(line_start + after_open),
        ByteOffset::new(line_start + close),
    )
}

/// Finds `lexical-binding: VALUE` among the `;`-separated settings.
///
/// A bare `-*- emacs-lisp -*-` mode declaration has no settings at all and is
/// handled by finding no colon, which reads as absent — the same answer as a
/// header that lists other variables but not this one.
fn lexical_binding_setting(
    body: &str,
    body_start: usize,
) -> (EmacsLispLexicalBinding, Option<ByteSpan>) {
    let mut cursor = 0;
    for setting in body.split(';') {
        let offset = cursor;
        cursor += setting.len() + 1;

        let Some((name, value)) = setting.split_once(':') else {
            continue;
        };
        if name.trim() != "lexical-binding" {
            continue;
        }

        // Emacs treats every value but `nil` as true here, which is why
        // `lexical-binding: t` and `lexical-binding: 1` behave alike and only
        // the literal `nil` turns the flag off.
        let binding = if value.trim() == "nil" {
            EmacsLispLexicalBinding::DisabledExplicitly
        } else {
            EmacsLispLexicalBinding::Enabled
        };

        let start = body_start + offset + leading_space(setting);
        let end = body_start + offset + setting.trim_end().len();
        return (
            binding,
            ByteSpan::try_new(ByteOffset::new(start), ByteOffset::new(end)),
        );
    }

    (EmacsLispLexicalBinding::Absent, None)
}

fn leading_space(text: &str) -> usize {
    text.len() - text.trim_start().len()
}
