use crate::dialect::Dialect;

use super::tree::ReaderPrefix;
use super::types::Delimiter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReaderMacro {
    Prefix {
        semantic: ReaderPrefix,
        width: usize,
    },
    Discard {
        width: usize,
    },
    MultiDatum {
        width: usize,
        payload_forms: usize,
    },
    UnsupportedDispatch {
        width: usize,
    },
}

/// Dialect-specific lexical decisions shared by normal parsing and discarded
/// form scanning. Keeping these decisions in one place prevents the two paths
/// from disagreeing about the extent of a reader form.
#[derive(Debug, Clone, Copy)]
pub(super) struct DialectReaderPolicy {
    dialect: Dialect,
}

impl DialectReaderPolicy {
    pub(super) const fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    pub(super) const fn is_legacy(self) -> bool {
        matches!(self.dialect, Dialect::Unknown)
    }

    pub(super) const fn additional_discarded_forms_for_prefix(self, prefix: ReaderPrefix) -> usize {
        if matches!(
            (self.dialect, prefix),
            (Dialect::Clojure, ReaderPrefix::Metadata)
        ) {
            1
        } else {
            0
        }
    }

    pub(super) const fn is_whitespace(self, byte: u8) -> bool {
        byte.is_ascii_whitespace() || matches!(self.dialect, Dialect::Clojure) && byte == b','
    }

