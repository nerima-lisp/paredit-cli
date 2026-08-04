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

/// Where a `|...|` region may begin inside a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarQuoting {
    /// A `|` is an ordinary symbol constituent wherever it appears.
    None,
    /// CLHS 2.1.4.2 / R7RS 2.1: a multiple-escape may open anywhere in a token.
    Anywhere,
    /// LFE: a `|` opens a quoted symbol only where a token starts; anywhere
    /// else it is an ordinary constituent.
    TokenStart,
}

/// How far a Janet long string reaches from its opening backtick run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LongStringExtent {
    /// Total byte width of the literal, both delimiter runs included.
    Closed { width: usize },
    /// An opening run with no closing run of the same length before EOF.
    Unterminated,
}

/// How far a Racket here string reaches from its opening `#<<`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HereStringExtent {
    /// Total byte width of the literal: `#<<`, the tag, the newline that ends
    /// the opening line, the content, the terminating tag, and the newline
    /// after it when there is one.
    Closed { width: usize },
    /// Either no newline after `#<<` before EOF, or no terminator line before
    /// EOF. Racket raises a reader error for both.
    Unterminated,
}

/// How far a Hy string literal reaches from its opening delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HyStringExtent {
    /// Total byte width of the literal, prefix and both delimiters included.
    Closed { width: usize },
    /// An opening delimiter with no matching close before EOF.
    Unterminated,
    /// A byte Hy's own reader refuses outright: a `]` inside a bracket
    /// string's delimiter, or an undoubled `}` in an f-string's literal text.
    Refused { position: usize, byte: u8 },
}

/// The bytes a Hy string prefix may be built from (`hy_reader.prefixed_string`).
const HY_STRING_PREFIX_BYTES: &[u8] = b"bfrt";

/// How deeply a Hy f-string may nest string literals inside its `{...}`
/// interpolations before the reader refuses it.
///
/// `f"{f"{x}"}"` is legal and reads recursively, so the scanner recurses too,
/// and a bound keeps adversarial input from exhausting the stack. Real code
/// does not go past two or three; 32 is far above anything the 2825-file
/// corpus contains and far below a stack limit.
const MAX_HY_STRING_NESTING: usize = 32;

/// What ends a Hy string body.
#[derive(Debug, Clone, Copy)]
enum HyCloser<'a> {
    /// An unescaped `"`.
    Quote,
    /// `]`, the bracket string's delimiter, then `]`.
    Bracket(&'a [u8]),
}

