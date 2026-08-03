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

/// How far a Janet long string reaches from its opening backtick run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LongStringExtent {
    /// Total byte width of the literal, both delimiter runs included.
    Closed { width: usize },
    /// An opening run with no closing run of the same length before EOF.
    Unterminated,
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
            // `#lang racket/base` is a reader *directive*, not a datum: it
            // names the language for the rest of the file and extends to the
            // end of the line. Every real Racket file opens with one, and
            // reading it as a dispatch made all of them fail to parse.
            //
            // Consuming it as a line comment keeps it in the trivia, so it
            // survives a format round-trip; `Dialect::detect` reads it
            // separately to decide which language the file is in.
            //
            // The `byte == b'#'` guard is not redundant. `is_atom_boundary`
            // calls this once per byte of every atom in the file, so without
            // it each of those bytes would pay a bounds check and a five-byte
            // comparison. `head_index` records what a per-token cost on this
            // path did to the lint benchmark.
            Dialect::Scheme | Dialect::Racket
                if byte == b'#' && starts_with_lang_directive(bytes, pos) =>
            {
                Some(LANG_DIRECTIVE.len())
            }
            // An Emacs Lisp script starts `#!/usr/bin/emacs --script`, and
            // Emacs skips that line the way it skips a comment. Reading it as
            // one keeps the byte offsets of everything after it unchanged,
            // which stripping the line would not; and restricting it to
            // offset 0 keeps a stray `#!` anywhere else the reader error it
            // has always been.
            Dialect::EmacsLisp if pos == 0 && bytes.starts_with(b"#!") => Some(2),
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

    /// Whether a backtick opens a long string in this dialect.
    ///
    /// Janet is the only one. Its `root` state sends every backtick to the
    /// `longstring` consumer (`src/core/parse.c`), and `symchars` leaves bit
    /// 0x60 clear, so a backtick is not a symbol character either -- it both
    /// opens a literal and ends whatever token preceded it.
    ///
    /// In every other dialect here a backtick is quasiquote (Common Lisp,
    /// Scheme, Racket, Emacs Lisp, Fennel, and the permissive legacy reader)
    /// or syntax-quote (Clojure), which `classify_reader_macro` already
    /// returns as a one-byte `ReaderPrefix::Quasiquote`. Nothing below may
    /// change for them.
    pub(super) const fn has_long_strings(self) -> bool {
        matches!(self.dialect, Dialect::Janet)
    }

    pub(super) fn is_atom_boundary(self, bytes: &[u8], pos: usize) -> bool {
        bytes.get(pos).is_none_or(|byte| {
            self.is_whitespace(*byte)
                || Self::is_raw_delimiter(*byte)
                // Dialect first: `self.dialect` is loop-invariant across the
                // per-byte calls this makes for every atom in the document, so
                // for the nine dialects without long strings the test folds
                // away instead of costing a comparison per byte.
                || (self.has_long_strings() && *byte == b'`')
                || self.line_comment_width(bytes, pos).is_some()
        })
    }

    /// How far the Janet long string starting at `pos` reaches, if one starts
    /// there.
    ///
    /// Janet's `longstring` state (`src/core/parse.c`) counts the opening run
    /// in `argn` while it keeps seeing backticks, then closes the literal on
    /// the `argn`-th consecutive backtick it meets afterwards. Two consequences
    /// follow, and both are load-bearing:
    ///
    /// * The opener is the *whole* run. ```` ```` ```` is a four-backtick
    ///   opener, not two empty strings, so an empty long string cannot be
    ///   written at all.
    /// * The close is exactly `argn` backticks, not at least `argn`. Janet
    ///   returns 0 from `stringend` so the character that revealed the end is
    ///   re-dispatched, which means a longer run leaves its surplus to open
    ///   the next datum: `` ```ab```` x` `` reads as `"ab"` then `" x"`. A run
    ///   shorter than `argn` is content ("failed end candidate" pushes the
    ///   backticks it had buffered back into the string).
    ///
    /// There is no escape processing inside one -- the `PFLAG_INSTRING` branch
    /// has no `\\` case -- and a newline is an ordinary content byte, which is
    /// the entire point of the form.
    pub(super) fn long_string_extent(self, bytes: &[u8], pos: usize) -> Option<LongStringExtent> {
        if !self.has_long_strings() || bytes.get(pos) != Some(&b'`') {
            return None;
        }
        let open_len = backtick_run_length(bytes, pos);
        let mut cursor = pos + open_len;
        while cursor < bytes.len() {
            if bytes[cursor] != b'`' {
                cursor += 1;
                continue;
            }
            let run = backtick_run_length(bytes, cursor);
            if run >= open_len {
                return Some(LongStringExtent::Closed {
                    width: cursor + open_len - pos,
                });
            }
            cursor += run;
        }
        Some(LongStringExtent::Unterminated)
    }

    /// How many bytes introduce a character literal at `pos`, if one starts
    /// there.
    ///
    /// A prefix is only a prefix when something follows it. `#\` at end of
    /// input is not the character literal for nothing — it is a truncated one,
    /// and reading it as a complete atom had a consequence beyond pedantry:
    /// the formatter appends a trailing newline, the truncated literal
    /// swallowed it as its character, and `format(format(x))` differed from
    /// `format(x)`. A robustness property found it on `"#\\"` in Scheme.
    ///
    /// The permissive legacy reader already rejects the same input
    /// ("single escape is missing an escaped character"), so this makes the
    /// dialect readers agree with it rather than inventing a new rule.
    pub(super) fn character_literal_prefix_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let width = match self.dialect {
            Dialect::Scheme | Dialect::Racket if byte == b'#' && next == Some(b'\\') => 2,
            Dialect::Clojure if byte == b'\\' => 1,
            Dialect::EmacsLisp if byte == b'?' && next == Some(b'\\') => 2,
            Dialect::EmacsLisp if byte == b'?' => 1,
            _ => return None,
        };
        bytes.get(pos + width).is_some().then_some(width)
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
            Dialect::EmacsLisp => self.classify_emacs_lisp(bytes, pos),
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
            // `#p"..."` pathname, `#s(...)` structure, `#c(re im)` complex.
            // All three are a two-byte dispatch followed by exactly one datum.
            //
            // `#c` was missing until a corpus run over Quicklisp found it:
            // `(equal '(#C(0.0 0.0) #C(0.0 2.0)) ...)` in alexandria's test
            // suite failed to parse, and a file that does not parse is a file
            // none of this tool's 275 commands can say anything about. It is
            // CLHS 2.4.8.11, not an implementation extension.
            Some(b'p' | b'P' | b's' | b'S' | b'c' | b'C') => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
            Some(b'\'') => prefix(ReaderPrefix::Function, 2),
            Some(b'.') => prefix(ReaderPrefix::ReadEval, 2),
            Some(b'(') => prefix(ReaderPrefix::HashLiteral, 1),
            _ => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
        }
    }

    fn classify_emacs_lisp(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();

        if let Some(prefix) = classify_quote_prefix(byte, next) {
            return Some(prefix);
        }
        // `?` and `?\` at end of input are truncated character literals.
        // Rejecting them prevents formatting from turning a missing payload
        // into whitespace that changes meaning on the next parse.
        if byte == b'?' && (next.is_none() || (next == Some(b'\\') && bytes.get(pos + 2).is_none()))
        {
            return Some(ReaderMacro::UnsupportedDispatch { width: 1 });
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
        // `#\` at end of input is a truncated character literal, not the
        // character literal for nothing. Reading it as a complete atom made
        // the formatter non-idempotent: it appends a trailing newline, the
        // truncated literal claims it as its character, and the next pass
        // appends another. Common Lisp, Emacs Lisp and Clojure already reject
        // it through their own escape rules; this makes Scheme and Racket
        // agree rather than being the two that do not.
        if next == Some(b'\\') && bytes.get(pos + 2).is_none() {
            return Some(ReaderMacro::UnsupportedDispatch { width: 1 });
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
            // A bare `\` at end of input is a truncated character literal, not
            // a symbol. Same reasoning as `#\` in Scheme: the formatter's
            // trailing newline would become its character, and
            // `format(format(x))` would differ from `format(x)`.
            b'\\' if next.is_none() => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
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
            // `#=(form)` is Clojure's read-time-eval dispatch, the analogue
            // of Common Lisp's `#.`: it reads and evaluates exactly one form
            // at read time. Giving it its own two-byte-dispatch arm, rather
            // than leaving it to fall into the generic tagged-literal case
            // below, matters for more than symmetry with `#.` — the tagged-
            // literal scanner treats every non-boundary byte after the `=`
            // as part of the tag name, so `#=foo` with no space would read
            // as the single-character tag `=foo` applied to the *next* form
            // rather than as `#=` applied to `foo`. `=` is not a symbol
            // constituent in Clojure, so `#=foo` unambiguously means
            // "evaluate `foo`" and this arm reads it that way.
            b'#' if next == Some(b'=') => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
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

    /// Byte width of the `#:ns` dispatch introducing a namespaced map literal.
    ///
    /// The width covers the dispatch alone -- `#:foo`, `#::foo`, `#::` -- and
    /// never the `{`. The brace is where the `MultiDatum` payload starts, and
    /// `skip_form` consumes it; the two together become one opaque reader-form
    /// node spanning the whole literal.
    ///
    /// That opacity is this model's known limitation, and it is unchanged here:
    /// `#:foo{:a 1}` reports zero atom occurrences, so a rule looking for map
    /// keys sees nothing inside a namespaced map. Representing the dispatch as
    /// a [`ReaderPrefix`] on the `{...}` list -- the way `#{...}` keeps its
    /// elements visible through `HashLiteral` -- is the shape that would fix
    /// that, but `ReaderPrefix` is a payload-free enum whose `as_source`
    /// returns a `&'static str`, and `#:foo` has no fixed spelling. Giving it
    /// one is a change to the prefix representation itself, rippling through
    /// the formatter and every prefix consumer, and it would rewrite the tree
    /// of every `#:foo{...}` that already parses. This fix deliberately does
    /// neither: it only widens *which* namespaced maps parse at all, and
    /// leaves the shape of the ones that already did byte-identical.
    ///
    /// Clojure's `NamespaceMapReader` (`LispReader.java`) allows whitespace
    /// between the namespace and the brace, and this must too:
    ///
    /// ```text
    /// } else if(nextChar != '{') {  // #:foo { } or #::foo { }
    ///     unread(r, nextChar);
    ///     sym = read(r, true, null, false, opts, pendingForms);
    ///     nextChar = read1(r);
    ///     while(isWhitespace(nextChar))
    ///         nextChar = read1(r);
    /// }
    /// if(nextChar != '{')
    ///     throw Util.runtimeException("Namespaced map must specify a map");
    /// ```
    ///
    /// Requiring the brace to touch the namespace made `#:foo {:a 1}` an
    /// unsupported dispatch, and an unsupported dispatch fails the whole parse
    /// -- so a single such literal silently dropped its entire file from every
    /// lint run. `#::it {:a #::it {}}` in clj-kondo's own corpus is the case
    /// that found it.
    ///
    /// `skip_form` skips trivia before reading a `MultiDatum` payload, so the
    /// whitespace needs no representation in the width; it stays trivia, which
    /// is what keeps a format round-trip byte-identical.
    ///
    /// Comments are deliberately not skipped. Clojure's loop advances over
    /// `isWhitespace` only, so `#:foo ;; c` then `{}` is its "must specify a
    /// map" error, and refusing it here agrees rather than inventing a rule.
    /// `isWhitespace` counts a comma as whitespace and so does
    /// [`Self::is_whitespace`], so `#:foo,{:a 1}` reads for both.
    ///
    /// Visible to the crate for the same reason [`Self::long_string_extent`]
    /// is: the width is a shared decision worth pinning directly, rather than
    /// only through documents that happen to exercise it.
    pub(super) fn clojure_namespaced_map_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        let mut cursor = pos + 2;
        let auto_resolved = bytes.get(cursor) == Some(&b':');
        if auto_resolved {
            cursor += 1;
        }
        let namespace_start = cursor;
        while let Some(&byte) = bytes.get(cursor) {
            if byte == b'{' || self.is_atom_boundary(bytes, cursor) {
                break;
            }
            cursor += 1;
        }
        let namespace_end = cursor;
        // `#:{...}` and `#: {...}` are both "Namespaced map must specify a
        // namespace" in Clojure. Only the auto-resolved `#::` may omit one.
        if !auto_resolved && namespace_end == namespace_start {
            return None;
        }
        let mut probe = namespace_end;
        while matches!(bytes.get(probe), Some(&byte) if self.is_whitespace(byte)) {
            probe += 1;
        }
        (bytes.get(probe) == Some(&b'{')).then_some(namespace_end - pos)
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
            // Janet's `root` state lists `'` in the same `PFLAG_READERMAC`
            // group as `,` `;` `~` `|` (`src/core/parse.c`), and `popstate`
            // expands it to the two-element tuple `(quote <form>)` -- exactly
            // one following datum, the same shape Common Lisp gives it. It was
            // the only member of that group missing here.
            //
            // Its absence was not a cosmetic gap. `'` is not in Janet's
            // `symchars` either, but `is_atom_boundary` does not know that, so
            // with no reader-macro arm the quote glued onto whatever followed:
            // `'foo` read as the single atom `'foo` rather than a quote prefix
            // on `foo`, and `(a '" " b)` failed outright with "unterminated
            // string" because the atom swallowed the opening quotation mark of
            // the string after it. That accounted for both remaining parse
            // failures over a 210-file Janet/spork corpus.
            b'\'' => prefix(ReaderPrefix::Quote, 1),
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

/// How many consecutive backticks start at `pos`.
fn backtick_run_length(bytes: &[u8], pos: usize) -> usize {
    bytes[pos..]
        .iter()
        .take_while(|byte| **byte == b'`')
        .count()
}

/// The Racket language directive, which the reader consumes to end of line.
pub(crate) const LANG_DIRECTIVE: &str = "#lang";

/// Whether a `#lang` directive starts at `pos`.
///
/// The directive must be followed by whitespace: `#language` is not one, and
/// neither is a bare `#lang` at end of input.
pub(crate) fn starts_with_lang_directive(bytes: &[u8], pos: usize) -> bool {
    bytes
        .get(pos..pos + LANG_DIRECTIVE.len())
        .is_some_and(|window| window == LANG_DIRECTIVE.as_bytes())
        && bytes
            .get(pos + LANG_DIRECTIVE.len())
            .is_some_and(u8::is_ascii_whitespace)
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

/// The language a `#lang` line names, given the line's text.
///
/// Lives beside the reader's own directive constants so the parser and the
/// dialect detector cannot disagree about what counts as one.
pub(crate) fn lang_directive_language(line: &str) -> Option<&str> {
    if !starts_with_lang_directive(line.as_bytes(), 0) {
        return None;
    }
    let language = line[LANG_DIRECTIVE.len()..].trim();
    (!language.is_empty()).then_some(language)
}
