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
