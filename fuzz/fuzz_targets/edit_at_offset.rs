//! Every structural edit, at a caller-supplied byte offset.
//!
//! `--at <offset>` is the one place this tool takes a raw index from outside
//! and turns it into a slice. The offset can point inside a multi-byte
//! character, inside a string literal, or past the end of the document, and an
//! agent computing offsets from a diff will eventually supply all three.
//!
//! The first byte of the input chooses the offset, so the fuzzer can steer the
//! selection as well as the document — with the offset derived from the length
//! instead, almost every sample would select the same node.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paredit_core_syntax::sexpr::{Edit, SyntaxTree};

fuzz_target!(|data: &[u8]| {
    let Some((&offset_seed, rest)) = data.split_first() else {
        return;
    };
    let Ok(source) = std::str::from_utf8(rest) else {
        return;
    };
    let Ok(tree) = SyntaxTree::parse(source) else {
        return;
    };

    // Spread the seed over the whole document rather than the first 256 bytes.
    let offset = if source.is_empty() {
        0
    } else {
        usize::from(offset_seed) * source.len() / 256
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
