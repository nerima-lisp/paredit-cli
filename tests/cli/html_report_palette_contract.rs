//! docs/src/project/feature-candidates.md section L2: the HTML report's
//! `.gate.fail`/`.gate.pass` pair must not be a plain red/green distinction.
//!
//! `--output html` is a separate rendering surface from the terminal palette
//! guarded by `color_palette_contract.rs`: it emits CSS, not SGR escapes, and
//! a browser renders a pinned hex value exactly as given rather than
//! remapping it through a user theme the way a terminal does. That is why
//! this surface is held to the Okabe-Ito colorblind-safe palette (vermillion
//! `#D55E00` / blue `#0072B2`) instead of red/green, even though the gate
//! line's `passed`/`failed` text already makes the pair unambiguous without
//! color.
//!
//! Read as text, like the other architecture contracts, so the check runs in
//! the Nix sandbox without building anything.

use std::fs;

const INTEROP_SOURCE: &str = "packages/core/cli/src/report/interop.rs";
const PALETTE_DOC: &str = "docs/src/reference/color-palette.md";

const GATE_FAIL_HEX: &str = "#D55E00";
const GATE_PASS_HEX: &str = "#0072B2";

fn interop_source() -> String {
    fs::read_to_string(INTEROP_SOURCE).unwrap_or_else(|_| panic!("read {INTEROP_SOURCE}"))
}

#[test]
fn html_style_uses_the_approved_gate_colors() {
    let source = interop_source();
    assert!(
        source.contains(&format!(".gate.fail{{color:{GATE_FAIL_HEX}}}")),
        "{INTEROP_SOURCE} must style `.gate.fail` with the approved \
         colorblind-safe hex {GATE_FAIL_HEX}"
    );
    assert!(
        source.contains(&format!(".gate.pass{{color:{GATE_PASS_HEX}}}")),
        "{INTEROP_SOURCE} must style `.gate.pass` with the approved \
         colorblind-safe hex {GATE_PASS_HEX}"
    );
}

/// `#b00`/`#070` is the red/green pair this contract replaced; it must not
/// come back as a hand-rolled shorthand alternative to the approved hexes.
#[test]
fn html_style_does_not_regress_to_plain_red_green() {
    let source = interop_source();
    assert!(
        !source.contains(".gate.fail{color:#b00}"),
        "{INTEROP_SOURCE} must not style `.gate.fail` with the plain red \
         `#b00`; deuteranopia/protanopia cannot distinguish it from \
         `.gate.pass`'s green"
    );
    assert!(
        !source.contains(".gate.pass{color:#070}"),
        "{INTEROP_SOURCE} must not style `.gate.pass` with the plain green \
         `#070`; deuteranopia/protanopia cannot distinguish it from \
         `.gate.fail`'s red"
    );
}

#[test]
fn gate_line_always_prints_a_text_label_beside_the_color() {
    let source = interop_source();
    assert!(
        source.contains("if flat.gate_passed { \"pass\" } else { \"fail\" }"),
        "{INTEROP_SOURCE}'s gate line must keep deriving its CSS class from \
         `gate_passed`, not a hardcoded color"
    );
    assert!(
        source.contains("if flat.gate_passed { \"passed\" } else { \"failed\" }"),
        "{INTEROP_SOURCE}'s gate line must keep printing the literal \
         `passed`/`failed` word alongside the colored class — color is \
         never the only signal, on this surface either"
    );
}

#[test]
fn the_palette_page_documents_the_html_gate_colors() {
    let doc = fs::read_to_string(PALETTE_DOC).unwrap_or_else(|_| panic!("read {PALETTE_DOC}"));

    for hex in [GATE_FAIL_HEX, GATE_PASS_HEX] {
        assert!(
            doc.contains(&format!("`{hex}`")),
            "{PALETTE_DOC} does not document the HTML gate color `{hex}`"
        );
    }
    assert!(
        doc.contains(".gate.fail") && doc.contains(".gate.pass"),
        "{PALETTE_DOC} must name the `.gate.fail`/`.gate.pass` CSS classes \
         the two hex values apply to"
    );
    assert!(
        doc.contains("html_report_palette_contract"),
        "{PALETTE_DOC} must name the test that enforces the HTML palette, \
         so a reader who changes it learns what will fail"
    );
}
