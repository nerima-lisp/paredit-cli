//! `format(format(x)) == format(x)`, for every dialect.
//!
//! A formatter without this property produces diffs that never converge: `edit
//! format --write` in a pre-commit hook would change the file on every commit,
//! forever. The failure mode is subtle enough that it survives review — it
//! needs an input where the *output* of one pass lexes differently from the
//! input, which is exactly the shape a coverage-guided fuzzer is good at
//! finding.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Formatter, SyntaxTree};

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
        let formatter = Formatter::with_dialect(2, dialect);
        let once = formatter.format(&tree);

        // A formatted document must still parse. Anything else means the
        // formatter can turn a valid file into an invalid one.
        let reparsed = SyntaxTree::parse_with_dialect(&once, dialect)
            .unwrap_or_else(|error| panic!("{dialect:?}: formatted output does not reparse ({error}) for {source:?}"));

        let twice = formatter.format(&reparsed);
        assert_eq!(
            once, twice,
            "{dialect:?}: formatting did not converge for {source:?}"
        );
    }
});
