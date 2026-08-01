//! `inspect lint`'s JSON envelope and the shared report envelope spell the
//! same two concepts differently, on purpose, forever.
//!
//! Every other `inspect`-family command prints through the shared envelope in
//! `packages/core/cli/src/report/render.rs`: a finding's type is the JSON key
//! `"kind"` (`file_json`), and the pass/fail gate is `policy.gate` /
//! `policy.passed` (`print_json`). `inspect lint` — the one aggregate command
//! that rolls ~133 individual rules into a single report — does not go
//! through that envelope. It has its own, older, hand-rolled printer in
//! `src/presentation/cli/lint_report/render.rs`, which spells the same two
//! concepts as `"rule"` instead of `"kind"`, and `policy.fail_on_finding`
//! instead of `policy.gate`.
//!
//! A 2026-08-01 requirements review looked at this divergence and decided
//! **not** to unify the two shapes. Changing either one is a breaking change
//! for whatever already parses `inspect lint --output json` or any of the
//! other ~167 `inspect` reports, and nothing about the divergence is a latent
//! bug — `lint` published `"rule"`/`"fail_on_finding"` first, the shared
//! envelope published `"kind"`/`"gate"` later for everything built after it,
//! and both are now load-bearing for existing consumers under their own
//! names.
//!
//! What the review flagged instead is that the divergence had never been
//! written down anywhere a test could see it, so a future rename in either
//! file — someone "cleaning up" `lint`'s field names to match the newer
//! reports, or vice versa — would silently drift the two shapes further
//! apart (or accidentally *converge* them, which is just as unreviewed a
//! change) with nothing to catch it. This test is that record: it pins the
//! four literals both files rely on today, so a future edit to either one
//! either restores the literal or is forced to consciously re-litigate the
//! "do we unify these" question the 2026-08-01 review already answered,
//! instead of drifting further as an unreviewed side effect of an unrelated
//! change.
//!
//! # This is not `report_json_key_casing_contract`
//!
//! That test (`tests/cli/report_json_key_casing_contract.rs`) pins every
//! shared-envelope key's *casing convention* (snake_case) and explicitly
//! declines to say whether `lint`'s envelope should conform to the shared
//! one — "a separate question", in its own words. This test answers a
//! different question about the same two files: not how the keys are cased,
//! but which literal words they use. It does not assert anything about
//! casing, and it does not scan the ~167 reports that already share one
//! envelope — only the one pair of files that deliberately does not.

use std::fs;
use std::path::Path;

/// The shared envelope every other `inspect` report prints through.
const SHARED_ENVELOPE: &str = "packages/core/cli/src/report/render.rs";

/// `inspect lint`'s own, separate printer.
const LINT_ENVELOPE: &str = "src/presentation/cli/lint_report/render.rs";

fn read(path: &str) -> String {
    fs::read_to_string(Path::new(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

/// The shared envelope still calls a finding's type `"kind"` and the gate
/// field `"gate"`.
///
/// `"kind"` is written in `file_json` (around line 217 at the time this test
/// was added, now checked at whatever line it has moved to); `"gate"` sits in
/// the `policy` object `print_json` builds (around line 179/236). Losing
/// either means the shared envelope changed the very fields this test exists
/// to contrast with `lint`'s — which would make the contrast wrong rather
/// than the test simply missing something.
#[test]
fn shared_envelope_still_spells_kind_and_gate() {
    let source = read(SHARED_ENVELOPE);
    assert!(
        source.contains("\"kind\""),
        "{SHARED_ENVELOPE} no longer contains the JSON key \"kind\" (expected in `file_json`, \
         where every non-lint report names a finding's type). If this is an intentional rename, \
         `inspect lint`'s \"rule\" key in {LINT_ENVELOPE} is either still a deliberate, \
         documented divergence from the new name, or the 2026-08-01 decision not to unify the \
         two envelopes needs to be revisited — update this test to reflect whichever is true, \
         do not just delete the assertion."
    );
    assert!(
        source.contains("\"gate\""),
        "{SHARED_ENVELOPE} no longer contains the JSON key \"gate\" (expected in `print_json`'s \
         `policy` object, alongside \"passed\"). If this is an intentional rename, \
         `inspect lint`'s \"fail_on_finding\" key in {LINT_ENVELOPE} is either still a \
         deliberate, documented divergence, or the non-unification decision needs revisiting."
    );
}

/// `inspect lint`'s own printer still calls a finding's rule name `"rule"`
/// and the gate field `"fail_on_finding"`.
///
/// Both are written more than once in `src/presentation/cli/lint_report/render.rs`
/// (`"rule"` in the per-rule summary and in each finding; `"fail_on_finding"`
/// in the `policy` object beside `"passed"`), so this only needs to find the
/// literal somewhere in the file rather than at one specific call site.
#[test]
fn lint_envelope_still_spells_rule_and_fail_on_finding() {
    let source = read(LINT_ENVELOPE);
    assert!(
        source.contains("\"rule\""),
        "{LINT_ENVELOPE} no longer contains the JSON key \"rule\" (expected on a lint finding, \
         where the shared envelope's equivalent report field is \"kind\"). If this is an \
         intentional rename, either it now matches the shared envelope's \"kind\" — in which \
         case the divergence this test pins is gone and the 2026-08-01 decision should be \
         revisited on purpose — or it is a different deliberate spelling, in which case update \
         this test to the new literal."
    );
    assert!(
        source.contains("\"fail_on_finding\""),
        "{LINT_ENVELOPE} no longer contains the JSON key \"fail_on_finding\" (expected in the \
         `policy` object, where the shared envelope's equivalent field is \"gate\"). Same \
         choice as above: either this is now an intentional convergence with the shared \
         envelope worth its own decision, or a still-deliberate rename this test should be \
         updated to pin instead."
    );
}

/// The two envelopes are still two different files.
///
/// If `lint` were ever migrated onto the shared envelope, these two paths
/// would collapse into one and the assertions above would stop meaning what
/// their names say — they would both be reading the same source, so a
/// passing test would prove nothing about a divergence that no longer
/// exists. That migration is exactly the kind of change the 2026-08-01
/// review would need to sign off on, not something that should be able to
/// happen underneath this test unnoticed.
#[test]
fn the_two_envelopes_are_still_separate_files() {
    assert_ne!(
        SHARED_ENVELOPE, LINT_ENVELOPE,
        "the shared envelope and lint's envelope now point at the same file; the divergence \
         this test pins as permanent no longer exists, which is a real migration decision, not \
         something this test should silently start passing over"
    );
}
