//! The reader, over arbitrary bytes, for every dialect.
//!
//! The properties are the ones `tests/parser_robustness.rs` checks on stable;
//! what a coverage-guided fuzzer adds is the ability to *reach* the deep reader
//! states — a nested block comment inside a feature expression inside a
//! quasiquote — that a token-soup generator finds only by luck.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

const DIALECTS: [Dialect; 10] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Lfe,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::Clojure,
    Dialect::Hy,
    Dialect::Carp,
    Dialect::Janet,
    Dialect::Fennel,
];

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    for dialect in DIALECTS {
        let Ok(tree) = SyntaxTree::parse_with_dialect(source, dialect) else {
            continue;
        };
        // The parse must be lossless: every rewrite in this workspace is a
        // span replacement over exactly this string.
        assert_eq!(
            tree.source(),
            source,
            "parsing was lossy for {dialect:?} on {source:?}"
        );
    }

    // Whatever the repair path returns must parse. It claims to append only
    // the closing delimiters an unclosed document needs, and a document it
    // "repaired" into something still unparseable would be a false success.
    if let Ok(repaired) = SyntaxTree::repair_unclosed_lists(source) {
        assert!(
            SyntaxTree::parse(&repaired).is_ok(),
            "repair produced unparseable output for {source:?}"
        );
    }
});
