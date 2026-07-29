//! What a lint run produces: findings, and the summary a report renders.

use std::path::PathBuf;

use paredit_core_syntax::sexpr::ByteSpan;

use super::RuleFix;

/// One rule's complaint about one form.
///
/// The field shapes are the published contract of `inspect lint` (text, JSON,
/// and SARIF all project directly from them) and are deliberately primitive:
/// `rule` stays a `&'static str` so consumers outside the domain need no
/// domain types to read a report.
#[derive(Debug, Clone)]
pub struct LintFinding {
    pub rule: &'static str,
    pub path: PathBuf,
    pub span: ByteSpan,
    pub message: String,
}

/// How much of a reported form's text the identity hash reads.
///
/// The whole form would make the id change whenever *anything* inside a large
/// `defun` is edited, which defeats the purpose: a baseline entry would go
/// stale for a finding nobody touched. The opening of the form is enough to
/// tell two findings apart and is what a reader would quote to name it.
const FINGERPRINT_BYTES: usize = 160;

/// A finding's position-independent identity.
///
/// The baseline and the suppression file both need to answer "is this the same
/// finding as last time?" across edits. Byte offsets answer it wrongly —
/// inserting a line above a finding moves every offset below it — and line
/// numbers only slightly less wrongly.
///
/// So the identity is derived from what the finding *is*: its rule, and a
/// whitespace-normalized prefix of the form it reports. Two findings that
/// survive that normalization identically are disambiguated by an occurrence
/// counter, which is the only positional input and is scoped to a single
/// `(path, rule, fingerprint)` group — so it only shifts when a genuine
/// duplicate is added or removed next to it.
///
/// Normalizing whitespace is what makes the id survive `paredit format`:
/// reindenting a form changes every line of it and none of its identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FindingId(String);

impl FindingId {
    /// Derives the id of `rule`'s `occurrence`-th finding over `form_source`,
    /// the exact source text of the reported span.
    #[must_use]
    pub fn new(rule: &str, form_source: &str, occurrence: usize) -> Self {
        let hash = fnv1a64(&normalize(form_source));
        Self(format!("{rule}/{hash:016x}/{occurrence}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FindingId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Collapses every run of whitespace to one space and keeps the opening
/// [`FINGERPRINT_BYTES`] bytes, truncated on a char boundary so the result is
/// always valid UTF-8.
fn normalize(source: &str) -> String {
    let mut normalized = String::with_capacity(source.len().min(FINGERPRINT_BYTES));
    let mut pending_space = false;
    for ch in source.chars() {
        if ch.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            if normalized.len() + 1 > FINGERPRINT_BYTES {
                break;
            }
            normalized.push(' ');
            pending_space = false;
        }
        if normalized.len() + ch.len_utf8() > FINGERPRINT_BYTES {
            break;
        }
        normalized.push(ch);
    }
    normalized
}

/// FNV-1a/64, matching `paredit_core_cli::shared::stable_text_hash` so the two
/// stable identities in the tool are the same function. Reimplemented rather
/// than imported because the engine must not depend on the CLI package.
fn fnv1a64(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A finding together with the rewrite its rule can apply, if any.
///
/// Findings and fixes are produced by the same visit rather than by two passes,
/// because a fix generally needs data the finding does not carry (the inner
/// span to splice, the complement operator to substitute). Re-deriving that
/// from a `LintFinding` alone is impossible; re-walking the tree to recover it
/// is what the old presentation-layer fix collector did.
#[derive(Debug, Clone)]
pub struct LintOutcome {
    finding: LintFinding,
    fix: Option<RuleFix>,
}

impl LintOutcome {
    #[must_use]
    pub const fn new(finding: LintFinding, fix: Option<RuleFix>) -> Self {
        Self { finding, fix }
    }

    #[must_use]
    pub fn into_parts(self) -> (LintFinding, Option<RuleFix>) {
        (self.finding, self.fix)
    }
}

/// The findings of one run, filtered to the active rules, plus the per-rule
/// checklist a report prints.
#[derive(Debug)]
pub struct LintSummary {
    pub finding_count: usize,
    /// Count of findings per rule, in `RULES` order (rules with zero findings
    /// are included so a consumer sees the full checklist).
    pub per_rule: Vec<(&'static str, usize)>,
    pub findings: Vec<LintFinding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindenting_a_form_does_not_change_its_id() {
        let tight = FindingId::new("redundant-the", "(the t (compute x))", 0);
        let loose = FindingId::new("redundant-the", "(the   t\n   (compute\n     x))", 0);
        assert_eq!(tight, loose);
    }

    #[test]
    fn a_different_form_gets_a_different_id() {
        let one = FindingId::new("redundant-the", "(the t (compute x))", 0);
        let other = FindingId::new("redundant-the", "(the t (compute y))", 0);
        assert_ne!(one, other);
    }

    #[test]
    fn the_same_form_under_a_different_rule_gets_a_different_id() {
        let one = FindingId::new("redundant-the", "(the t x)", 0);
        let other = FindingId::new("the-arity", "(the t x)", 0);
        assert_ne!(one, other);
    }

    #[test]
    fn identical_findings_are_separated_by_their_occurrence() {
        let first = FindingId::new("redundant-quote", "'5", 0);
        let second = FindingId::new("redundant-quote", "'5", 1);
        assert_ne!(first, second);
        assert!(first.as_str().ends_with("/0"));
        assert!(second.as_str().ends_with("/1"));
    }

    #[test]
    fn the_id_names_its_rule_so_a_reader_can_act_on_it_alone() {
        let id = FindingId::new("zero-divisor", "(/ x 0)", 0);
        assert!(id.as_str().starts_with("zero-divisor/"));
        assert_eq!(id.to_string(), id.as_str());
    }

    #[test]
    fn two_long_forms_that_differ_only_past_the_prefix_share_an_id() {
        // The documented trade: reading only the opening is what keeps the id
        // stable under edits deep inside a large definition. Findings that
        // collide here are still separated by their occurrence counter.
        let prefix = "(defun handler (request response) ";
        let padding = "(ignore-me x) ".repeat(20);
        let one = FindingId::new("empty-body", &format!("{prefix}{padding}(a))"), 0);
        let other = FindingId::new("empty-body", &format!("{prefix}{padding}(b))"), 0);
        assert_eq!(one, other);
    }

    #[test]
    fn a_multibyte_form_truncates_on_a_char_boundary() {
        // A panic here would be a truncation that split a UTF-8 sequence.
        let source = format!("(princ \"{}\")", "日".repeat(200));
        let id = FindingId::new("some-rule", &source, 0);
        assert!(id.as_str().starts_with("some-rule/"));
    }
}
