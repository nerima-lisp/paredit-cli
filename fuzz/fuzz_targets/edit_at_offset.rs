//! Every structural edit, at a caller-supplied byte offset.
//!
//! `--at <offset>` is the one place this tool takes a raw index from outside
//! and turns it into a slice. The offset can point inside a multi-byte
//! character, inside a string literal, or past the end of the document, and an
//! agent computing offsets from a diff will eventually supply all three.
//!
//! The first two bytes choose the dialect and an offset boundary, so the fuzzer
//! can steer both selection inputs as well as the document.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paredit_core_syntax::{
    dialect::Dialect,
    sexpr::{Edit, SyntaxTree},
};

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
    let Some((&dialect_seed, data)) = data.split_first() else {
        return;
    };
    let Some((&offset_seed, source_bytes)) = data.split_first() else {
        return;
    };
    let Ok(source) = std::str::from_utf8(source_bytes) else {
        return;
    };
    let dialect = DIALECTS[usize::from(dialect_seed) % DIALECTS.len()];
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, dialect) else {
        return;
    };

    let len = source.len();
    let offset = match offset_seed % 6 {
        0 => 0,
        1 => len.saturating_sub(1),
        2 => len,
        3 => len.saturating_add(1),
        4 => usize::MAX,
        // Keep coverage over ordinary positions as well as the raw boundaries.
        _ => len.saturating_mul(usize::from(offset_seed)) / 256,
    };
    let Ok(selection) = tree.select_at(offset) else {
        return;
    };

    // Reaching the end of this list is the assertion: each of these either
    // rewrites or refuses, and neither may panic or slice mid-character.
    let _ = Edit::kill(source, &tree, selection);
    let _ = Edit::splice(source, &tree, selection);
    let _ = Edit::raise(source, &tree, selection);
    let _ = Edit::split(source, &tree, selection);
    let _ = Edit::join(source, &tree, selection);
    let _ = Edit::transpose_forward(source, &tree, selection);
    let _ = Edit::transpose_backward(source, &tree, selection);
    let _ = Edit::slurp_forward(source, &tree, selection);
    let _ = Edit::slurp_backward(source, &tree, selection);
    let _ = Edit::barf_forward(source, &tree, selection);
    let _ = Edit::barf_backward(source, &tree, selection);
    let _ = Edit::convolute(source, &tree, selection);
    let _ = Edit::splice_killing_forward(source, &tree, selection);
    let _ = Edit::splice_killing_backward(source, &tree, selection);
});
