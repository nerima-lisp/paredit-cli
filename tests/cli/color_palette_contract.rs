//! docs/src/project/feature-candidates.md section L2: the terminal palette is
//! a fixed, reviewed set rather than whatever a renderer happened to reach
//! for.
//!
//! The accessibility property this guards is not "these six hues are
//! readable" — a terminal resolves the base eight against the user's own
//! theme, so this tool does not know what they look like. It is that no hue
//! ever carries a signal alone: severity colors the severity *word*, diffs
//! color lines that already begin with `-`/`+`. That pairing cannot be
//! checked mechanically, so what is checked is the moment it would have to be
//! decided again — the arrival of a seventh code — and that the page stating
//! the rule still matches the source.
//!
//! Read as text, like the other architecture contracts, so the check runs in
//! the Nix sandbox without building anything.

use std::collections::BTreeSet;
use std::fs;

const COLOR_SOURCE: &str = "packages/core/cli/src/color.rs";
const PALETTE_DOC: &str = "docs/src/reference/color-palette.md";

/// SGR code, the `Painter` method that emits it, and the ANSI name the doc
/// page uses for it.
const APPROVED: &[(&str, &str, &str)] = &[
    ("31", "red", "red"),
    ("33", "yellow", "yellow"),
    ("32", "green", "green"),
    ("36", "cyan", "cyan"),
    ("1", "bold", "bold"),
    ("2", "dim", "dim"),
];

fn color_source() -> String {
    fs::read_to_string(COLOR_SOURCE).unwrap_or_else(|_| panic!("read {COLOR_SOURCE}"))
}

fn approved_codes() -> BTreeSet<&'static str> {
    APPROVED.iter().map(|(code, _, _)| *code).collect()
}

/// The literal in each `self.wrap("<code>", ..)` call.
fn wrapped_codes(source: &str) -> BTreeSet<String> {
    let marker = "self.wrap(\"";
    source
        .match_indices(marker)
        .map(|(index, _)| {
            let rest = &source[index + marker.len()..];
            let end = rest
                .find('"')
                .expect("every `self.wrap(` argument is a closed string literal");
            rest[..end].to_owned()
        })
        .collect()
}

/// Every SGR escape spelled out anywhere in the file, `wrap`ped or not —
/// including the ones in its own unit tests. Catches an escape hand-rolled
/// past `wrap` as well as one passed through it.
fn escaped_codes(source: &str) -> BTreeSet<String> {
    let marker = "\\x1b[";
    source
        .match_indices(marker)
        .map(|(index, _)| {
            let rest = &source[index + marker.len()..];
            let end = rest
                .find('m')
                .expect("every SGR escape in color.rs terminates in `m`");
            rest[..end].to_owned()
        })
        .collect()
}

#[test]
fn painter_wraps_only_the_approved_sgr_codes() {
    let found = wrapped_codes(&color_source());
    let approved: BTreeSet<String> = approved_codes()
        .iter()
        .map(|&code| code.to_owned())
        .collect();

    assert_eq!(
        found, approved,
        "{COLOR_SOURCE} emits a different SGR set than the approved palette.\n\
         Adding one means answering, in {PALETTE_DOC}, what mandatory text label \
         it pairs with — color is never the only signal."
    );
}

#[test]
fn color_source_spells_out_no_unapproved_escape() {
    let source = color_source();
    let mut allowed: BTreeSet<String> = approved_codes()
        .iter()
        .map(|&code| code.to_owned())
        .collect();
    // The reset every wrapped span closes with, and `wrap`'s own format template.
    allowed.insert("0".to_owned());
    allowed.insert("{code}".to_owned());

    let found = escaped_codes(&source);
    let unapproved: Vec<&String> = found.difference(&allowed).collect();

    assert!(
        unapproved.is_empty(),
        "{COLOR_SOURCE} spells out SGR escapes outside the approved palette: {unapproved:?}"
    );
}

/// 256-color and truecolor escapes are excluded on purpose: they pin an exact
/// RGB value and override the palette a reader has already adapted to their
/// own vision. Named separately from the set check so the failure says why.
#[test]
fn no_extended_color_escapes() {
    let source = color_source();
    for parameter in ["38;", "48;", "38m", "48m"] {
        assert!(
            !source.contains(parameter),
            "{COLOR_SOURCE} uses an extended-color SGR parameter (`{parameter}`); \
             the base eight let the user's terminal theme apply their own palette"
        );
    }
}

#[test]
fn every_approved_code_has_a_painter_method() {
    let source = color_source();
    for (code, method, _) in APPROVED {
        assert!(
            source.contains(&format!("pub fn {method}(self, text: &str) -> String")),
            "{COLOR_SOURCE} has no `Painter::{method}` for the approved code `{code}`"
        );
    }
}

#[test]
fn the_palette_page_documents_every_approved_code() {
    let doc = fs::read_to_string(PALETTE_DOC).unwrap_or_else(|_| panic!("read {PALETTE_DOC}"));

    for (code, method, name) in APPROVED {
        assert!(
            doc.contains(&format!("`{code}`")),
            "{PALETTE_DOC} does not document SGR code `{code}`"
        );
        assert!(
            doc.contains(&format!("`Painter::{method}`")),
            "{PALETTE_DOC} does not name `Painter::{method}` as the emitter of `{code}`"
        );
        assert!(
            doc.contains(name),
            "{PALETTE_DOC} does not give `{code}` its ANSI name `{name}`"
        );
    }

    assert!(
        doc.contains("must pair with a mandatory text label"),
        "{PALETTE_DOC} must keep stating the rule this contract exists to defend: \
         a new color pairs with a text label that prints regardless of color"
    );
    assert!(
        doc.contains("color_palette_contract"),
        "{PALETTE_DOC} must name the test that enforces it, so a reader who \
         changes the table learns what will fail"
    );
}

#[test]
fn the_palette_page_is_navigable() {
    let nav = fs::read_to_string("docs/mkdocs.yml").expect("read docs/mkdocs.yml");
    assert!(
        nav.contains("reference/color-palette.md"),
        "docs/mkdocs.yml must list reference/color-palette.md in `nav`; \
         an unlisted page fails the --strict build"
    );
}
