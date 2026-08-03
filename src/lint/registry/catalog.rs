//! The published rule catalogue, derived from [`super::REGISTRY`].
//!
//! Every constant here is computed at compile time by walking the registry.
//! That is the whole point: the four arrays these replace were maintained by
//! hand and could disagree — a rule present in `RULES` but missing from
//! `RULE_DOCS`, or listed in `FIXABLE_RULES` without the fix engine ever
//! producing one. There is now a single array, and the `const` assertions
//! below pin each derived length so that gaining or losing a rule is a
//! deliberate change.

use crate::lint::model::{RuleCategory, RuleExplanation, RuleSetting, RuleTag, RuleTags, Severity};

use super::{REGISTRY, RULE_COUNT};

/// Stable rule identifiers, matching each lint's own `inspect` command name.
pub const RULES: [&str; RULE_COUNT] = {
    let mut names = [""; RULE_COUNT];
    let mut index = 0;
    while index < RULE_COUNT {
        names[index] = REGISTRY[index].meta().name().as_str();
        index += 1;
    }
    names
};

/// The category names accepted by `--category`.
pub const CATEGORIES: [&str; RuleCategory::ALL.len()] = {
    let mut names = [""; RuleCategory::ALL.len()];
    let mut index = 0;
    while index < RuleCategory::ALL.len() {
        names[index] = RuleCategory::ALL[index].as_str();
        index += 1;
    }
    names
};

/// `(rule name, category, one-line description)` for each rule, in [`RULES`]
/// order. Powers `inspect lint --list-rules`, the `--category` filter, and
/// inline descriptions in the report, so an agent can discover the rule set,
/// its groupings, and its `--rule`/`--exclude`/`--category` names without
/// consulting the documentation.
pub const RULE_DOCS: [(&str, &str, &str); RULE_COUNT] = {
    let mut docs = [("", "", ""); RULE_COUNT];
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        docs[index] = (
            meta.name().as_str(),
            meta.category().as_str(),
            meta.description(),
        );
        index += 1;
    }
    docs
};

const fn fixable_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().fixability().is_fixable() {
            count += 1;
        }
        index += 1;
    }
    count
}

const fn warning_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().severity().is_warning() {
            count += 1;
        }
        index += 1;
    }
    count
}