/// The outcome of scanning one Hy string body.
#[derive(Debug, Clone, Copy)]
enum HyScan {
    /// Offset just past the closing delimiter.
    End(usize),
    Unterminated,
    Refused {
        position: usize,
        byte: u8,
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

    /// Whether `byte` separates tokens rather than belonging to one.
    ///
    /// Carp joins Clojure in treating a comma as whitespace: upstream's
    /// `emptyCharacters` is `[space, tab, comma, linebreak, eof, comment]`
    /// (`src/Parsing.hs`), so a comma is a separator there exactly as it is in
    /// Clojure. Without this it fell through to the shared quote-prefix table
    /// and read as an unquote, which is a reader macro Carp does not have.
    pub(super) const fn is_whitespace(self, byte: u8) -> bool {
        byte.is_ascii_whitespace()
            || matches!(self.dialect, Dialect::Clojure | Dialect::Carp) && byte == b','
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
            // `#!` followed by a space or a `/` is a Unix line comment in
            // Racket, and it is skipped in `read-char/skip-whitespace-and-
            // comments` beside `;` and `#|` rather than in `read-dispatch`:
            //
            // ```racket
            // [(and (char=? #\# ec)
            //       (eqv? #\! (readtable-effective-char/# rt (peek-char/special in config 0 source)))
            //       (let ([c3 (peek-char/special in config 1 source)])
            //         (or (eqv? #\space c3) (eqv? #\/ c3))))
            //  (skip-unix-line-comment! in config)
            // ```
            //
            // Two differences from the Emacs Lisp and Hy arms below, and both
            // come straight from that placement. It is *not* restricted to
            // offset 0 — being in the whitespace skipper means it applies
            // wherever a datum could start — and `skip-unix-line-comment!`
            // continues onto the next line when the byte before the newline is
            // a `\`, so the width has to be scanned rather than fixed. The
            // scan returns the full extent so `skip_trivia`'s "run to the next
            // newline" loop finds the cursor already there.
            //
            // Without this, `#!/bin/sh` and `#! /usr/bin/env racket` scanned as
            // junk atoms sitting at top level as if they were code — the same
            // silent defect the Hy arm below records, at exit 0. It is what
            // made 13 of 4492 real `.rkt` files disagree with Racket's own
            // reader about how many top-level forms they contain.
            Dialect::Racket if byte == b'#' => racket_unix_line_comment_width(bytes, pos),
            // An Emacs Lisp script starts `#!/usr/bin/emacs --script`, and
            // Emacs skips that line the way it skips a comment. Reading it as
            // one keeps the byte offsets of everything after it unchanged,
            // which stripping the line would not; and restricting it to
            // offset 0 keeps a stray `#!` anywhere else the reader error it
            // has always been.
            Dialect::EmacsLisp if pos == 0 && bytes.starts_with(b"#!") => Some(2),
            // Hy strips a shebang the same way, and under exactly the same
            // restriction. `HyReader.parse` peeks the first two characters of
            // the *stream* and, when `skip_shebang` is set, consumes to the
            // first newline; `hy.importer` and `hy2py` both set it, so this is
            // what reading a `.hy` file means. Offset 0 only: Hy's own peek
            // happens before any character is consumed, and `\n#!/usr/bin/env
            // hy` is rejected with "reader macro '#!/usr/bin/env' is not
            // defined", so a `#!` anywhere else stays the error it has always
            // been.
            //
            // Reading it as a line comment rather than stripping the line
            // keeps every later byte offset unchanged, which matters because
            // every rewrite in this workspace is a span replacement over the
            // original string.
            //
            // Without this, 393 of 2825 real `.hy` files parsed at exit 0 with
            // the shebang split into two junk atoms -- `#!/usr/bin/env` and
            // `hy` -- sitting at top level as if they were code.
            Dialect::Hy if pos == 0 && bytes.starts_with(b"#!") => Some(2),
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

    /// Where `|...|` reads as one symbol rather than a token boundary.
    ///
    /// R7RS 2.1 gives Scheme the same vertical-line notation Common Lisp has
    /// in CLHS 2.1.4.2, so `|Foo Bar|` is a single identifier in both, and in
    /// both a `|` may open a quoted region part-way through a token.
    ///
    /// LFE has the notation but not that second property, and the difference
    /// is explicit in `lfe_scan.erl`: `start_symbol_char($|) -> false` sends a
    /// leading `|` to `scan_qsymbol`, while `symbol_char/1` has no `$|` clause
    /// at all, so it falls through to `(C > $\s) and (C =< $~)` — true for
    /// `|` (124). A `|` inside a token is therefore an ordinary constituent
    /// and `a|b c|d` is the two symbols `a|b` and `c|d`, not one.
    pub(super) const fn bar_quoting(self) -> BarQuoting {
        match self.dialect {
            Dialect::CommonLisp | Dialect::Scheme | Dialect::Racket | Dialect::Unknown => {
                BarQuoting::Anywhere
            }
            Dialect::Lfe => BarQuoting::TokenStart,
            Dialect::EmacsLisp
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel => BarQuoting::None,
        }
    }

    /// Whether a bare `\` escapes the next character *outside* `|...|`.
    ///
    /// Deliberately narrower than [`Self::bar_quoting`]. Common Lisp's
    /// single-escape works anywhere in a token, so `a\ b` is one symbol;
    /// Scheme has no such rule, and its `\x41;` escapes are legal only inside
    /// a vertical-line region, which `consume_multiple_escape` handles on its
    /// own. Reading a stray `\` as an escape in Scheme would swallow the
    /// delimiter after it and unbalance the tree.
    ///
    /// ### Why Emacs Lisp belongs here
    ///
    /// Its reader has the same rule in both of the places a `\` can appear
    /// outside a string. `read0`'s symbol loop (`src/lread.c`) takes the byte
    /// after a `\` verbatim and marks the token `quoted`, so `'a\ b` is the
    /// one symbol whose name is `a b` and `'byte\[\]` is the one symbol
    /// `byte[]`; and `read_escape`, which handles the payload of a `?…`
    /// character literal, is *recursive* through its modifier prefixes, so
    /// `?\S-\ ` is super-space and `?\C-\[` is 27 — six and six bytes, one
    /// token each.
    ///
    /// Leaving Emacs Lisp out was not a missing nicety. `consume_atom_body`
    /// stopped at the byte *after* the backslash, so the literal's payload
    /// fell out of its own span and the formatter re-emitted the truncated
    /// token followed by whatever came next:
    ///
    /// ```text
    /// (if (= char ?\S-\ ) …)   ->   (if (= char ?\S-\) …)
    /// ```
    ///
    /// which Emacs then reads as super-`)` and runs off the end of the file.
    /// That is `isearch.el`; `elp.el` and `trace.el` lost the trailing space
    /// of `'ELP-instrumentation\ `, `soap-client.el` gained a space inside
    /// `byte\[\]`, and `?\C-\[` unbalanced the tree outright. Measured with
    /// Emacs's own reader as the oracle over the 1588 files of `lisp/` that
    /// `read` accepts, `edit format` changed what Emacs reads in 14 of them
    /// and made 7 unreadable, at exit 0.
    pub(super) const fn supports_single_escape(self) -> bool {
        matches!(
            self.dialect,
            Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Unknown
        )
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

    /// Whether a `"` ends the token before it rather than belonging to it.
    ///
    /// Racket and Emacs Lisp, and narrowly on purpose. Racket's
    /// `char-delimiter?` (`racket/src/expander/read/delimiter.rkt`) lists `"`
    /// beside whitespace and the brackets, so `(format"~a" x)` is the symbol
    /// `format` followed by a string — no space required, and Racket's own
    /// benchmark suite writes it that way. Emacs Lisp's `read0`
    /// (`src/lread.c`) ends a symbol on the same byte, through the
    /// `strchr ("\"';#()[]`,", c)` test its scanning loop runs per character,
    /// and `(read "(format\"x\" 1)")` returns `(format "x" 1)`.
    ///
    /// Reading it as one token was not merely coarse. The atom swallowed the
    /// opening quote, stopped at the space *inside* the literal, and the rest
    /// of the string became sibling atoms; `edit format` then re-emitted them
    /// as separate forms and put a line break inside what Racket reads as
    /// string data. That is silent corruption at exit 0, and it is what the
    /// corpus round trip against Racket's own reader caught in
    /// `benchmarks/shootout/wordfreq.rkt`.
    ///
    /// Emacs Lisp had the identical defect, found the identical way. Over the
    /// 1588 files of GNU Emacs's `lisp/` that `read` accepts, `edit format`
    /// changed what Emacs reads in `table.el`, `tpu-mapper.el` and
    /// `autoarg.el`, and each one is a `(format"…"` or `:lighter"…"` written
    /// without the space: the atom swallowed the opening quote, the closing
    /// one opened a *new* string, and everything between two literals swapped
    /// places with everything inside them. `table.el`'s
    /// `(format"    </%s>\n" …)` came back as `(format " </%s>\n" …)` — HTML
    /// indentation deleted from inside a string, at exit 0.
    ///
    /// Every dialect here except Hy terminates a token at `"` for the same
    /// reason, so this is still a general gap. It stays gated on the two
    /// dialects whose corpus has been audited against their own reader,
    /// because widening it is a behaviour change for eight others that needs
    /// each one's own audit — and because Hy is the counter-example that shows
    /// the rule cannot simply be made unconditional:
    /// [`Self::has_prefixed_strings`] exists because there an identifier
    /// immediately before a `"` really is a prefix on the literal.
    ///
    /// The other non-bracket delimiters both readers have — `'`, `` ` `` and
    /// `,`, plus `#` and `;` in Emacs Lisp — are deliberately still missing.
    /// They mis-read a token the same way, but they cannot corrupt one: the
    /// merged atom is re-emitted verbatim, so a format round trip over the
    /// corpus is byte-identical for them. Fixing them is worth doing on its
    /// own evidence, not as a rider here.
    pub(super) const fn string_terminates_a_token(self) -> bool {
        matches!(self.dialect, Dialect::Racket | Dialect::EmacsLisp)
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
                || (self.string_terminates_a_token() && *byte == b'"')
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

    /// Whether an identifier written immediately before `"` is a Python-style
    /// string prefix rather than a symbol.
    ///
    /// Hy only. `HyReader.read_default` (`hy/reader/hy_reader.py`) reads an
    /// identifier and then, if the very next character is `"`, hands that
    /// identifier to `prefixed_string` *as a prefix* instead of returning it
    /// as a symbol. So `r"a)b"` is one string literal, not the symbol `r`
    /// followed by anything.
    ///
    /// Missing this rule was the single largest reader defect for Hy: the atom
    /// scanner ran past the opening quote, stopped at the `)` *inside* the
    /// literal, and reported a stray closing delimiter. 417 of 469 parse
    /// failures over a 2825-file corpus were this one rule.
    pub(super) const fn has_prefixed_strings(self) -> bool {
        matches!(self.dialect, Dialect::Hy)
    }

    /// Whether `#[` opens a bracket string.
    ///
    /// Hy only, and unconditionally: `tag_dispatch` finds no identifier after
    /// the `#` (because `[` ends one), takes `[` as the tag, and dispatches to
    /// `bracketed_string`. There is no other reading of `#[` in Hy.
    ///
    /// This is why the arm matters so much more than its frequency suggests:
    /// `#[[...]]` was being read as a `HashLiteral` prefix on a nested bracket
    /// *list*, so `#[[(defn evil [] 1)]]` produced a real `defn` node — inside
    /// what is actually a raw string. Any of the 320 lint rules could fire on
    /// text that is not code.
    pub(super) const fn has_bracket_strings(self) -> bool {
        matches!(self.dialect, Dialect::Hy)
    }

    /// The width of a valid non-empty Hy string prefix at `pos`, when one is
    /// immediately followed by `"`.
    ///
    /// `prefixed_string` accepts a prefix whose characters are distinct, are a
    /// proper subset of `bfrt`, and include at most one of `b`/`f`/`t`. That
    /// admits `r`, `b`, `f`, `t` and the two-character pairs that add `r`, and
    /// rejects `bf`, `ff` and `bfrt`. A longer identifier such as `foo"` is a
    /// Hy error rather than a string; this returns `None` for it so the atom
    /// scanner keeps its existing behaviour instead of the reader inventing a
    /// literal where Hy has none.
    pub(super) fn hy_string_prefix_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        if !self.has_prefixed_strings() {
            return None;
        }
        hy_string_prefix_width_at(bytes, pos)
    }

    /// How far the Hy string literal starting at `pos` reaches, if one starts
    /// there.
    ///
    /// Covers both spellings: a quoted string with an optional prefix, and a
    /// `#[delim[...]delim]` bracket string. The whole literal, interpolations
    /// included, is one opaque atom -- see [`hy_string_extent_at`] for why the
    /// interpolated forms are deliberately not exposed as children.
    pub(super) fn hy_string_extent(self, bytes: &[u8], pos: usize) -> Option<HyStringExtent> {
        if !self.has_prefixed_strings() {
            return None;
        }
        hy_string_extent_at(bytes, pos, MAX_HY_STRING_NESTING)
    }

    /// Whether `#<<` opens a here string in this dialect.
    ///
    /// Racket only. `read-dispatch` (`racket/src/expander/read/main.rkt`)
    /// sends `#<` to a peek for a second `<` and nowhere else:
    ///
    /// ```racket
    /// [(#\<)
    ///  (define c2 (peek-char/special in config))
    ///  (cond
    ///   [(eqv? #\< c2)
    ///    (consume-char in #\<)
    ///    (read-here-string in config)]
    ///   [else
    ///    (reader-error in config #:due-to c2 "bad syntax `~a<`" dispatch-c)])]
    /// ```
    ///
    /// No Scheme has the form, which is why this is gated on the dialect
    /// rather than added to the shared `#`-dispatch table.
    pub(super) const fn has_here_strings(self) -> bool {
        matches!(self.dialect, Dialect::Racket)
    }

    /// How far the Racket here string starting at `pos` reaches, if one starts
    /// there.
    ///
    /// `read-here-string` (`racket/src/expander/read/string.rkt`) reads the
    /// terminator first — every character after `#<<` up to, but not
    /// including, the first newline — and then matches it against the content
    /// with a non-backtracking state machine seeded from
    /// `(cons #\newline tag)`:
    ///
    /// ```racket
    /// (let loop ([terminator (cdr full-terminator)] [terminator-accum null])
    ///   ...
    ///   [(and (null? terminator) (char=? c #\newline)) (void)]
    /// ```
    ///
    /// Four consequences, and each one is load-bearing:
    ///
    /// * The tag is the *whole* rest of the opening line, spaces included.
    ///   `#<<END ` and `#<<END` are different terminators.
    /// * The loop starts at `(cdr full-terminator)`, so the terminator may
    ///   match at the very first content byte: `#<<END\nEND\n` is the empty
    ///   string, not an unterminated literal.
    /// * A terminator line is the tag and nothing else. Leading whitespace
    ///   fails the match against `(car terminator)`; trailing whitespace fails
    ///   the `(char=? c #\newline)` test and falls into the `else` branch,
    ///   which flushes the matched characters back into the content. So
    ///   `  END` and `END ` are ordinary content lines.
    /// * The EOF branch is `(unless (null? terminator) (reader-error ...))`,
    ///   so a tag matched at the very end of input with no newline after it
    ///   *does* terminate. `#<<END\nx\nEND` at EOF is a complete literal.
    ///
    /// The state machine is equivalent to the scan below because a partial
    /// match can only ever consume tag characters, and a tag can never contain
    /// a newline — so no newline is ever swallowed by a failed partial match,
    /// and every position after a newline is tried.
    ///
    /// A tag that never reappears is [`HereStringExtent::Unterminated`] rather
    /// than a literal running to EOF, for the reason
    /// [`Self::long_string_extent`] gives: an atom that swallows the rest of
    /// the file is silent corruption, and Racket refuses the same input with
    /// "found end-of-file before terminating".
    ///
    /// ### Why the terminating newline is inside the span
    ///
    /// Racket consumes it, and so must this: a here string is the one literal
    /// here that is not self-delimiting. Its terminator is only a terminator
    /// when a newline or EOF follows, so an extent that stopped one byte
    /// earlier would let the formatter emit `(list #<<E ... E)` — the closing
    /// paren landing on the terminator line and quietly turning a complete
    /// literal into an unterminated one.
    pub(super) fn here_string_extent(self, bytes: &[u8], pos: usize) -> Option<HereStringExtent> {
        if !self.has_here_strings() || bytes.get(pos..pos + 3) != Some(HERE_STRING_OPEN) {
            return None;
        }
        Some(here_string_extent_at(bytes, pos))
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
            Dialect::Scheme | Dialect::Racket | Dialect::Lfe
                if byte == b'#' && next == Some(b'\\') =>
            {
                2
            }
            Dialect::Clojure if byte == b'\\' => 1,
            // Carp spells a character literal `\a`, like Clojure. Upstream's
            // `aChar` is `Parsec.char '\\'` followed by one of the named
            // characters (`space`, `newline`, `tab`, `backspace`, `return`,
            // `formfeed`), `\"`, `\uXXXX`, or `Parsec.anyChar` -- so the
            // payload is always at least one character and never a delimiter
            // boundary. Without this arm `\{`, `\}`, `\(`, `\)`, `\[`, `\]`
            // and `\"` had their payload read as a *real* delimiter, which is
            // what made `core/Format.carp` and `examples/json_parser.carp`
            // fail to parse outright. The named spellings need no arm of their
            // own: consuming `\` plus one character leaves `pace` of `\space`
            // to the atom scanner, which stops at the same boundary and yields
            // the one atom `\space`.
            Dialect::Carp if byte == b'\\' => 1,
            Dialect::EmacsLisp if byte == b'?' && next == Some(b'\\') => 2,
            Dialect::EmacsLisp if byte == b'?' => 1,
            _ => return None,
        };
        bytes.get(pos + width).is_some().then_some(width)
    }

    /// The total byte width of the character literal at `pos`, for a dialect
    /// whose character-literal grammar is modelled exactly.
    ///
    /// `None` means either that no literal starts here or that this dialect
    /// has no exact model, and the caller falls back to
    /// [`Self::character_literal_prefix_width`] plus one character plus a scan
    /// to the next atom boundary.
    ///
    /// Emacs Lisp is the only dialect modelled exactly, because it is the only
    /// one here whose literal can *contain* what would otherwise end a token.
    /// `read_char_escape` (`src/lread.c`) reads the payload after a modifier
    /// prefix with a bare `READCHAR` and takes whatever comes back verbatim:
    ///
    /// ```c
    ///   mod_key:
    ///     {
    ///       int c1 = READCHAR;
    ///       if (c1 != '-') { ... }
    ///       modifiers |= mod;
    ///       c1 = READCHAR;
    ///       if (c1 == '\\') { next_char = READCHAR; goto again; }
    ///       chr = c1;
    ///       break;
    ///     }
    /// ```
    ///
    /// So `?\C- ` is control-space and `?\C-]` is 29 — a space and a closing
    /// bracket that are payload, not boundary. Scanning to the next atom
    /// boundary cannot find the end of those, however the backslash rule is
    /// written, which is why [`Self::supports_single_escape`] is necessary for
    /// Emacs Lisp but not sufficient:
    ///
    /// ```text
    /// (define-key map [?\C- ] 'kkc-first-char-only)
    ///   ->  (define-key map [?\C-] 'kkc-first-char-only)
    /// ```
    ///
    /// which Emacs then reads as control-`]` with the vector left open.
    /// `bindings.el`, `kkc.el`, `korea-util.el` and `ns-win.el` are all this
    /// one shape.
    ///
    /// The caller advances past this width and then keeps scanning to the next
    /// atom boundary rather than ending the token here, which is what makes
    /// the new extent a superset of the old one for every input;
    /// `Parser::consume_character_literal` records why that matters and what
    /// terminating instead broke.
    ///
    /// | source | payload | width |
    /// |---|---|---|
    /// | `?a` `?あ` `?{` | the character itself | 1 + its UTF-8 length |
    /// | `?\n` `?\e` `?\;` `?\)` `?\ ` | the character after the `\` | 2 + its UTF-8 length |
    /// | `?\x41` | every hex digit, at least one | scanned |
    /// | `?\u00e9` `?\U0001F600` | exactly 4 / 8 hex digits | 7 / 11 |
    /// | `?\N{U+261D}` `?\N{OGHAM SPACE MARK}` | up to the `}` | scanned |
    /// | `?\101` | at most three octal digits | scanned |
    /// | `?\C-` `?\M-` `?\S-` `?\H-` `?\A-` `?\s-` `?\^` | a whole payload again | recursive |
    ///
    /// A spelling Emacs itself refuses — `?\x` with no digit, `?\Ca`, `?\N`
    /// with no brace, a truncated literal at end of input — returns `None` and
    /// leaves the old scan in place rather than inventing a refusal for input
    /// no reader accepts anyway.
    pub(super) fn exact_character_literal_width(self, bytes: &[u8], pos: usize) -> Option<usize> {
        if !matches!(self.dialect, Dialect::EmacsLisp) || bytes.get(pos) != Some(&b'?') {
            return None;
        }
        emacs_lisp_character_payload_end(bytes, pos + 1).map(|end| end - pos)
    }

    /// Whether a character literal is *exactly* its prefix plus one character,
    /// so the token ends there rather than running on to the next boundary.
    ///
    /// LFE is the only dialect here where it is. `lfe_scan.erl` spells the
    /// whole grammar in one clause:
    ///
    /// ```erlang
    /// scan_hash2([$\\,C|Cs], Line, Col, [], St) ->
    ///     {ok,{number,Line,C},Cs,Line,Col+2,St};
    /// ```
    ///
    /// One character, taken verbatim. There are no named characters, and no
    /// escape processing at all — `#\n` is the letter `n` (110), not a newline.
    /// So `#\"abc"` is the character `"` followed by the string `abc`, and
    /// running the token on to the next boundary instead would swallow the
    /// string's opening quote and glue the rest of the file into one atom.
    ///
    /// Everywhere else the name may be longer than one character — Scheme's
    /// `#\space`, Emacs Lisp's `?\C-x`, Clojure's `\newline` — so the token has
    /// to keep scanning, and nothing below may change for them.
    pub(super) const fn character_literal_is_exactly_one_char(self) -> bool {
        matches!(self.dialect, Dialect::Lfe)
    }

    pub(super) fn classify_reader_macro(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let third = bytes.get(pos + 2).copied();

        match self.dialect {
            Dialect::Unknown => self.classify_legacy(byte, next, third),
            Dialect::Lfe => self.classify_lfe(bytes, pos),
            Dialect::Hy => self.classify_hy(byte, next, third),
            Dialect::Carp => Self::classify_carp(byte, next),
            Dialect::CommonLisp => self.classify_common_lisp(bytes, pos),
            Dialect::EmacsLisp => self.classify_emacs_lisp(bytes, pos),
            Dialect::Scheme => self.classify_scheme(bytes, pos),
            Dialect::Racket => self.classify_racket(bytes, pos),
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

    /// LFE's `#`-dispatch table, from `lfe_scan.erl`'s `scan_hash1`/`scan_hash2`.
    ///
    /// That is the complete set, in the scanner's own order. `scan_hash`
    /// collects decimal digits first (`scan_hash_digits`), and every form
    /// except `#<digits>r` requires that digit run to be empty:
    ///
    /// | source        | token                     | handled by                    |
    /// |---------------|---------------------------|-------------------------------|
    /// | `#(`          | `'#('` tuple open         | [`classify_shared_prefix`]    |
    /// | `#B(` `#b(`   | `'#B('` binary open       | this arm                      |
    /// | `#M(` `#m(`   | `'#M('` map open          | this arm                      |
    /// | `#S(` `#s(`   | `'#S('` struct open       | this arm                      |
    /// | `#"…"`        | `binary` string           | this arm                      |
    /// | `#\C`         | `{number,_,C}` one char   | `character_literal_prefix_width` |
    /// | `#'f/2`       | `'#\''` fun reference     | [`classify_shared_prefix`]    |
    /// | `#.`          | `'#.'` read-eval          | [`classify_shared_prefix`]    |
    /// | `#\|`          | block comment             | `supports_block_comments`      |
    /// | `#*1010`      | base-2 number             | scans as a plain atom         |
    /// | `#b…` `#o…` `#d…` `#x…` | based numbers   | scan as plain atoms           |
    /// | `#<digits>r…` | base-2..36 number         | scans as a plain atom         |
    /// | `` #` `` `#;` `#,` `#,@` | scanned, no grammar production | left as they were  |
    ///
    /// The based-number forms need nothing: `#x1f` has no delimiter in it, so
    /// the ordinary atom scanner already takes the whole token. The last row is
    /// deliberately untouched — `lfe_parse.spell1` declares no production for
    /// those tokens, so LFE itself refuses them, and the existing readings are
    /// neither more nor less wrong than they were.
    ///
    /// Everything else falls through to [`Self::classify_legacy`], which is
    /// what LFE used before this function existed, so no reading changes except
    /// the ones named above.
    fn classify_lfe(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let third = bytes.get(pos + 2).copied();

        if byte != b'#' {
            return self.classify_legacy(byte, next, third);
        }

        // `#B(`, `#M(`, `#S(` are single opening tokens in `scan_hash2`, each
        // closed by a plain `)` in `lfe_parse.spell1`. Reading the two-byte
        // dispatch as a prefix on the list that follows gives exactly that
        // shape, and is how `#(` has always been read. Without it the `#B`
        // scanned as its own atom and the list became a *sibling*, so
        // `(f #B(1 2) X)` had four children where LFE sees three -- silently,
        // at exit 0, which is why this is the defect that matters most.
        if third == Some(b'(') {
            match next {
                Some(b'b' | b'B') => return prefix(ReaderPrefix::LfeBinary, 2),
                Some(b'm' | b'M') => return prefix(ReaderPrefix::LfeMap, 2),
                Some(b's' | b'S') => return prefix(ReaderPrefix::LfeStruct, 2),
                _ => {}
            }
        }

        // `#"…"` is one `binary` token (`scan_hash1([$"|Cs], …)` hands
        // straight to `scan_binary_string`). Treating the `#` as a prefix on
        // the string that follows keeps the whole literal in one node, where
        // before the `#` glued onto the string's *first word* and every later
        // word became a sibling atom: `#"text/plain; version=0.0.4"` split
        // into three, and one containing a `)` closed its enclosing list early.
        if next == Some(b'"') {
            return prefix(ReaderPrefix::HashLiteral, 1);
        }

        // `#\` with nothing after it is a truncated character literal.
        // Refusing it is what stops the formatter's trailing newline from
        // becoming the literal's character on the next parse, which would make
        // `format(format(x))` differ from `format(x)`. Scheme and Racket
        // already refuse the same input for the same reason; LFE's own scanner
        // refuses it too, as `{illegal_token,"#\\"}`.
        if next == Some(b'\\') && third.is_none() {
            return Some(ReaderMacro::UnsupportedDispatch { width: 1 });
        }

        self.classify_legacy(byte, next, third)
    }

    /// Hy's reader macros.
    ///
    /// Split out of [`Self::classify_legacy`] so `#[` can stop being a
    /// `HashLiteral` prefix. In Hy it opens a bracket string, so the shared arm
    /// turned a raw string's contents into real nodes;
    /// [`Self::has_bracket_strings`] describes what that cost. `#(` and `#{`
    /// keep the shared reading: they are Hy's tuple and set literals, which
    /// really do contain forms.
    ///
    /// ### Why `~` is *not* a prefix here yet
    ///
    /// `~` is Hy's unquote and `~@` its unquote-splice (`@reader_for("~")`
    /// returns `(unquote ...)`, or `(unquote-splice ...)` when an `@`
    /// follows), so a Hy `~foo` coming out as the bare atom `~foo` is wrong:
    /// `QuoteState`'s quasiquote counter never comes back down, and every form
    /// inside a Hy `` ` `` is treated as data. It suppresses findings rather
    /// than inventing them.
    ///
    /// Adding the two arms is a three-line change and it *was* implemented and
    /// measured. It is left out because it is not safe to ship on its own:
    /// several formatter paths open a child list by pushing `delimiter.open()`
    /// directly, without first writing that child's reader prefixes, so a
    /// prefixed list reaching one of them is emitted with its prefix deleted.
    /// `format_body_clause` and `format_sequence_list` are two; there are more.
    ///
    /// That bug is already live — `#(...)` in a Hy `cond` loses its `#` today,
    /// turning a tuple into a call — but it is rare, because `#(` in clause
    /// position is rare. Making `~` a prefix would aim it straight at the
    /// single most common construct in Hy macro code. Measured over the 2825
    /// file corpus with Hy's reader as the oracle, `edit format` newly changed
    /// the *meaning* of 14 files that it had previously formatted correctly,
    /// every one of them a dropped `~`/`~@`.
    ///
    /// So this waits on a formatter fix. It is a bug family across five files
    /// in `sexpr::formatter`, it affects every dialect, and it needs its own
    /// golden review — not a rider on a reader change.
    ///
    /// ### `,` is not a reader macro in Hy
    ///
    /// Hy's `@reader_for` table has no `,` entry, and `NON_IDENT` — the
    /// complete set of identifier terminators — is `set("()[]{};\"'`~")`,
    /// which lists `~` and not `,`. So `,bar` is *one symbol*, not an unquote
    /// of `bar`, and classifying `,` as a prefix was wrong at the root rather
    /// than merely at a closing delimiter.
    ///
    /// It stayed invisible for as long as a dangling prefix was tolerated,
    /// because the mis-parse only changed the tree's shape. Once a prefix with
    /// no following form became a hard `MissingReaderForm`, it started refusing
    /// ordinary Hy outright:
    ///
    /// * `(,)` — an expression whose single element is the symbol `,`, which
    ///   Hy's tuple constructor uses for the empty tuple.
    /// * A trailing comma before `}` or `]`, as in `{"a" 1 ,}` — idiomatic in
    ///   Hy's Python-flavoured dict and list literals.
    ///
    /// An earlier version of this comment claimed a leading `,` "occurs at a
    /// token start essentially never". Over 2825 real `.hy` files those two
    /// shapes account for **13 outright parse failures**, including Hy's own
    /// `contrib/walk.hy`, `hylang/simalq` and `kanaka/mal`.
    ///
    /// Dropping the comma is enough on its own: `is_atom_boundary` never
    /// treated `,` as a terminator, so `,bar` and `1,` already scanned as
    /// single atoms and now agree with Hy at token start too. `'` and `` ` ``
    /// really are Hy reader macros taking exactly one following form, so a
    /// closing delimiter after one is still refused — this corrects *which*
    /// characters are prefixes, not what a prefix requires.
    ///
    /// * `#;`, `#+` and `#-` are not Hy reader macros at all.
    const fn classify_hy(
        self,
        byte: u8,
        next: Option<u8>,
        third: Option<u8>,
    ) -> Option<ReaderMacro> {
        match (byte, next) {
            (b'#', Some(b'[')) => None,
            (b',', _) => None,
            _ => self.classify_legacy(byte, next, third),
        }
    }

    /// Carp's reader macros, from the `sexpr` dispatch in `src/Parsing.hs`.
    ///
    /// Carp used to share [`Self::classify_legacy`] with LFE, Hy and the
    /// permissive reader, and the two grammars have almost nothing in common.
    /// Upstream dispatches on a single lookahead character:
    ///
    /// ```text
    /// sexpr = do
    ///   c <- Parsec.lookAhead Parsec.anyChar
    ///   x <- case c of
    ///     '&' -> ref
    ///     '~' -> deref
    ///     '@' -> copy
    ///     '\'' -> quote
    ///     '`' -> quasiquote
    ///     '%' -> Parsec.try unquoteSplicing <|> unquote
    ///     '(' -> list
    ///     '[' -> array
    ///     '$' -> staticArray
    ///     '{' -> dictionary
    ///     _ -> atom
    /// ```
    ///
    /// where `readerMacro` consumes the sigil and then recurses into `sexpr`,
    /// so a sigil prefixes *any* following form -- symbol, list, string
    /// literal, or another prefixed form (`@@x`, `&@x`). None of `& ~ @ % $ #`
    /// is in upstream's `validCharacters`, so none can occur inside a symbol
    /// and each is unambiguously a prefix wherever it appears.
    ///
    /// Missing these cost more than tidiness. `&` and `@` alone accounted for
    /// 1493 bare sigil atoms across 116 of the 248 files in `carp-lang/Carp`,
    /// each one an extra sibling that inflated its enclosing call's arity, so
    /// no argument-counting analysis was sound for Carp.
    ///
    /// Two upstream forms are deliberately still not implemented here:
    ///
    /// * `%` / `%@` (unquote, unquote-splicing). Recognizing them would make
    ///   the interior of every `` ` `` template read as code rather than data,
    ///   which *adds* lint findings on macro bodies. That direction needs a
    ///   per-dialect false-positive audit of its own, and leaving it reads
    ///   templates as inert data -- the suppressing, safe direction.
    /// * `{...}` dictionaries desugar to `(Map.from-array [(Pair.init k v)…])`
    ///   in upstream's reader. Structurally they are already a brace list here,
    ///   which is the right shape for a structural tool; the desugaring is a
    ///   semantic concern.
    const fn classify_carp(byte: u8, next: Option<u8>) -> Option<ReaderMacro> {
        match byte {
            // A trailing `\` is a truncated character literal, not a symbol.
            // Same contract `classify_clojure`, `classify_scheme` and
            // `classify_emacs_lisp` already state for their own spellings: the
            // formatter appends a trailing newline, a `\` left as an atom
            // claims that newline as its character on the next parse, and
            // `format(format(x))` stops converging. The recorded fuzz input
            // `fuzz/corpus/format_idempotence/truncated-clojure-char` is
            // exactly this byte and found it here too.
            b'\\' if next.is_none() => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
            b'&' => prefix(ReaderPrefix::Ref, 1),
            b'@' => prefix(ReaderPrefix::Copy, 1),
            b'~' => prefix(ReaderPrefix::Deref, 1),
            b'\'' => prefix(ReaderPrefix::Quote, 1),
            b'`' => prefix(ReaderPrefix::Quasiquote, 1),
            // `staticArray` matches the two-byte string `$[`; a `$` anywhere
            // else is a parse error upstream, and reading it as an atom here
            // is the more permissive of the two options.
            b'$' if matches!(next, Some(b'[')) => prefix(ReaderPrefix::StaticArray, 1),
            // `#"…"` is a `Pattern` literal and the only `#` form Carp has.
            // One dispatch byte plus exactly one datum, which is how
            // `classify_clojure` reads its `#"…"` regex literal too, so both
            // go through the same string scanner. Upstream's
            // `parseInternalPattern` accepts a `"` only as `\"` and every
            // other escape as backslash-plus-one, so a backslash-aware scan
            // ends the literal on exactly the byte upstream ends it on.
            b'#' if matches!(next, Some(b'"')) => Some(ReaderMacro::MultiDatum {
                width: 1,
                payload_forms: 1,
            }),
            // `#"` is the *only* `#` form Carp has. `#` is absent from
            // upstream's `validCharacters`, so it cannot occur inside a symbol
            // either, and `atom` has no branch that accepts it: `#` anywhere
            // else is a parse error upstream. Refusing it here keeps that,
            // and keeps the robustness suite's out-of-band oracle -- "no
            // complete document contains a bare `#+`/`#-` standing alone as
            // its own datum" -- true for Carp rather than exempting it.
            // Carp inherited `#;`, `#_`, `#+`, `#-`, `#.`, `#'`, `#?`, `#(`,
            // `#[` and `#{` from the legacy reader; it has none of them.
            b'#' => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
            _ => None,
        }
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
        // A well-formed radix integer is not a reader macro at all. `#x1f` has
        // no delimiter in it, so returning `None` lets the ordinary atom
        // scanner take the whole token -- which is exactly how
        // [`Self::classify_common_lisp`] and [`Self::classify_scheme`] already
        // read their own `#x`/`#b`/`#o` spellings, and why neither of them ever
        // had this defect. See [`emacs_lisp_radix_literal_is_valid`] for the
        // grammar and for why a malformed one is deliberately left to the
        // refusal below.
        if emacs_lisp_radix_literal_is_valid(bytes, pos) {
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

    /// Racket's `#`-dispatch table, from `read-dispatch` in
    /// `racket/src/expander/read/main.rkt`.
    ///
    /// Racket used to share [`Self::classify_scheme`], and the two tables have
    /// diverged far enough that sharing was the defect: **1777 of 4492 real
    /// `.rkt` files (39.6%) failed to parse** over `racket/racket` plus
    /// `racket/typed-racket`, every one of them an
    /// `UnsupportedReaderDispatch { dispatch: "#" }`. A file that does not
    /// parse is a file none of this workspace's commands can say anything
    /// about, so this capped every Racket lint rule at 60% of its corpus.
    ///
    /// [`Self::classify_scheme`] is left byte-identical. Several of the forms
    /// below — `#'`, `` #` ``, `#,`, `#,@` — are R6RS lexical syntax that Guile,
    /// Chez and Chicken all accept, so Scheme has a real gap here too, but
    /// closing it is a change to a *different* dialect's reader that needs its
    /// own Scheme corpus audit rather than a rider on this one.
    ///
    /// | source | reading | handled by |
    /// |---|---|---|
    /// | `#;` | datum comment | this arm |
    /// | `#(` `#[` `#{` | vector | [`ReaderPrefix::HashLiteral`] |
    /// | `#3(…)` `#fx(…)` `#fl6(…)` | sized / fixnum / flonum vector | [`racket_vector_dispatch_width`] |
    /// | `#0=` `#0#` | graph definition and reference | [`classify_numeric_dispatch`] |
    /// | `#s(…)` | prefab struct | this arm |
    /// | `#&x` | box | this arm |
    /// | `#'x` | `syntax` | [`ReaderPrefix::Function`] |
    /// | `` #`x `` `#,x` `#,@x` | `quasisyntax` / `unsyntax` / `unsyntax-splicing` | this arm |
    /// | `#\c` | character | `character_literal_prefix_width` |
    /// | `#"…"` | byte string | [`ReaderPrefix::HashLiteral`] |
    /// | `#<<TAG` | here string | [`Self::here_string_extent`] |
    /// | `#%app` | *a symbol named* `#%app` | scans as a plain atom |
    /// | `#:mode` | keyword | scans as a plain atom |
    /// | `#t` `#f` `#true` `#false` | booleans | scan as plain atoms |
    /// | `#e` `#i` `#d` `#b` `#o` `#x` | number prefixes | scan as plain atoms |
    /// | `#hash(…)` `#hasheq(…)` `#hasheqv(…)` `#hashalw(…)` | hash literal | [`racket_hash_dispatch_width`] |
    /// | `#rx"…"` `#px"…"` `#rx#"…"` | regexp | this arm |
    /// | `#ci…` `#cs…` | case-folding dispatch | this arm |
    /// | `#lang …` `#!…` | language directive | `line_comment_width` / plain atom |
    /// | `#reader` `#~` `#2d…` | reader extension, compiled code, 2D syntax | refused loudly |
    ///
    /// ### `#%foo` is an identifier, not a dispatch
    ///
    /// This is the one entry where guessing would have put the fix in the
    /// wrong place entirely. `read-dispatch` sends `%` to the *symbol* reader:
    ///
    /// ```racket
    /// [(#\%)
    ///  (read-symbol-or-number c in config #:extra-prefix dispatch-c #:mode 'symbol)]
    /// ```
    ///
    /// `#:extra-prefix` seeds the accumulator with the `#`, so `#%app` reads as
    /// the symbol whose name is the four characters `#%app` — `'#%kernel`,
    /// `#%module-begin` and `#%plain-lambda` are ordinary identifiers that
    /// happen to start with `#%`. Returning `None` here hands the whole token
    /// to the atom scanner, which already stops in the right place because `#`
    /// and `%` are not atom boundaries. There is no reader macro to add.
    ///
    /// ### Why `` #` ``/`#,`/`#,@` are opaque while `#'` keeps its child
    ///
    /// [`ReaderPrefix`] carries a fixed `as_source` spelling and three
    /// re-emission paths write it straight back into source text, so a prefix
    /// may only be used where a spelling already exists. `#'` has one —
    /// `ReaderPrefix::Function`, which `quote_edit::quote_operators` already
    /// documents as "not the `#'` reader macro" and deliberately confines its
    /// `(function x)` longhand to Common Lisp and Emacs Lisp precisely because
    /// "Scheme and Racket read it as `syntax`, not `function`". So `#'x` keeps
    /// `x` visible to rename, reference tracking and every lint rule.
    ///
    /// `` #` ``, `#,` and `#,@` have no spelling in that enum. Giving them one
    /// means three new variants, and `reader::apply_reader_prefix_context`
    /// matches [`ReaderPrefix`] *exhaustively* — it is a shared table, in the
    /// crate every dialect and all 347 lint rules depend on. Reading them as
    /// opaque reader forms instead needs no shared edit, and it is also the
    /// suppressing direction: the interior of a `` #`(…) `` template is read as
    /// inert data rather than as live code that could invent findings. The
    /// alternative today is not "visible" but "the file does not parse at all",
    /// so blind still beats wrong. Promoting them to real prefixes is a
    /// worthwhile follow-up with its own false-positive audit.
    fn classify_racket(self, bytes: &[u8], pos: usize) -> Option<ReaderMacro> {
        let byte = *bytes.get(pos)?;
        let next = bytes.get(pos + 1).copied();
        let third = bytes.get(pos + 2).copied();

        if byte == b'#' && next == Some(b';') {
            return Some(ReaderMacro::Discard { width: 2 });
        }
        if let Some(prefix) = classify_quote_prefix(byte, next) {
            return Some(prefix);
        }
        if byte != b'#' {
            return None;
        }
        // `#0=`/`#0#` first: `read-vector-or-graph` collects the digit run
        // before it decides, and `#3(` below shares that run.
        if let Some(dispatch) = classify_numeric_dispatch(bytes, pos, false) {
            return Some(dispatch);
        }
        if let Some(width) = racket_vector_dispatch_width(bytes, pos) {
            return Some(ReaderMacro::MultiDatum {
                width,
                payload_forms: 1,
            });
        }
        match next {
            // A vector, a byte string, or a bracketed vector: one dispatch
            // byte glued to the literal that follows, which is exactly what
            // `HashLiteral` spells. Keeping the elements visible rather than
            // opaque matters — `#(a b)` really does contain data a rule may
            // want to read.
            Some(b'(' | b'[' | b'{' | b'"') => prefix(ReaderPrefix::HashLiteral, 1),
            Some(b'\'') => prefix(ReaderPrefix::Function, 2),
            Some(b'`') => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
            // `#,@` is three bytes, not two: `read-dispatch`'s `#\,` arm peeks
            // for an `@` and consumes it before delegating to `read-quote`.
            // Both spellings take exactly one following datum.
            Some(b',') => Some(ReaderMacro::MultiDatum {
                width: 2 + usize::from(third == Some(b'@')),
                payload_forms: 1,
            }),
            // `#\` with nothing after it is a truncated character literal, and
            // is refused for the reason `classify_scheme` gives: the formatter
            // appends a trailing newline, a truncated literal claims it as its
            // character, and `format(format(x))` stops converging.
            Some(b'\\') if third.is_none() => Some(ReaderMacro::UnsupportedDispatch { width: 1 }),
            // `#\c`, `#%app`, `#:mode`, `#t`/`#f`/`#true`/`#false`, the number
            // prefixes, and `#!lang`: all scan as plain atoms.
            Some(
                b'\\' | b'%' | b':' | b'!' | b't' | b'T' | b'f' | b'F' | b'e' | b'E' | b'i' | b'I'
                | b'd' | b'D' | b'b' | b'B' | b'o' | b'O' | b'x' | b'X',
            ) => None,
            // `#&x` is a box around exactly one datum (`read-box`), and
            // `#s(…)` a prefab struct whose description is one sequence
            // (`read-struct`). `read-struct`'s `case` has no `#\S` clause, so
            // an upper-case `#S(` really is "bad syntax" in Racket and falls
            // through to the refusal below.
            Some(b'&') => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
            Some(b's') if opens_sequence(bytes, pos + 2) => Some(ReaderMacro::MultiDatum {
                width: 2,
                payload_forms: 1,
            }),
            // The here string's extent is a scan, not a width, so it is left
            // to `here_string_extent` through the atom path. A `#<` without
            // the second `<` is Racket's own "bad syntax `#<`".
            Some(b'<') if third == Some(b'<') => None,
            Some(b'h' | b'H') => racket_hash_dispatch_width(bytes, pos)
                .map(|width| ReaderMacro::MultiDatum {
                    width,
                    payload_forms: 1,
                })
                .or(Some(ReaderMacro::UnsupportedDispatch { width: 1 })),
            // `#rx"…"`, `#px"…"` and their byte-string spellings `#rx#"…"`,
            // `#px#"…"`. `read-regexp` accepts nothing else after the `x` —
            // "expected `\"` or `#`" — so the literal that follows is required
            // rather than assumed, and `#re` (the start of `#reader`) falls
            // through to the refusal below.
            //
            // The payload is an *ordinary* string literal: `read-regexp` calls
            // the same `read-string` the `"` dispatch does, so `\"` closes
            // nothing and `#rx"\"[a-z]\""` is one datum. Reading it raw would
            // have ended the literal at the first escaped quote.
            Some(b'r' | b'p') if third == Some(b'x') && opens_regexp_payload(bytes, pos + 3) => {
                Some(ReaderMacro::MultiDatum {
                    width: 3,
                    payload_forms: 1,
                })
            }
            // `#cs`/`#ci` (either case, both letters) set case sensitivity for
            // exactly one following datum.
            Some(b'c' | b'C') if matches!(third, Some(b'i' | b'I' | b's' | b'S')) => {
                Some(ReaderMacro::MultiDatum {
                    width: 3,
                    payload_forms: 1,
                })
            }
            // `#reader` extends the reader with an arbitrary module's grammar,
            // `#~` is compiled code, and `#2d…` needs the `2d` readtable.
            // None of the three has a fixed extent this reader could scan, so
            // each stays the loud refusal it already was rather than becoming
            // a guess about where the form ends.
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

/// Whether `byte` ends a Hy identifier (`HyReader.NON_IDENT`).
///
/// Note what is *absent*: `#`, `,`, `:`, `!` and `@` are all ordinary
/// identifier constituents in Hy, which is why `a,` is the single symbol `a,`
/// and why `f"{x:>10}"` interpolates the symbol `x:>10` rather than applying a
/// format spec.
const fn is_hy_non_ident(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b';' | b'"' | b'\'' | b'`' | b'~'
        )
}

/// The width of a valid non-empty Hy string prefix at `pos` followed by `"`.
fn hy_string_prefix_width_at(bytes: &[u8], pos: usize) -> Option<usize> {
    // A valid prefix is one or two bytes: `prefixed_string` requires distinct
    // characters, a *proper* subset of `bfrt`, and at most one of `b`/`f`/`t`.
    for width in 1..=2usize {
        let candidate = bytes.get(pos..pos + width)?;
        if bytes.get(pos + width) != Some(&b'"') {
            continue;
        }
        if !candidate
            .iter()
            .all(|byte| HY_STRING_PREFIX_BYTES.contains(byte))
        {
            continue;
        }
        if candidate[0] == *candidate.last().expect("candidate is non-empty") && width == 2 {
            // Duplicate characters: `prefix_chars` would be shorter than
            // `prefix`, which `prefixed_string` rejects (`ff"..."`).
            continue;
        }
        if candidate.iter().filter(|byte| **byte != b'r').count() > 1 {
            // Two of `b`/`f`/`t` together, such as `bf"..."`.
            continue;
        }
        return Some(width);
    }
    None
}

/// Whether the prefix bytes at `pos` select f-string (interpolating) mode.
fn hy_prefix_is_fstring(prefix: &[u8]) -> bool {
    prefix.iter().any(|byte| matches!(byte, b'f' | b't'))
}

/// How far the complete Hy string literal starting at `pos` reaches.
///
/// ### Why the whole literal is one opaque atom
///
/// A Hy f-string genuinely contains code: `read_fcomponent` calls
/// `parse_one_form`, so `f"{(get d \"k\")}"` holds a real function call, and
/// this scanner has to understand that sub-language just to find the closing
/// quote. It would therefore be possible to expose the interpolated forms as
/// children. That is deliberately not done, for three reasons.
///
/// * Hy models it as a literal. The reader returns `FString`, a value-
///   producing model, not program structure.
/// * There is no node kind for it. `ExpressionKind` is `Root`, `List` or
///   `Atom`; interleaved literal text and forms fits none of them, and adding
///   a kind changes a crate shared by the formatter, the edit engine and 320
///   lint rules.
/// * Children are *editable*, and that is the danger. The formatter reindents
///   children; reindenting inside an f-string rewrites the literal segments
///   between the interpolations and silently changes what the program prints.
///   That is the same failure this fix removes for `#[[...]]` — code visible
///   where there is none — and re-introducing it deliberately would trade one
///   silent-corruption class for another.
///
/// Blind beats wrong, and the alternative today is worse than blind: the file
/// does not parse at all, so no rule sees any of it.
fn hy_string_extent_at(bytes: &[u8], pos: usize, budget: usize) -> Option<HyStringExtent> {
    let scan = hy_string_scan_at(bytes, pos, budget)?;
    Some(match scan {
        HyScan::End(end) => HyStringExtent::Closed { width: end - pos },
        HyScan::Unterminated => HyStringExtent::Unterminated,
        HyScan::Refused { position, byte } => HyStringExtent::Refused { position, byte },
    })
}

/// Scans a complete Hy string literal at `pos`, in either spelling.
fn hy_string_scan_at(bytes: &[u8], pos: usize, budget: usize) -> Option<HyScan> {
    if budget == 0 {
        return Some(HyScan::Unterminated);
    }
    if bytes.get(pos) == Some(&b'#') && bytes.get(pos + 1) == Some(&b'[') {
        return Some(hy_bracket_string_scan(bytes, pos, budget));
    }
    let prefix_width = hy_string_prefix_width_at(bytes, pos).unwrap_or(0);
    if bytes.get(pos + prefix_width) != Some(&b'"') {
        return None;
    }
    let prefix = &bytes[pos..pos + prefix_width];
    Some(hy_string_body_scan(
        bytes,
        pos + prefix_width + 1,
        hy_prefix_is_fstring(prefix),
        HyCloser::Quote,
        budget,
    ))
}

/// Scans a `#[delim[...]delim]` bracket string whose `#[` is at `pos`.
///
/// The delimiter is every byte up to the second `[`; a `]` there is the error
/// Hy raises as "Ran into a ']' where it wasn't expected". A delimiter of `f`
/// or one starting `f-` makes the body an f-string. `t` does not, because
/// `bracketed_templates` is off in the default reader.
fn hy_bracket_string_scan(bytes: &[u8], pos: usize, budget: usize) -> HyScan {
    let delim_start = pos + 2;
    let mut cursor = delim_start;
    loop {
        let Some(&byte) = bytes.get(cursor) else {
            return HyScan::Unterminated;
        };
        if byte == b'[' {
            break;
        }
        if byte == b']' {
            return HyScan::Refused {
                position: cursor,
                byte,
            };
        }
        cursor += 1;
    }
    let delim = &bytes[delim_start..cursor];
    let fstring = delim == b"f" || delim.starts_with(b"f-");
    // A single newline straight after the opening `[` is dropped from the
    // value. It is still inside the literal's span, so the extent is unchanged
    // and the scanner simply steps over it.
    let mut body = cursor + 1;
    if bytes.get(body) == Some(&b'\r') {
        body += 1;
    }
    if bytes.get(body) == Some(&b'\n') {
        body += 1;
    }
    hy_string_body_scan(bytes, body, fstring, HyCloser::Bracket(delim), budget)
}

/// Whether the string's closing delimiter sits at `pos`, and how wide it is.
fn hy_closer_width(bytes: &[u8], pos: usize, closer: HyCloser<'_>) -> Option<usize> {
    match closer {
        HyCloser::Quote => (bytes.get(pos) == Some(&b'"')).then_some(1),
        HyCloser::Bracket(delim) => {
            if bytes.get(pos) != Some(&b']') {
                return None;
            }
            let after = pos + 1;
            let end = after + delim.len();
            if bytes.get(after..end) != Some(delim) {
                return None;
            }
            (bytes.get(end) == Some(&b']')).then_some(delim.len() + 2)
        }
    }
}

/// Scans a Hy string body from its first content byte to just past its close.
///
/// Two regions alternate. In the literal region the closing delimiter ends the
/// string, a backslash escapes the next byte (for quoted strings — a bracket
/// string's `delim_closing` has no escape case at all, which is what makes it
/// raw), and in f-string mode `{{`/`}}` are doubled literals while a lone `{`
/// opens an interpolation.
///
/// Inside an interpolation the bytes are Hy code, so the scanner has to skip
/// what the reader would skip: nested string literals of either spelling,
/// `;` line comments, and nested braces from dict literals. Getting this wrong
/// would move the closing quote, which is why it is a real sub-reader rather
/// than a search for the next `"`.
fn hy_string_body_scan(
    bytes: &[u8],
    pos: usize,
    fstring: bool,
    closer: HyCloser<'_>,
    budget: usize,
) -> HyScan {
    let mut cursor = pos;
    let mut escaped = false;
    let mut depth = 0usize;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if depth == 0 {
            // Hy checks `closing(c)` before it looks at braces, so the closing
            // delimiter wins over everything except a pending escape.
            if !escaped {
                if let Some(width) = hy_closer_width(bytes, cursor, closer) {
                    return HyScan::End(cursor + width);
                }
            }
            if escaped {
                escaped = false;
                cursor += 1;
                continue;
            }
            if byte == b'\\' && matches!(closer, HyCloser::Quote) {
                escaped = true;
                cursor += 1;
                continue;
            }
            if !fstring {
                cursor += 1;
                continue;
            }
            match byte {
                b'{' if bytes.get(cursor + 1) == Some(&b'{') => cursor += 2,
                b'{' => {
                    depth = 1;
                    cursor += 1;
                }
                b'}' if bytes.get(cursor + 1) == Some(&b'}') => cursor += 2,
                // `read_chars_until` raises "single '}' is not allowed" here.
                b'}' => {
                    return HyScan::Refused {
                        position: cursor,
                        byte,
                    };
                }
                _ => cursor += 1,
            }
            continue;
        }

        // Interpolation: this is Hy code.
        match byte {
            b';' => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth -= 1;
                cursor += 1;
            }
            _ => {
                // A nested literal keeps its own rules, including its own
                // escapes and its own braces, so it is scanned rather than
                // skipped byte by byte. `f"{(str \"}\")}"` depends on this:
                // the `}` inside the nested string must not close the field.
                let nested = if is_nested_hy_string_start(bytes, cursor) {
                    hy_string_scan_at(bytes, cursor, budget - 1)
                } else {
                    None
                };
                match nested {
                    Some(HyScan::End(end)) => cursor = end,
                    Some(other) => return other,
                    None => cursor += 1,
                }
            }
        }
    }

    HyScan::Unterminated
}

/// Whether a nested string literal starts at `pos` inside an interpolation.
///
/// A prefix only counts at the start of an identifier, so the `f` of `xf"..."`
/// is not one.
fn is_nested_hy_string_start(bytes: &[u8], pos: usize) -> bool {
    if bytes.get(pos) == Some(&b'#') {
        return bytes.get(pos + 1) == Some(&b'[');
    }
    if bytes.get(pos) == Some(&b'"') {
        return true;
    }
    if hy_string_prefix_width_at(bytes, pos).is_none() {
        return false;
    }
    pos == 0 || bytes.get(pos - 1).copied().is_some_and(is_hy_non_ident)
}

/// The three bytes that open a Racket here string.
const HERE_STRING_OPEN: &[u8] = b"#<<";

/// The hash-literal dispatch spellings `read-hash` accepts, longest first so
/// `#hasheqv(` is not read as `#hasheq` followed by a stray `v`.
///
/// `read-hash` spells them out as a chain of `get-next!` calls, each of which
/// takes a character and its upper-case alternate, so every letter is
/// case-insensitive and `#HASH(` is as valid as `#hash(`.
const RACKET_HASH_SPELLINGS: [&[u8]; 4] = [b"hasheqv", b"hashalw", b"hasheq", b"hash"];

/// Whether a sequence a reader dispatch can take as its payload opens at `pos`.
///
/// All three brackets, because Racket's `allows_delimiter` admits all three and
/// `read-struct`, `read-hash` and `read-vector` each accept `(`, `[` and `{`.
fn opens_sequence(bytes: &[u8], pos: usize) -> bool {
    matches!(bytes.get(pos), Some(b'(' | b'[' | b'{'))
}

/// Whether a `#rx`/`#px` payload starts at `pos`: a string, or a byte string.
fn opens_regexp_payload(bytes: &[u8], pos: usize) -> bool {
    match bytes.get(pos) {
        Some(b'"') => true,
        Some(b'#') => bytes.get(pos + 1) == Some(&b'"'),
        _ => false,
    }
}

/// Width of a `#` dispatch that introduces a vector literal whose elements
/// follow in a bracketed sequence: `#3(1 2 3)`, `#fx(1)`, `#fl6(0.0)`.
///
/// `read-dispatch` reaches these three ways — a digit run goes to
/// `read-vector-or-graph`, and `#f` followed by `x` or `l` goes to
/// `read-fixnum-or-flonum-vector`, which reads its own optional digit run.
/// A bare `#(` has neither and is not this shape: it returns `None` so the
/// caller's [`ReaderPrefix::HashLiteral`] arm keeps reading it as a prefix on
/// a visible list, which is what it has always been.
///
/// The opener is required. Without it `#fx` is not a vector at all, and
/// claiming it were would consume whatever followed as a payload.
fn racket_vector_dispatch_width(bytes: &[u8], pos: usize) -> Option<usize> {
    let mut cursor = pos + 1;
    if bytes.get(cursor) == Some(&b'f') && matches!(bytes.get(cursor + 1), Some(b'x' | b'l')) {
        cursor += 2;
    }
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_digit()) {
        cursor += 1;
    }
    if cursor == pos + 1 {
        return None;
    }
    opens_sequence(bytes, cursor).then_some(cursor - pos)
}

/// Width of the `#hash…` dispatch introducing a hash-table literal, covering
/// the dispatch alone and never the opening bracket.
///
/// The bracket is where the `MultiDatum` payload starts and `skip_form`
/// consumes it, so the two together become one opaque reader-form node
/// spanning the whole literal — the same shape, and the same known limitation,
/// as `clojure_namespaced_map_width`: a rule looking for keys sees nothing
/// inside a `#hash(…)`. Representing the dispatch as a [`ReaderPrefix`] is what
/// would fix that, and it is blocked on the same thing — `ReaderPrefix` carries
/// a `&'static str` spelling and `#hash`, `#hasheq`, `#hasheqv` and `#hashalw`
/// are four.
///
/// Requiring the bracket to touch the dispatch is faithful here, unlike in
/// Clojure: `read-hash`'s loop reads the next character with
/// `read-char/special` and has no whitespace case at all, so `#hash (…)` is
/// its own "bad syntax" error.
fn racket_hash_dispatch_width(bytes: &[u8], pos: usize) -> Option<usize> {
    RACKET_HASH_SPELLINGS.iter().find_map(|spelling| {
        let end = pos + 1 + spelling.len();
        if !bytes.get(pos + 1..end)?.eq_ignore_ascii_case(spelling) {
            return None;
        }
        opens_sequence(bytes, end).then_some(end - pos)
    })
}

/// How far the Racket here string whose `#<<` sits at `pos` reaches.
///
/// See [`DialectReaderPolicy::here_string_extent`] for why this matches
/// `read-here-string`'s state machine, and for what the terminator line may
/// and may not contain.
fn here_string_extent_at(bytes: &[u8], pos: usize) -> HereStringExtent {
    let tag_start = pos + HERE_STRING_OPEN.len();
    let Some(tag_len) = bytes.get(tag_start..).and_then(newline_offset) else {
        // "found end-of-file after `#<<` and before a newline".
        return HereStringExtent::Unterminated;
    };
    let tag = &bytes[tag_start..tag_start + tag_len];
    // The loop is seeded with `(cdr full-terminator)`, so the first content
    // byte is already a candidate terminator start.
    let mut cursor = tag_start + tag_len + 1;
    loop {
        if bytes.get(cursor..cursor + tag.len()) == Some(tag) {
            let after = cursor + tag.len();
            match bytes.get(after) {
                None => return HereStringExtent::Closed { width: after - pos },
                Some(b'\n') => {
                    return HereStringExtent::Closed {
                        width: after + 1 - pos,
                    };
                }
                Some(_) => {}
            }
        }
        // Only a newline restarts the match, so the next candidate is the byte
        // after the next newline.
        let Some(offset) = bytes.get(cursor..).and_then(newline_offset) else {
            // "found end-of-file before terminating `~a`".
            return HereStringExtent::Unterminated;
        };
        cursor += offset + 1;
    }
}

/// Width of the Racket Unix line comment at `pos`, if one starts there.
///
/// `#!` plus a space or a `/`, then everything to the end of the line — or to
/// the end of a later line, because `skip-unix-line-comment!` continues when
/// the byte before the newline is a `\`:
///
/// ```racket
/// (let loop ([backslash? #f])
///   (define c (read-char/special in config))
///   (cond
///    [(eof-object? c) (void)]
///    [(not (char? c)) (loop #f)]
///    [(char=? c #\newline) (when backslash? (loop #f))]
///    [(char=? c #\\) (loop #t)]
///    [else (loop #f)]))
/// ```
///
/// The width stops *before* the newline that ends it, which is what the caller
/// wants: `skip_trivia` records the comment and then advances to the newline
/// itself, so a width that included it would swallow the line break.
///
/// `#!racket` and `#!r6rs` are deliberately not comments. Without a space or a
/// `/` the reader takes `#!` to `read-lang`, which reads a language name, so
/// leaving them to the atom scanner keeps the existing reading rather than
/// hiding a directive inside trivia.
///
/// ### Why the token-start test is load-bearing
///
/// `line_comment_width` is called from `is_atom_boundary` for *every byte of
/// every atom*, not only where a datum may start, and `#` and `!` are ordinary
/// symbol constituents in Racket. Without this test the symbol
/// `read-extension-#!` — followed by a space, as it is at every one of its call
/// sites in Racket's own reader — split at its `#` and turned the rest of the
/// line into a comment, so `read/main.rkt` and `read/language.rkt` stopped
/// parsing. The whitespace skipper Racket runs this from only ever sees a
/// position where a datum may begin, and a token cannot contain whitespace or
/// a delimiter, so "preceded by whitespace, a delimiter, or nothing" is exactly
/// that position and admits no token interior.
fn racket_unix_line_comment_width(bytes: &[u8], pos: usize) -> Option<usize> {
    if bytes.get(pos + 1) != Some(&b'!') || !matches!(bytes.get(pos + 2), Some(b' ' | b'/')) {
        return None;
    }
    let at_token_start = pos == 0
        || bytes.get(pos - 1).is_some_and(|byte| {
            byte.is_ascii_whitespace() || DialectReaderPolicy::is_raw_delimiter(*byte)
        });
    if !at_token_start {
        return None;
    }
    let mut cursor = pos + 3;
    loop {
        let Some(offset) = bytes.get(cursor..).and_then(newline_offset) else {
            return Some(bytes.len() - pos);
        };
        let newline = cursor + offset;
        if bytes.get(newline - 1) != Some(&b'\\') {
            return Some(newline - pos);
        }
        cursor = newline + 1;
    }
}

/// How many bytes precede the first newline in `bytes`, if there is one.
fn newline_offset(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|byte| *byte == b'\n')
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

/// Whether a well-formed Emacs Lisp radix integer begins at `pos`.
///
/// Emacs Lisp spells an integer in a non-decimal base four ways, all of them
/// in the Elisp reference manual's "Integer Basics": `#b1010`, `#o777`,
/// `#x2a`, and `#<radix>r<digits>` for any radix from 2 to 36. Every one of
/// them was an `unsupported reader dispatch` here, which failed the whole
/// document -- 122 of 1674 files in GNU Emacs's own `lisp/` tree, including
/// `bookmark.el`, `ansi-color.el` and `calc.el`, parsed not at all.
///
/// The grammar below is `read_integer` in `src/lread.c`, and every clause of it
/// was confirmed against a running Emacs rather than read off the manual:
///
/// | source     | Emacs reads | why                                    |
/// |------------|-------------|----------------------------------------|
/// | `#x10`     | 16          | `b`/`o`/`x` are the only letter bases   |
/// | `#X10`     | 16          | the prefix letter is case-insensitive   |
/// | `#d99`     | **error**   | Emacs Lisp has no `#d`, unlike CLHS 2.4.8.6 |
/// | `#24r1k`   | 44          | `r` takes a radix of 2..=36             |
/// | `#24R1K`   | 44          | the `r` and the digits are both case-insensitive |
/// | `#016r10`  | 16          | the radix may carry leading zeros       |
/// | `#37r1`    | **error**   | radix above 36                          |
/// | `#1r0`     | **error**   | radix below 2                           |
/// | `#x-1f`    | -31         | a sign belongs to the *digits*          |
/// | `#-2r1`    | **error**   | but never to the radix                  |
/// | `#x`       | **error**   | at least one digit is required          |
/// | `#o8`      | **error**   | a digit must be in range for the base   |
/// | `#x1f2gh`  | **error**   | *including* one past the end of the number |
///
/// That last row is the clause that a hand-written reader gets wrong, and it is
/// why this validates instead of merely recognising a prefix. `digit_to_number`
/// returns -2 for a non-alphanumeric byte and -1 for an alphanumeric one that
/// is out of range for the base, and `read_integer`'s loop continues on -1
/// while setting `valid = 0`. So the token runs to the end of the alphanumeric
/// run whatever that run contains, and one out-of-range letter anywhere in it
/// invalidates the whole literal rather than ending it early: `#x1f2gh` is an
/// error, not `#x1f2` followed by the symbol `gh`.
///
/// A byte that is not alphanumeric ends the number instead of poisoning it, so
/// `#xFF)`, `#xFF;c` and `#xff]` all read as 255 -- the literal ends at the
/// paren, the comment and the bracket. Those three are already atom boundaries
/// here, which is what makes returning `None` the whole fix: the atom scanner
/// stops on exactly the bytes Emacs stops on for every literal that occurs in
/// real code.
///
/// A [`Self::character_literal_prefix_width`]-style bespoke extent is
/// deliberately *not* introduced for the bytes where the two disagree, because
/// the disagreement is not specific to radix integers. `#o777"s"` reads as one
/// atom where Emacs reads 511 and then a string -- but `abc"s"` and `12"s"`
/// already do the same on `main`, in every dialect. That is the atom scanner's
/// existing model of a token adjacent to a string literal, and changing it is a
/// separate cross-dialect decision rather than a rider on this one.
///
/// # Why a malformed literal is still refused
///
/// Returning `false` leaves the caller's `UnsupportedDispatch`, which fails the
/// parse. That is not this function inventing a rule -- it is what Emacs itself
/// does: `#xZZ` signals `invalid-read-syntax`, so a file containing one does not
/// load, and refusing it here agrees with the reader rather than silently
/// reading a number that Emacs never would. It is also the behaviour every one
/// of these spellings already had before this function existed, so nothing
/// regresses for input that was refused yesterday.
///
/// # Known approximation
///
/// A *valid* literal followed immediately by a non-alphanumeric byte that is
/// not an atom boundary here -- `#xa+b`, `#x1.5` -- reads as one atom, where
/// Emacs reads `#xa` and then `+b`. Both spellings are absent from all 1674
/// files of GNU Emacs's `lisp/` tree, and closing the gap would mean giving the
/// literal a bespoke extent in the atom scanner, which is a change to a code
/// path shared by all ten dialects. The safe reading is taken instead.
fn emacs_lisp_radix_literal_is_valid(bytes: &[u8], pos: usize) -> bool {
    if bytes.get(pos) != Some(&b'#') {
        return false;
    }

    let (radix, digits_start) = match bytes.get(pos + 1).copied() {
        Some(b'b' | b'B') => (2u32, pos + 2),
        Some(b'o' | b'O') => (8, pos + 2),
        Some(b'x' | b'X') => (16, pos + 2),
        // `#<radix>r`. The radix is a decimal run that may carry leading zeros
        // (`#0002r11` is 3), so it is accumulated rather than length-limited;
        // saturating arithmetic keeps an adversarially long run from wrapping
        // into the valid range instead of failing the bound below.
        Some(byte) if byte.is_ascii_digit() => {
            let mut cursor = pos + 1;
            let mut radix: u32 = 0;
            while let Some(&byte) = bytes.get(cursor) {
                let Some(digit) = char::from(byte).to_digit(10) else {
                    break;
                };
                radix = radix.saturating_mul(10).saturating_add(digit);
                cursor += 1;
            }
            if !matches!(bytes.get(cursor), Some(b'r' | b'R')) {
                return false;
            }
            (radix, cursor + 1)
        }
        _ => return false,
    };

    // Guards `to_digit` below, which panics above 36, as well as rejecting the
    // radices Emacs rejects.
    if !(2..=36).contains(&radix) {
        return false;
    }

    let mut cursor = digits_start;
    if matches!(bytes.get(cursor), Some(b'-' | b'+')) {
        cursor += 1;
    }
    let digits_run_start = cursor;
    while let Some(&byte) = bytes.get(cursor) {
        if !byte.is_ascii_alphanumeric() {
            break;
        }
        if char::from(byte).to_digit(radix).is_none() {
            return false;
        }
        cursor += 1;
    }
    cursor > digits_run_start
}

/// How many modifier prefixes one Emacs Lisp character literal may stack
/// before [`emacs_lisp_character_payload_end`] gives up.
///
/// Emacs has six (`A- C- H- M- S- s-`) and `read_char_escape` applies each at
/// most once per literal, so any chain longer than this is input no reader
/// accepts. The bound exists because the payload rule is recursive and the
/// scanner is not: without it, `?\C-\C-\C-…` would walk the rest of the file.
const MAX_EMACS_LISP_CHARACTER_MODIFIERS: usize = 8;

/// The offset just past the payload of the Emacs Lisp character literal whose
/// `?` sits at `start - 1`, or `None` when Emacs itself would refuse it.
///
/// Transcribed from `read_char_escape` in `src/lread.c`; see
/// [`DialectReaderPolicy::exact_character_literal_width`] for the table this
/// implements and for why the recursion matters.
fn emacs_lisp_character_payload_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    for _ in 0..MAX_EMACS_LISP_CHARACTER_MODIFIERS {
        let byte = *bytes.get(cursor)?;
        if byte != b'\\' {
            // `?a`, `?あ`, `?{`, `? `. One whole character, taken verbatim.
            return Some(cursor + utf8_sequence_width(byte));
        }
        let escape = *bytes.get(cursor + 1)?;
        let after = cursor + 2;
        match escape {
            // A modifier prefix. `read_char_escape` re-enters its own switch
            // on whatever follows the hyphen, so the payload may be another
            // escape (`?\M-\C-b`) or a raw byte that would otherwise end the
            // token (`?\C- `, `?\C-]`).
            b'A' | b'C' | b'H' | b'M' | b'S' | b's' if bytes.get(after) == Some(&b'-') => {
                cursor = after + 1;
            }
            // `\s` *not* followed by a hyphen is SPC, and is the one escape
            // whose letter is both a modifier and a character. `\A`, `\C`,
            // `\H`, `\M` and `\S` without a hyphen are errors in Emacs, so
            // they fall to the `None` below rather than being read as the
            // letter.
            b's' => return Some(after),
            b'A' | b'C' | b'H' | b'M' | b'S' => return None,
            // `?\^I` is control-I, and `^` takes a whole payload again just as
            // `C-` does — `read_char_escape` reaches it by falling through
            // from the `C` case.
            b'^' => cursor = after,
            // `\x` takes every hex digit that follows, and at least one.
            b'x' => {
                let digits = hex_digit_run(bytes, after);
                return (digits > 0).then_some(after + digits);
            }
            // `\uXXXX` and `\UXXXXXXXX` are exact-length, and Emacs raises
            // "Malformed Unicode escape" for anything shorter.
            b'u' => return exact_hex_run(bytes, after, 4),
            b'U' => return exact_hex_run(bytes, after, 8),
            // `?\N{U+261D}` and `?\N{OGHAM SPACE MARK}`. The brace is
            // mandatory ("Expected opening brace after \\N"), and the name may
            // contain spaces — which is the whole reason this cannot be left
            // to a boundary scan.
            b'N' => {
                if bytes.get(after) != Some(&b'{') {
                    return None;
                }
                let close = bytes.get(after + 1..)?.iter().position(|&b| b == b'}')?;
                return Some(after + 1 + close + 1);
            }
            // At most three octal digits, and only `0`-`7`: `?\8` and `?\9`
            // are the characters `8` and `9`, through the verbatim arm below.
            b'0'..=b'7' => {
                let mut digits = 1;
                while digits < 3 && matches!(bytes.get(cursor + 1 + digits), Some(b'0'..=b'7')) {
                    digits += 1;
                }
                return Some(cursor + 1 + digits);
            }
            // `?\n`, `?\e`, `?\;`, `?\)`, `?\ `, `?\\`. The character after
            // the backslash, whatever it is.
            _ => return Some(cursor + 1 + utf8_sequence_width(escape)),
        }
    }
    None
}

/// The byte length of the UTF-8 sequence `lead` starts.
///
/// Every caller has already been handed a `&str`'s bytes, so `lead` is a real
/// lead byte; a continuation byte returns 1 so a malformed slice advances
/// rather than stalling.
const fn utf8_sequence_width(lead: u8) -> usize {
    match lead {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    }
}

/// How many hex digits run from `pos`.
fn hex_digit_run(bytes: &[u8], pos: usize) -> usize {
    bytes[pos.min(bytes.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count()
}

/// `pos + count` when exactly `count` hex digits sit at `pos`.
fn exact_hex_run(bytes: &[u8], pos: usize, count: usize) -> Option<usize> {
    let run = bytes.get(pos..pos + count)?;
    run.iter().all(u8::is_ascii_hexdigit).then_some(pos + count)
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

#[cfg(test)]
mod emacs_lisp_radix_tests {
    use super::emacs_lisp_radix_literal_is_valid;

    /// Every row was produced by a running GNU Emacs 31.0.91 rather than read
    /// off the manual, with `(read-from-string ...)` under `emacs --batch -Q`.
    /// The value column is what that call returned; a row in
    /// [`REFUSED_BY_EMACS`] is one where it signalled `invalid-read-syntax`.
    ///
    /// Keeping the oracle's own answers here is what makes this table worth
    /// more than the implementation restated: `#d99`, `#x1f2gh` and `#35rz`
    /// are all shapes a plausible hand-written reader accepts and Emacs does
    /// not.
    const ACCEPTED_BY_EMACS: &[(&str, i128)] = &[
        // The three letter bases, each case-insensitive.
        ("#b101", 5),
        ("#B101", 5),
        ("#o777", 511),
        ("#O777", 511),
        ("#x10", 16),
        ("#X10", 16),
        ("#xAb", 171),
        ("#XaB", 171),
        // `#<radix>r`, radix 2..=36, `r` and digits both case-insensitive.
        ("#2r1111", 15),
        ("#24r1k", 44),
        ("#24R1K", 44),
        ("#24r1K", 44),
        ("#24R1k", 44),
        ("#36rZZ", 1295),
        ("#36rzz", 1295),
        ("#10r99", 99),
        // A radix may carry leading zeros: `read_integer` accumulates the run.
        ("#016r10", 16),
        ("#02r11", 3),
        ("#0002r11", 3),
        // A sign belongs to the digits.
        ("#x-1f", -31),
        ("#x+1f", 31),
        ("#b-101", -5),
        ("#b+101", 5),
        ("#o-7", -7),
        ("#24r-1k", -44),
        ("#2r-1", -1),
        // Zero, and redundant leading zeros in the digits.
        ("#x0", 0),
        ("#b0", 0),
        ("#o0", 0),
        ("#x00ff", 255),
        ("#b00000000", 0),
        // `e` is an ordinary hex digit here, not an exponent marker.
        ("#xFFe0", 65504),
        // Beyond 64 bits. The reader's own bignum path; nothing here parses the
        // value, but the token must still be recognised.
        ("#o17777777777777777777777", 147_573_952_589_676_412_927),
        ("#x0000000000000000000000001", 1),
    ];

    /// Spellings Emacs refuses with `invalid-read-syntax`, so a file containing
    /// one does not load. Returning `false` for these leaves the caller's
    /// `UnsupportedDispatch`, which fails the parse -- agreeing with the reader
    /// rather than inventing a number Emacs never produced.
    const REFUSED_BY_EMACS: &[&str] = &[
        // Emacs Lisp has no `#d`, though CLHS 2.4.8.6 gives Common Lisp one.
        // Copying the Common Lisp arm wholesale would have accepted these.
        "#d99", "#D99", // No digits at all.
        "#x", "#b", "#o", "#24r", "#x-", "#x+", "#24r-",
        // A digit out of range for the base.
        "#b2", "#o8", "#xg", "#xZZ", "#10r9a", "#24rZZ",
        // Radix 35 is legal; `z` is 35, which is not a digit in base 35.
        "#35rz", // Radix out of range.
        "#0r0", "#1r0", "#37r1", "#39r1",
        // The clause a hand-written reader gets wrong: `digit_to_number`
        // returns -1 (not -2) for an alphanumeric byte out of range, and the
        // loop keeps consuming while marking the literal invalid. So the token
        // runs to the end of the alphanumeric run and one bad letter anywhere
        // in it poisons the whole literal -- `#x1f2gh` is an error, not
        // `#x1f2` followed by the symbol `gh`.
        "#x1f2gh", "#b101abc", "#o7778", "#24r1kz", "#xFFxFF",
        // A radix takes no sign; `#-`/`#+` are their own (absent) dispatch.
        "#-2r1", "#+2r1",
        // `_` is not alphanumeric, so it ends the run -- leaving none.
        "#x_1", // Not radix syntax at all.
        "#r1", "#'foo", "#s(a)", "#[1]", "##", "#:foo", "#(a)", "#&5\"x\"", "#1=(a)", "#^[1]",
    ];

    #[test]
    fn accepts_every_radix_integer_emacs_accepts() {
        for (source, value) in ACCEPTED_BY_EMACS {
            assert!(
                emacs_lisp_radix_literal_is_valid(source.as_bytes(), 0),
                "{source} reads as {value} in Emacs and must be accepted"
            );
        }
    }

    #[test]
    fn refuses_every_spelling_emacs_refuses() {
        for source in REFUSED_BY_EMACS {
            assert!(
                !emacs_lisp_radix_literal_is_valid(source.as_bytes(), 0),
                "{source} is invalid-read-syntax in Emacs and must be refused"
            );
        }
    }

    /// A non-alphanumeric byte ends the number instead of invalidating it, so
    /// the literal is still well formed when the delimiter, comment or string
    /// that follows it touches the digits.
    #[test]
    fn a_non_alphanumeric_byte_ends_the_literal() {
        for source in [
            "#xFF)",
            "#xff]",
            "#xFF}",
            "#xFF;c",
            "#xFF\"s\"",
            "#x1f'",
            "#xFF ",
            "#xFF\n",
            "#xFF\t",
            "#xa+b",
            "#x1.5",
            "#x1_0",
            "#xff.",
        ] {
            assert!(
                emacs_lisp_radix_literal_is_valid(source.as_bytes(), 0),
                "{source} begins a well-formed radix integer"
            );
        }
    }

    /// The scan starts at `pos`, not at 0, because `classify_reader_macro` is
    /// called at every dispatch position in the document.
    #[test]
    fn scans_from_the_given_offset() {
        let source = b"(f #x1f #xZZ)";
        assert!(emacs_lisp_radix_literal_is_valid(source, 3));
        assert!(!emacs_lisp_radix_literal_is_valid(source, 8));
        // Not a `#` at all.
        assert!(!emacs_lisp_radix_literal_is_valid(source, 0));
        assert!(!emacs_lisp_radix_literal_is_valid(source, 1));
    }

    /// A radix run long enough to overflow `u32` must fail the 2..=36 bound
    /// rather than wrap into it -- and must not reach `to_digit`, which panics
    /// above radix 36.
    #[test]
    fn an_absurd_radix_saturates_rather_than_wrapping() {
        for source in [
            "#99999999999999999999999999r1",
            "#4294967298r1",
            "#4294967320r1",
        ] {
            assert!(
                !emacs_lisp_radix_literal_is_valid(source.as_bytes(), 0),
                "{source} has a radix far outside 2..=36"
            );
        }
    }

    /// Truncation at end of input is refused rather than read as a complete
    /// literal, which is what keeps the formatter's trailing newline from
    /// becoming a digit on the next parse.
    #[test]
    fn a_truncated_literal_at_end_of_input_is_refused() {
        for source in ["#", "#x", "#2", "#24", "#24r", "#24r-", "#b", "#o"] {
            assert!(
                !emacs_lisp_radix_literal_is_valid(source.as_bytes(), 0),
                "{source} is truncated"
            );
        }
    }
}

#[cfg(test)]
mod emacs_lisp_character_literal_tests {
    use super::{Dialect, DialectReaderPolicy};

    fn width(source: &str) -> Option<usize> {
        DialectReaderPolicy::new(Dialect::EmacsLisp)
            .exact_character_literal_width(source.as_bytes(), 0)
    }

    /// Every row was produced by a running GNU Emacs 31.0.91 rather than read
    /// off the manual, with `(read-from-string ...)` under `emacs --batch -Q`.
    /// The width column is the byte length of the prefix that call consumed;
    /// the value column is what it returned, and is carried so a later change
    /// that shortens a literal without changing its width still fails a review
    /// of this table.
    ///
    /// The rows that matter most are the ones whose payload is a byte that
    /// ends a token everywhere else: `?\C- `, `?\C-]`, `?\N{OGHAM SPACE MARK}`.
    /// Those are why this function exists at all — no backslash rule, however
    /// written, finds the end of them.
    const ACCEPTED_BY_EMACS: &[(&str, usize, i64)] = &[
        // A bare character, taken verbatim, delimiters included.
        ("?a", 2, 97),
        ("?{", 2, 123),
        ("?}", 2, 125),
        ("?[", 2, 91),
        ("?\"", 2, 34),
        ("? ", 2, 32),
        // Multi-byte payloads are one character, not one byte.
        ("?\u{3042}", 4, 12354),
        ("?\u{1F600}", 5, 128512),
        // Named escapes, and the ordinary escaped-character arm.
        ("?\\n", 3, 10),
        ("?\\e", 3, 27),
        ("?\\d", 3, 127),
        ("?\\s", 3, 32),
        ("?\\;", 3, 59),
        ("?\\)", 3, 41),
        ("?\\\\", 3, 92),
        ("?\\ ", 3, 32),
        // `\8` and `\9` are the digits, not octal: the octal arm is `0`-`7`.
        ("?\\8", 3, 56),
        ("?\\9", 3, 57),
        // Radix escapes. `\x` is greedy, `\u` and `\U` are exact-length.
        ("?\\x41", 5, 65),
        ("?\\xeFc", 6, 3836),
        ("?\\u00e9", 7, 233),
        ("?\\U0001F600", 11, 128512),
        ("?\\101", 5, 65),
        ("?\\0", 3, 0),
        // Named characters. The name may contain spaces, which is the whole
        // reason a boundary scan cannot find the end of one.
        ("?\\N{U+261D}", 11, 9757),
        ("?\\N{OGHAM SPACE MARK}", 21, 5760),
        // Modifier prefixes, whose payload is read all over again.
        ("?\\C-a", 5, 1),
        ("?\\^I", 4, 9),
        ("?\\M-\\C-b", 8, 134217730),
        ("?\\M-\\S-\\C-a", 11, 167772161),
        ("?\\s-a", 5, 8388705),
        ("?\\S-a", 5, 33554529),
        // The reported corruption, and its family: a payload that is itself a
        // token boundary. `isearch.el` line 3335 is the first row.
        ("?\\S-\\ ", 6, 33554464),
        ("?\\M-\\ ", 6, 134217760),
        ("?\\C-\\[", 6, 27),
        ("?\\C-\\]", 6, 29),
        ("?\\^\\]", 5, 29),
        // `bindings.el`, `kkc.el`, `korea-util.el` and `ns-win.el`: a modifier
        // followed by a *bare* space or bracket, with no backslash at all.
        ("?\\C- ", 5, 67108896),
        ("?\\S- ", 5, 33554464),
        ("?\\C-]", 5, 29),
        ("?\\M- ", 5, 134217760),
        ("?\\^ ", 4, 67108896),
        ("?\\C-\\s- ", 8, 75497504),
    ];

    /// Spellings a running Emacs 31.0.91 refuses with `invalid-read-syntax` or
    /// "Invalid escape char syntax". `None` leaves the old prefix-plus-scan
    /// reading in place rather than inventing a refusal for input no reader
    /// accepts, so this table pins that they fall through — not that they are
    /// rejected.
    const REFUSED_BY_EMACS: &[&str] = &[
        // "\\x not followed by hex digit"
        "?\\x",
        // "Malformed Unicode escape"
        "?\\u00e",
        "?\\U0001F6",
        // "Expected opening brace after \\N"
        "?\\Na",
        "?\\N",
        "?\\N{unterminated",
        // "\\C not followed by -"
        "?\\Ca",
        "?\\Sa",
        "?\\Ma",
        // Truncated at end of input.
        "?",
        "?\\",
    ];

    #[test]
    fn every_width_matches_what_emacs_consumed() {
        for (source, expected, _value) in ACCEPTED_BY_EMACS {
            assert_eq!(
                width(source),
                Some(*expected),
                "{source:?} is {expected} bytes to GNU Emacs"
            );
        }
    }

    #[test]
    fn a_spelling_emacs_refuses_falls_through_to_the_old_scan() {
        for source in REFUSED_BY_EMACS {
            assert_eq!(
                width(source),
                None,
                "{source:?} is refused by GNU Emacs and must not be modelled"
            );
        }
    }

    /// The width is measured from `pos`, not from zero: this is called once per
    /// literal, mid-document, for every file.
    #[test]
    fn the_width_is_relative_to_the_offset_it_is_asked_about() {
        let source = "(define-key map [?\\C- ] 'f)";
        let at = source.find('?').expect("the fixture has a literal");
        assert_eq!(
            DialectReaderPolicy::new(Dialect::EmacsLisp)
                .exact_character_literal_width(source.as_bytes(), at),
            Some(5)
        );
    }

    /// The model is gated on Emacs Lisp alone. `#\` and `\` literals in the
    /// other ten readers keep the prefix-plus-scan path they have always had,
    /// which is what makes a cross-dialect corpus sweep byte-identical by
    /// construction rather than by luck.
    #[test]
    fn no_other_dialect_is_modelled() {
        for dialect in [
            Dialect::CommonLisp,
            Dialect::Lfe,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Hy,
            Dialect::Carp,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let policy = DialectReaderPolicy::new(dialect);
            for source in ["?a", "?\\C- ", "#\\a", "\\newline", "?\\N{U+261D}"] {
                assert_eq!(
                    policy.exact_character_literal_width(source.as_bytes(), 0),
                    None,
                    "{dialect:?} must keep its own character-literal scan for {source:?}"
                );
            }
        }
    }

    /// A `?` that opens no literal, and a document that ends inside one, must
    /// both return `None` rather than a width past the end of the slice.
    #[test]
    fn a_non_literal_or_truncated_input_has_no_width() {
        for source in ["", "a", "(", "#\\a", "?"] {
            assert_eq!(width(source), None, "{source:?} opens no complete literal");
        }
    }

    /// A modifier chain is bounded, so adversarial input cannot walk the rest
    /// of the document. Emacs has six modifiers and applies each at most once,
    /// so nothing legal comes close to the bound.
    #[test]
    fn an_unbounded_modifier_chain_is_refused_rather_than_scanned_to_the_end() {
        let source = format!("?{}a", "\\C-".repeat(64));
        assert_eq!(width(&source), None);
    }

    /// The literal reported against `isearch.el`, spelled out. `?\S-\ ` is six
    /// bytes; reading five re-emitted `?\S-\`, and Emacs then took the closing
    /// paren after it as the escaped character.
    #[test]
    fn the_isearch_literal_keeps_its_escaped_space() {
        assert_eq!(width("?\\S-\\ "), Some(6));
    }

    /// `"` ends a token in Emacs Lisp as it does in Racket, and in neither of
    /// the other nine: `(read "(format\"x\" 1)")` is `(format "x" 1)`.
    #[test]
    fn a_string_terminates_a_token_in_emacs_lisp_and_racket_only() {
        for dialect in [Dialect::EmacsLisp, Dialect::Racket] {
            assert!(DialectReaderPolicy::new(dialect).string_terminates_a_token());
        }
        for dialect in [
            Dialect::CommonLisp,
            Dialect::Lfe,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Hy,
            Dialect::Carp,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            assert!(
                !DialectReaderPolicy::new(dialect).string_terminates_a_token(),
                "{dialect:?} has had no corpus audit for this rule"
            );
        }
    }

    /// Emacs Lisp joins Common Lisp in reading `\` as a single escape, and the
    /// eight dialects without the rule must not acquire it: a stray `\` read as
    /// an escape in Scheme swallows the delimiter after it.
    #[test]
    fn single_escape_is_emacs_lisp_common_lisp_and_the_legacy_reader() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp, Dialect::Unknown] {
            assert!(DialectReaderPolicy::new(dialect).supports_single_escape());
        }
        for dialect in [
            Dialect::Lfe,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Hy,
            Dialect::Carp,
            Dialect::Janet,
            Dialect::Fennel,
        ] {
            assert!(!DialectReaderPolicy::new(dialect).supports_single_escape());
        }
    }
}