    pub(super) fn line_comment_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let byte = *bytes.get(pos)?;
        match self.dialect {
            Dialect::Janet if byte == b'#' => Some(1),
            Dialect::Janet => None,
            _ if byte == b';' => Some(1),
            _ => None,
        }
    }

    pub(super) const fn supports_block_comments(self) -> bool {
        matches!(
            self.dialect,
            Dialect::CommonLisp
                | Dialect::Lfe
                | Dialect::Scheme
                | Dialect::Racket
                | Dialect::Unknown
        )
    }

    /// Whether `|...|` reads as one symbol rather than a token boundary.
    ///
    /// R7RS 2.1 gives Scheme the same vertical-line notation Common Lisp has
    /// in CLHS 2.1.4.2, so `|Foo Bar|` is a single identifier in both.
    pub(super) const fn supports_bar_quoted_symbols(self) -> bool {
        matches!(
            self.dialect,
            Dialect::CommonLisp | Dialect::Scheme | Dialect::Racket | Dialect::Unknown
        )
    }

    /// Whether a bare `\` escapes the next character *outside* `|...|`.
    ///
    /// Deliberately narrower than [`Self::supports_bar_quoted_symbols`].
    /// Common Lisp's single-escape works anywhere in a token, so `a\ b` is one
    /// symbol; Scheme has no such rule, and its `\x41;` escapes are legal only
    /// inside a vertical-line region, which `consume_multiple_escape` handles
    /// on its own. Reading a stray `\` as an escape in Scheme would swallow
    /// the delimiter after it and unbalance the tree.
    pub(super) const fn supports_single_escape(self) -> bool {
        matches!(self.dialect, Dialect::CommonLisp | Dialect::Unknown)
    }

    pub(super) fn delimiter_from_open(self, byte: u8) -> Option<Delimiter> {
        let delimiter = Delimiter::from_open(byte)?;
        self.allows_delimiter(delimiter).then_some(delimiter)
    }

    pub(super) fn delimiter_from_close(self, byte: u8) -> Option<Delimiter> {
        let delimiter = Delimiter::from_close(byte)?;
        self.allows_delimiter(delimiter).then_some(delimiter)
    }

    pub(super) const fn is_raw_delimiter(byte: u8) -> bool {
        Delimiter::from_open(byte).is_some() || Delimiter::from_close(byte).is_some()
    }

    pub(super) fn is_atom_boundary(self, bytes: &[u8], pos: usize) -> bool {
        bytes.get(pos).is_none_or(|byte| {
            self.is_whitespace(*byte)
                || Self::is_raw_delimiter(*byte)
                || self.line_comment_width(bytes, pos).is_some()
        })
    }

    pub(super) fn character_literal_prefix_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        match self.dialect {
            Dialect::Scheme | Dialect::Racket if byte == b'#' && next == Some(b'\\') => Some(2),
            Dialect::Clojure if byte == b'\\' => Some(1),
            Dialect::EmacsLisp if byte == b'?' && next == Some(b'\\') => Some(2),
            Dialect::EmacsLisp if byte == b'?' => Some(1),
            _ => None,
        }
    }

    pub(super) fn classify_reader_macro(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let third = bytes.get(pos + 2).copied();

        match self.dialect {
            Dialect::Unknown | Dialect::Lfe | Dialect::Hy | Dialect::Carp => {
                self.classify_legacy(byte, next, third)
            }
            Dialect::CommonLisp => self.classify_common_lisp(bytes, pos),
            Dialect::EmacsLisp => self.classify_emacs_lisp(byte, next),
            Dialect::Scheme | Dialect::Racket => self.classify_scheme(bytes, pos),
            Dialect::Clojure => self.classify_clojure(bytes, pos),
            Dialect::Janet => self.classify_janet(byte, next),
            Dialect::Fennel => self.classify_fennel(byte, next),
        }
    }

    const fn allows_delimiter(self, delimiter: Delimiter) -> bool {
        match self.dialect {
            Dialect::CommonLisp => matches!(delimiter, Delimiter::Paren),
            // R6RS 4.2.1 makes `[` `]` equivalent to `(` `)`, and every Scheme
            // in wide use -- Guile, Chez, Chicken, Gauche, MIT -- accepts them
            // even under an R7RS reader, where they are merely reserved. The
            // bracketed binding list `(let ([x 1]) x)` is the dominant idiom,
            // so refusing it rejected the majority of real Scheme outright.
            // Braces stay out: no Scheme reader gives them a meaning, and
            // Racket, which does, is a separate arm below.
            Dialect::Scheme | Dialect::EmacsLisp => {
                matches!(delimiter, Delimiter::Paren | Delimiter::Bracket)
            }
            Dialect::Racket
            | Dialect::Lfe
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel
            | Dialect::Unknown => true,
        }
    }

    const fn classify_legacy(
        self,
        byte: u8,
        next: Option<u8>,
        third: Option<u8>,
    ) -> Option<ReaderMacro> {
        if byte == b'#' && matches!(next, Some(b';' | b'_')) {
            return Some(ReaderMacro::Discard { width: 2 });
        }
        if byte == b'#' && matches!(next, Some(b'+' | b'-')) {
            return Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 2,
            });
        }
        classify_shared_prefix(byte, next, third)
    }

    fn classify_common_lisp(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();

        if byte == b'#' && matches!(next, Some(b'+' | b'-')) {
            return Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 2,
            });
        }
        if let Some(prefix) = classify_quote_prefix(byte, next) {
            return Some(prefix);
        }
        if byte != b'#' {
            return None;
        }
        if let Some(dispatch) = classify_numeric_dispatch(bytes, pos, true) {
            return Some(dispatch);
        }
        if is_numeric_radix_dispatch(bytes, pos) {
            return None;
        }
        match next {
            Some(b':' | b'\\' | b'*' | b'b' | b'B' | b'o' | b'O' | b'd' | b'D' | b'x' | b'X') => {
                None
            }
            Some(b'p' | b'P' | b's' | b'S') => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
            Some(b'\'') => prefix(ReaderPrefix::Function, 2),
            Some(b'.') => prefix(ReaderPrefix::ReadEval, 2),
            Some(b'(') => prefix(ReaderPrefix::HashLiteral, 1),
            _ => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
        }
    }

    const fn classify_emacs_lisp(self, byte: u8, next: Option<u8>) -> Option<ReaderMacro> {
        if let Some(prefix) = classify_quote_prefix(byte, next) {
            return Some(prefix);
        }
        if byte != b'#' {
            return None;
        }
        match next {
            Some(b'\'') => prefix(ReaderPrefix::Function, 2),
            _ => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
        }
    }

    fn classify_scheme(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        if byte == b'#' && next == Some(b';') {
            return Some(ReaderMacro::Discard { width: 2 });
        }
        if let Some(prefix) = classify_quote_prefix(byte, next) {
            return Some(prefix);
        }
        if byte != b'#' {
            return None;
        }
        if let Some(dispatch) = classify_numeric_dispatch(bytes, pos, false) {
            return Some(dispatch);
        }
        if matches!(next, Some(b'u' | b'U'))
            && bytes.get(pos + 2) == Some(&b'8')
            && bytes.get(pos + 3) == Some(&b'(')
        {
            return Some(ReaderMacro::MultiDatum {
                width: 3,
                payload_forms: 1,
            });
        }
        match next {
            Some(b'(') => prefix(ReaderPrefix::HashLiteral, 1),
            Some(
                b'\\' | b't' | b'T' | b'f' | b'F' | b'b' | b'B' | b'o' | b'O' | b'd' | b'D' | b'x'
                | b'X' | b'e' | b'E' | b'i' | b'I',
            ) => None,
            // `#:mode` is a Racket keyword: a self-evaluating literal, and the
            // spelling of every keyword argument in Racket's standard library.
            // It reads as one atom, so it is not a reader macro at all.
            Some(b':') if matches!(self.dialect, Dialect::Racket) => None,
            // `#!fold-case`, `#!no-fold-case`, `#!eof`, `#!default`: R7RS
            // directives and their Guile/MIT extensions. A structural tool has
            // nothing to do with them beyond keeping them in the tree as
            // atoms, which is what `None` achieves.
            Some(b'!') => None,
            _ => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
        }
    }

    fn classify_clojure(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let third = bytes.get(pos + 2).copied();
        let fourth = bytes.get(pos + 3).copied();
        match byte {
            b'\'' => prefix(ReaderPrefix::Quote, 1),
            b'`' => prefix(ReaderPrefix::Quasiquote, 1),
            b'~' if next == Some(b'@') => prefix(ReaderPrefix::UnquoteSplicing, 2),
            b'~' => prefix(ReaderPrefix::Unquote, 1),
            b'@' => prefix(ReaderPrefix::Function, 1),
            b'^' => prefix(ReaderPrefix::Metadata, 1),
            b'#' if next == Some(b'_') => Some(ReaderMacro::Discard { width: 2 }),
            b'#' if next == Some(b'?') && third == Some(b'@') && fourth == Some(b'(') => {
                prefix(ReaderPrefix::ReaderConditionalSplicing, 3)
            }
            b'#' if next == Some(b'?') && third == Some(b'(') => {
                prefix(ReaderPrefix::ReaderConditional, 2)
            }
            b'#' if next == Some(b'?') => Some(ReaderMacro::UnsupportedDispatch {
                width: usize::from(third == Some(b'@')) + 2,
            }),
            b'#' if next == Some(b'\'') => prefix(ReaderPrefix::Function, 2),
            b'#' if matches!(next, Some(b'(' | b'{')) => prefix(ReaderPrefix::HashLiteral, 1),
            b'#' if next == Some(b'"') => Some(ReaderMacro::MultiDatum {
                width: 1,
                payload_forms: 1,
            }),
            b'#' if next == Some(b':') => self
                .clojure_namespaced_map_width(bytes, pos)
                .map(|width| ReaderMacro::MultiDatum {
                    width,
                    payload_forms: 1,
                })
                .or(Some(ReaderMacro::UnsupportedDispatch { width: 1 })),
            b'#' if next == Some(b'#') => None,
            b'#' => self
                .clojure_tagged_literal_width(bytes, pos)
                .map(|width| ReaderMacro::MultiDatum {
                    width,
                    payload_forms: 1,
                })
                .or(Some(ReaderMacro::UnsupportedDispatch { width: 1 })),
            _ => None,
        }
    }

    fn clojure_namespaced_map_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let mut cursor = pos + 2;
        let auto_resolved = bytes.get(cursor) == Some(&b':');
        if auto_resolved {
            cursor += 1;
        }
        let namespace_start = cursor;
        while let Some(&byte) = bytes.get(cursor) {
            if byte == b'{' {
                return (auto_resolved || cursor > namespace_start).then_some(cursor - pos);
            }
            if self.is_atom_boundary(bytes, cursor) {
                return None;
            }
            cursor += 1;
        }
        None
    }

    fn clojure_tagged_literal_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let first = *bytes.get(pos + 1)?;
        if !(first.is_ascii_alphabetic()
            || matches!(
                first,
                b'*' | b'+' | b'!' | b'-' | b'_' | b'\'' | b'?' | b'<' | b'>' | b'='
            ))
        {
            return None;
        }

        let mut cursor = pos + 2;
        while !self.is_atom_boundary(bytes, cursor) {
            cursor += 1;
        }

        let tag = &bytes[pos + 1..cursor];
        if tag.last() == Some(&b'/') || tag.iter().filter(|byte| **byte == b'/').count() > 1 {
            return None;
        }
        Some(cursor - pos)
    }

    const fn classify_janet(self, byte: u8, next: Option<u8>) -> Option<ReaderMacro> {
        match byte {
            b';' => prefix(ReaderPrefix::UnquoteSplicing, 1),
            b'~' => prefix(ReaderPrefix::Quasiquote, 1),
            b',' => prefix(ReaderPrefix::Unquote, 1),
            b'|' => prefix(ReaderPrefix::Function, 1),
            b'@' => prefix(ReaderPrefix::HashLiteral, 1),
            // `#` is consumed as a line comment before reader classification.
            b'#' if next.is_some() => None,
            _ => None,
        }
    }

    fn classify_fennel(self, byte: u8, next: Option<u8>) -> Option<ReaderMacro> {
        match byte {
            b'\'' => prefix(ReaderPrefix::Quote, 1),
            b'`' => prefix(ReaderPrefix::Quasiquote, 1),
            b',' if next == Some(b'@') => prefix(ReaderPrefix::UnquoteSplicing, 2),
            b',' => prefix(ReaderPrefix::Unquote, 1),
            b'#' => prefix(ReaderPrefix::Function, 1),
            _ => None,
        }
    }
}