/// The rules for which `inspect lint --fix` (and the SARIF `fixes` field) can
/// synthesize an automatic rewrite. The rest are diagnostic-only: their repair
/// depends on intent a machine cannot infer.
pub const FIXABLE_RULES: [&str; fixable_count()] = {
    let mut names = [""; fixable_count()];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.fixability().is_fixable() {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

/// The rules whose findings are warnings (correct-but-redundant/style code).
/// Every other rule is an `error` — a likely or certain bug.
pub const WARNING_RULES: [&str; warning_count()] = {
    let mut names = [""; warning_count()];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.severity().is_warning() {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

/// The tag names accepted by `--tag`.
pub const TAGS: [&str; RuleTag::ALL.len()] = {
    let mut names = [""; RuleTag::ALL.len()];
    let mut index = 0;
    while index < RuleTag::ALL.len() {
        names[index] = RuleTag::ALL[index].as_str();
        index += 1;
    }
    names
};

const fn tagged_count(tag: RuleTag) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().has_tag(tag) {
            count += 1;
        }
        index += 1;
    }
    count
}

/// The rules `--preset` keeps out of every rung but `all`, unless
/// `--experimental` is passed. Published so a caller can see what it is opting
/// into before opting in.
pub const EXPERIMENTAL_RULES: [&str; tagged_count(RuleTag::Experimental)] = {
    let mut names = [""; tagged_count(RuleTag::Experimental)];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.has_tag(RuleTag::Experimental) {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

/// The rules only `--preset pedantic` (and `all`) admits: correct, but
/// opinionated enough to be noise on a codebase that has not adopted the
/// convention.
pub const PEDANTIC_RULES: [&str; tagged_count(RuleTag::Pedantic)] = {
    let mut names = [""; tagged_count(RuleTag::Pedantic)];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.has_tag(RuleTag::Pedantic) {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

// The suite's shape, pinned. A rule added or removed without updating these is
// a compile error rather than a silently different report.
// 229 (through PR #82) + this branch's 37, spread over nine packages: 7
// (`lint-control-flow`), 6 (`lint-safety`), 5 (`lint-call-shape`), 4 each
// (`lint-conditional`, `lint-documentation`), 2 (`lint-contract-annotation`)
// and 3 each (`lint-performance`, `lint-portability`, `lint-introspection`) =
// 266.
//
// 37, not the 39 this branch first proposed. `check-type-redundant-with-declare`
// and `clojure-pre-referencing-percent` were dropped before merge once their
// premises were checked against the primary sources and refuted; both were
// false-positive generators on correct code. See
// `packages/feature/lint-contract-annotation/README.md`.
//
// 266 + the next batch's 20: 8 (`lint-form-shape`), 4 each (`lint-sequence`,
// `lint-numeric`), 3 (`lint-string-char`) and 1 (`lint-package-hygiene`) = 286.
//
// 286 + this batch's 9: 4 (`lint-elisp-idiom`) and 5 (`lint-pathname-io`),
// both new packages. Twelve rules were proposed; two were dropped as exact
// duplicates of rules already shipping, two more had their premises refuted
// against a running Emacs, and `elisp-keymap-binds-non-command` was dropped
// after implementation when a false-positive audit over GNU Emacs's own
// sources returned 70 findings and 0 true positives.
//
// 295 + this batch's 8: 4 (`lint-clojure-idiom`) and 4 (`lint-scheme-idiom`),
// both new packages, neither of which runs on Common Lisp. All 8 ship with a
// standalone `inspect <rule>` command as well, so `INTROSPECTION_COMMANDS`
// moves with this number where the batch above left it alone.
//
// 303 + this batch's 10: 5 (`lint-fennel-janet-idiom`) and 5
// (`lint-type-declaration`), both new packages. Neither ships a `cli/`
// directory, so unlike the batch above this one adds no standalone command and
// `INTROSPECTION_COMMANDS` stays where it was. `lint-type-declaration`
// proposed six and ships five: `ignore-declared-variable-then-used` was dropped
// as a true duplicate of `lint-convention`'s `ignore-declaration-conflict`.
//
// 313 + this batch's 3: 3 (`lint-compile-time`), one new package, Common Lisp
// only. It ships a `cli/` directory per rule, so all three add a standalone
// command and `INTROSPECTION_COMMANDS` moves with this number (337 -> 340), as
// it did two batches ago and did not one batch ago.
//
// 316 + this batch's 4: 4 (`lint-hy-lfe-idiom`), one new package, and back to
// registry-only — no `cli/` directory, so no standalone command and
// `INTROSPECTION_COMMANDS` stays at 340 where the batch above left it. Three of
// the four are Hy and the fourth is LFE; none of them runs on Common Lisp.
const _: () = assert!(RULE_COUNT == 320);
// Unchanged at 99: every one of this branch's 37 rules is
// `Fixability::ReportOnly`. Each one reports a judgment the tool cannot make
// on the author's behalf — whether an annotation or the parameter list under it
// is the wrong half, which of two nested `cond`s the author meant to keep, or
// how a temp file should be named are all decisions the author has to make,
// not spellings of one they already made. The two dropped rules were
// `ReportOnly` too, which is why this number does not move with them.
//
// Still 99 after the next batch's 20: every one of those is `ReportOnly` as
// well. Several say so in their own words — `redundant-precision-coercion`
// records that removing the coercion changes the result on exactly the inputs
// that motivate the rule, and `package-circular-in-package-chain` would have to
// move forms between two regions of one package to repair anything.
//
// Still 99 after this batch's 10. `elisp-require-obsolete-cl` is the
// interesting one: rewriting `(require 'cl)` to `(require 'cl-lib)` looks
// mechanical but is not, because the unprefixed names `cl.el` provides do not
// exist in `cl-lib`. Offering that fix would make a working file stop working,
// so the rule says so in its own header and stays `ReportOnly`.
//
// 99 + 4 of this batch's 8 = 103, and this is the first batch in a long while
// to move the number at all. The four `lint-scheme-idiom` rules are all
// `Fixability::Fixable` because each repair is a spelling of a decision the
// author already made rather than a new one: unwrapping a one-form `begin`,
// `let*` to `let` when no initializer can see a sibling binding, `memq`/`assq`
// to the `eqv?`-based `memv`/`assv` R7RS 6.4 specifies, and deleting a loop
// name the body never mentions. Every fix rewrites a head symbol or copies an
// inner span verbatim, so comments and spacing survive it. The four
// `lint-clojure-idiom` rules are `ReportOnly`: each of them has a repair with
// more than one shape (`doall` versus `into` versus `reduce`; `let` versus a
// top-level `defonce`), and picking one would be picking for the author.
//
// Back to standing still: 103 after this batch's 10, every one of which is
// `Fixability::ReportOnly`. The two packages decline for the same reason from
// opposite ends. The Fennel/Janet rules report a mismatch between a spelling
// and an intent — a `var` that could be a `local`, a `{…}` that should have
// been `@{…}` — where the tool cannot tell which half the author meant, and
// `var-never-set` says outright that its assignment search is blind to scope
// and quoting. The declaration rules are the same shape: which of the type and
// the initform is wrong, or whether a late `declare` wanted hoisting or wanted
// to be a `the`, is the author's call and neither repair is right more often
// than the other.
//
// Still 103 after this batch's 3, all three `Fixability::ReportOnly` and all
// three for the same reason: the offending form says nothing about what was
// meant. A `(eval-when (:execute) …)` at top level may have wanted
// `:load-toplevel`, or may have wanted to be hoisted out of a macro that put it
// there; a nested `eval-when` the standard ignores may have wanted `:execute`
// or may want deleting outright; and a `defconstant` whose initform allocates
// may want `defparameter`, `alexandria:define-constant` with a `:test`, or a
// genuinely `eql`-able value. Rewriting any of them is guessing.
//
// Still 103 after this batch's 4, every one `Fixability::ReportOnly`. The Hy
// three each have a repair with more than one shape: a shared mutable default
// can become `None` plus a body guard or can be hoisted deliberately, `is`
// against a literal may have wanted `=` or may have wanted a different operand,
// and a bare `(except [] …)` has to name the exceptions the author actually
// meant to handle, which the tool cannot know. `lfe-catch-swallows-exit` is the
// clearest of the four: rewriting `(catch Expr)` to `try … catch` requires
// inventing the failure continuation the `catch` form never had.
const _: () = assert!(fixable_count() == 103);
// 164 (through PR #82) + 31 of this branch's 37 rules. The other 6 are
// `Severity::Error`: `when-unless-implicit-nil-misused` and the five
// `lint-safety` rules that report an exploitable defect rather than a risk —
// `format-tilde-slash-unvalidated-function-designator`,
// `path-traversal-via-concatenated-filename`, `read-eval-star-rebound-to-t`,
// `sql-query-string-built-via-format` and
// `world-writable-file-mode-in-open-call`. Both dropped rules were `Warning`,
// so this fell by 2 where `fixable_count` did not.
//
// 195 + 17 of the next batch's 20 = 212. The other 3 are `Severity::Error`,
// each because a real implementation refuses the code rather than merely
// disliking it: `quoted-form-contains-stray-unquote` (SBCL will not *read* the
// file — "Comma not inside a backquote"), `format-nested-directive-unbalanced`
// (`format` signals at run time on an unclosed `~[`/`~{`/`~<`/`~(`), and
// `ftype-values-arity-mismatch` (a violated `ftype` is undefined behaviour at
// low safety, and SBCL raises a full WARNING for it).
//
// 212 + 7 of this batch's 9 = 219. The other 2 are `Severity::Error` because
// the code fails outright rather than reading badly:
// `elisp-interactive-arity-mismatch` (`commandp` is true, so the command
// appears in `M-x` and then signals `wrong-number-of-arguments`) and
// `with-open-file-result-captures-stream` (the stream is closed before the
// value that carries it is ever used). Dropping
// `elisp-keymap-binds-non-command` did not move this number: it was an
// `Error` too.
//
// 219 + 6 of this batch's 8 = 225. The other 2 are `Severity::Error`, both
// from `lint-clojure-idiom` and both because the code throws rather than reads
// badly: `with-open-returns-lazy-seq` (the sequence is realized after the
// resource closes, so `IOException: Stream closed`) and
// `def-inside-function-body` (the Var does not exist until the function runs,
// and concurrent callers race on it — which is also clj-kondo's judgement,
// where `:inline-def` is on by default).
//
// 225 + 7 of this batch's 10 = 232. The other 3 are `Severity::Error`, each
// because a real implementation refuses or dies on the code rather than merely
// disliking it: `fennel-each-over-non-iterator` (`each` compiles to Lua's
// generic `for … in`, which *calls* the iterator, so a table or string literal
// raises "attempt to call a table value" on the first round),
// `janet-mutating-immutable-literal` (`put` and the `array/*` and `buffer/*`
// families panic when handed the immutable twin of a mutable container — a
// dropped `@` is code that reads correctly and dies on its first call), and
// `declare-not-at-head-of-body` (past the first body form a `(declare …)` is
// not a declaration at all but a call to an undefined function, which SBCL
// 2.6.0 reports as a full `caught ERROR`). `var-never-set` carries
// `RuleTag::Style`, which is not `Pedantic` and so does not hold it back from
// any preset; it counts as a warning here like the other 6.
//
// This batch's 3 leave it at 232: all three are `Severity::Error`, and each was
// run through SBCL 2.6.0 under both `load` of the source and `compile-file`
// plus `load` of the fasl before being given that severity. The two phases
// disagree, or the form is dead in both: `(eval-when (:execute) (defmacro m …))`
// loads fine from source and gives an *undefined function* at run time from a
// fasl; a non-top-level `eval-when` without `:execute` never runs in either
// phase and SBCL says nothing at all; and `(defconstant +x+ #("a" "b"))`
// signals `DEFCONSTANT-UNEQL` on the compile-then-load path. None of the three
// is a preference.
//
// 232 + 3 of this batch's 4 = 235. The fourth,
// `hy-mutable-default-argument`, is `Severity::Error`: measured against Hy
// 1.3.1 on CPython 3.14.6, `(defn f [[acc []]] (.append acc 1) acc)` returns
// `[1, 1, 1]` from its *third* call, so the function gives a wrong answer
// rather than reading badly. The three warnings are risky spellings whose
// behaviour is at least defensible — `is` against a literal answers correctly
// while the value stays inside CPython's interning range, `(except [] …)` does
// catch the exceptions it was written for as well as the two it should not, and
// `(catch Expr)` returns a term the caller *could* discriminate if it took the
// trouble. Note `lfe-catch-swallows-exit` is one of the 3: it is a warning that
// also carries `RuleTag::Pedantic`, so it counts here and is subtracted again
// in the preset-filtered count `tests/cli/lint_report.rs` pins.
const _: () = assert!(warning_count() == 235);
const _: () = assert!(EXPERIMENTAL_RULES.is_empty());
// 6 (through PR #82) + 8 of this branch's rules: `lint-call-shape`'s four
// threshold rules, whose limits are conventions a codebase either adopted or
// did not; `lint-documentation`'s `docstring-summary-line-too-long`,
// `missing-package-docstring` and `todo-fixme-no-attribution`; and
// `repeated-hash-table-lookup-same-key`, which is a real cost only on a hot
// path the rule cannot identify. Neither dropped rule was tagged `pedantic`,
// so this does not move either. Nor does the next batch's 20: none of them
// carries a tag at all, so `PEDANTIC_RULES` and `EXPERIMENTAL_RULES` are both
// unchanged by it. This batch moves it by one: `elisp-hook-lambda` is tagged
// `pedantic` because it is correct and still noisy — 106 findings over the
// 1654 `.el` files GNU Emacs 30.2 ships, 31 of them on buffer-local hooks
// where the lambda's buffer is about to be discarded and the re-evaluation
// problem it warns about cannot arise. The other 8 report a defect with a
// demonstrated failure and stay untagged. This batch's 8 leave it at 15 as
// well: none of them carries a tag, so `PEDANTIC_RULES` and
// `EXPERIMENTAL_RULES` are both unchanged by it.
// This batch's 10 leave it at 15 too. Nine carry no tag at all, and the tenth,
// `var-never-set`, carries `RuleTag::Style` — which says the rule's subject is
// layout or naming rather than behaviour, and which no preset filters on. Only
// `Pedantic` and `Experimental` gate admission, so `PEDANTIC_RULES` and
// `EXPERIMENTAL_RULES` are both unchanged.
// This batch's 3 leave it at 15 as well: none of them carries a tag at all, so
// `PEDANTIC_RULES` and `EXPERIMENTAL_RULES` are both unchanged by it.
// This batch's 4 moves it by one, to 16 — the first movement since
// `elisp-hook-lambda`, and for the same reason. `lfe-catch-swallows-exit` is
// tagged `pedantic` because the mechanism is real and measured (an exit and an
// honest `(tuple 'EXIT 'boom)` produce the identical term under LFE 2.2.0, so
// the caller cannot tell failure from success) and the rule is still noisy on a
// codebase that uses `catch` as its house style: 146 findings over 2604
// third-party `.lfe` files, of which 28 sit under an enclosing `case` that is
// explicitly discriminating `#(EXIT …)` and 28 more are in statement position
// with the value discarded — best-effort telemetry and optional startup. What
// kept it out of the bin rather than out of `recommended` alone is the
// *spread*: the findings fall in 9 of the corpus's ~143 repositories, so 94% of
// repositories are clean and two of the three heaviest are LFE's own
// implementation. Correct, and noise on a codebase that has not adopted the
// convention — which is the tag's definition. The other 3 carry no tag, and
// `EXPERIMENTAL_RULES` is still empty.
const _: () = assert!(PEDANTIC_RULES.len() == 16);

fn meta_of(name: &str) -> Option<&'static crate::lint::model::RuleMeta> {
    REGISTRY
        .iter()
        .map(super::RuleEntry::meta)
        .find(|meta| meta.name().as_str() == name)
}

/// The one-line description for a rule name, or `None` if the name is unknown.
#[must_use]
pub fn rule_description(name: &str) -> Option<&'static str> {
    meta_of(name).map(|meta| meta.description())
}

/// The category for a rule name, or `None` if the name is unknown.
#[must_use]
pub fn rule_category(name: &str) -> Option<RuleCategory> {
    meta_of(name).map(|meta| meta.category())
}

/// Whether `inspect lint --fix` can repair this rule's findings.
#[must_use]
pub fn rule_is_fixable(name: &str) -> bool {
    meta_of(name).is_some_and(|meta| meta.fixability().is_fixable())
}

/// The severity of a rule's findings (`error` unless it is a style rule).
///
/// An unknown name reports `Error`, matching the historical `contains`-based
/// lookup that treated anything not listed as a warning as an error.
#[must_use]
pub fn rule_severity(name: &str) -> Severity {
    meta_of(name).map_or(Severity::Error, |meta| meta.severity())
}

/// The orthogonal properties of a rule; empty for an unknown name and for the
/// majority of rules, which carry none.
#[must_use]
pub fn rule_tags(name: &str) -> RuleTags {
    meta_of(name).map_or(RuleTags::NONE, |meta| meta.tags())
}

/// The long-form documentation `--explain` prints, or `None` when the rule
/// supplies only its one-line description.
#[must_use]
pub fn rule_explanation(name: &str) -> Option<RuleExplanation> {
    meta_of(name).and_then(|meta| meta.explanation())
}

/// The tunable knobs a rule declares, empty for the rules that have none and
/// for an unknown name.
#[must_use]
pub fn rule_settings(name: &str) -> &'static [RuleSetting] {
    meta_of(name).map_or(&[], |meta| meta.settings())
}

/// The knob `key` of `rule`, or `None` — the lookup `--rule-arg` validates
/// against before a run starts.
#[must_use]
pub fn rule_setting(rule: &str, key: &str) -> Option<RuleSetting> {
    meta_of(rule).and_then(|meta| meta.setting(key))
}

/// The dialects a rule reports on, as wire names. Part of `--explain` because
/// "why did this rule find nothing?" is most often answered by the file's
/// dialect, not by the rule's logic.
#[must_use]
pub fn rule_dialects(name: &str) -> Vec<&'static str> {
    use paredit_core_syntax::dialect::Dialect;
    REGISTRY
        .iter()
        .find(|entry| entry.meta().name().as_str() == name)
        .map(|entry| {
            let scope = entry.rule().dialect_scope();
            Dialect::ALL
                .iter()
                .filter(|dialect| scope.includes(**dialect))
                .map(|dialect| dialect.label())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_name_is_unique() {
        let mut names = RULES.to_vec();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn rule_docs_stay_in_lockstep_with_rules() {
        let names: Vec<&str> = RULE_DOCS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names, RULES.to_vec());
        for (name, category, description) in RULE_DOCS {
            assert!(
                CATEGORIES.contains(&category),
                "{name} has a stray category"
            );
            assert!(!description.is_empty(), "{name} has no description");
        }
    }

    #[test]
    fn derived_subsets_are_subsets_in_rules_order() {
        let fixable: Vec<&str> = RULES
            .iter()
            .copied()
            .filter(|rule| rule_is_fixable(rule))
            .collect();
        assert_eq!(fixable, FIXABLE_RULES.to_vec());
        let warnings: Vec<&str> = RULES
            .iter()
            .copied()
            .filter(|rule| rule_severity(rule) == Severity::Warning)
            .collect();
        assert_eq!(warnings, WARNING_RULES.to_vec());
    }

    #[test]
    fn an_unknown_rule_name_resolves_to_nothing() {
        assert_eq!(rule_description("no-such-rule"), None);
        assert_eq!(rule_category("no-such-rule"), None);
        assert!(!rule_is_fixable("no-such-rule"));
    }
}