const fn classify_shared_prefix(
    byte: u8,
    next: Option<u8>,
    third: Option<u8>,
) -> Option<ReaderMacro> {
    if let Some(prefix) = classify_quote_prefix(byte, next) {
        return Some(prefix);
    }
    match (byte, next, third) {
        (b'^', _, _) => prefix(ReaderPrefix::Metadata, 1),
        (b'#', Some(b'.'), _) => prefix(ReaderPrefix::ReadEval, 2),
        (b'#', Some(b'\''), _) => prefix(ReaderPrefix::Function, 2),
        (b'#', Some(b'?'), Some(b'@')) => prefix(ReaderPrefix::ReaderConditionalSplicing, 3),
        (b'#', Some(b'?'), _) => prefix(ReaderPrefix::ReaderConditional, 2),
        (b'#', Some(b'(' | b'[' | b'{'), _) => prefix(ReaderPrefix::HashLiteral, 1),
        _ => None,
    }
}

fn classify_numeric_dispatch(bytes: &[u8], pos: usize, allow_array: bool) -> Option<ReaderMacro> {
    if bytes.get(pos) != Some(&b'#') {
        return None;
    }

    let mut marker_pos = pos + 1;
    while matches!(bytes.get(marker_pos), Some(byte) if byte.is_ascii_digit()) {
        marker_pos += 1;
    }

    let has_numeric_argument = marker_pos > pos + 1;
    let payload_forms = match bytes.get(marker_pos).copied() {
        Some(b'=') if has_numeric_argument => 1,
        Some(b'#') if has_numeric_argument => 0,
        Some(b'a' | b'A') if allow_array => 1,
        _ => return None,
    };
    Some(ReaderMacro::MultiDatum {
        width: marker_pos - pos + 1,
        payload_forms,
    })
}

fn is_numeric_radix_dispatch(bytes: &[u8], pos: usize) -> bool {
    if bytes.get(pos) != Some(&b'#') {
        return false;
    }

    let mut marker_pos = pos + 1;
    while matches!(bytes.get(marker_pos), Some(byte) if byte.is_ascii_digit()) {
        marker_pos += 1;
    }

    marker_pos > pos + 1 && matches!(bytes.get(marker_pos), Some(b'r' | b'R'))
}

const fn classify_quote_prefix(byte: u8, next: Option<u8>) -> Option<ReaderMacro> {
    match (byte, next) {
        (b'\'', _) => prefix(ReaderPrefix::Quote, 1),
        (b'`', _) => prefix(ReaderPrefix::Quasiquote, 1),
        (b',', Some(b'@')) => prefix(ReaderPrefix::UnquoteSplicing, 2),
        (b',', _) => prefix(ReaderPrefix::Unquote, 1),
        _ => None,
    }
}

const fn prefix(semantic: ReaderPrefix, width: usize) -> Option<ReaderMacro> {
    Some(ReaderMacro::Prefix { semantic, width })
}
