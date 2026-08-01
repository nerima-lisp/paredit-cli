//! `cargo xtask new-lint-rule` — scaffolds one rule inside an existing
//! `lint-<theme>` package (`domain`/`usecase`/`rule` plus the four-file
//! `cli` module, in the shape every rule in the suite already uses) and
//! performs the two registrations that are safe to automate because they
//! are pure arithmetic over a single well-known pattern:
//!
//! - `RULE_COUNT` and the `REGISTRY` array in
//!   `src/lint/registry/mod.rs`
//! - the matching `fixable_count()`/`warning_count()` assertions in
//!   `src/lint/registry/catalog.rs`, derived from the `Fixability`/
//!   `Severity` this rule's own generated `META` declares
//!
//! Everything past that — the standalone `inspect <rule>` command's six-file
//! wiring, and the rule-count prose scattered across docs/ — is printed as a
//! checklist by `crate::checklist`, not edited, for the reason explained
//! there.

use std::fs;
use std::path::Path;

use crate::error::Result;

use crate::case::Name;
use crate::checklist::{self, Namespace};
use crate::fs_util::{insert_sorted_mod_line, write_new_file};
use crate::repo::Repo;

use paredit_feature_lint_custom::pattern;
use paredit_feature_lint_custom::ruleset::{self, CustomRule, RuleTest};

pub struct NewLintRuleOptions {
    pub theme: String,
    pub name: Name,
    pub description: String,
    /// Set by `--from-custom-rule <path>#<name>`, to seed the scaffold's
    /// category/severity/pattern/message/fix and its generated tests.
    pub seed: Option<CustomRuleSeed>,
}

/// A `.paredit/rules/*.lisp` `defrule` (or `deprecate`) loaded to seed a new
/// scaffold, plus whatever `deftest` cases named it.
///
/// This reuses `paredit-feature-lint-custom`'s own reader
/// (`packages/feature/lint-custom/src/ruleset.rs`) rather than parsing Lisp a
/// second time: that file's grammar is the one this project's custom rules
/// actually get read with, so a second reader could silently disagree with it
/// about what a rule file means.
#[derive(Debug)]
pub struct CustomRuleSeed {
    /// The `<path>#<name>` this was loaded from, for the generated doc
    /// comment to attribute the seed to.
    pub spec: String,
    pub rule: CustomRule,
    pub tests: Vec<RuleTest>,
}

impl CustomRuleSeed {
    /// Parses `<path>#<name>`, reads `path`, and locates the rule named
    /// `name` inside it — failing with a clear, actionable message rather
    /// than panicking, since a dev tool with a bad `--from-custom-rule` value
    /// is a user error, not a defect.
    pub fn load(spec: &str) -> Result<Self> {
        let (path, name) = spec.split_once('#').ok_or_else(|| {
            crate::error::XtaskError::refused(format!(
                "--from-custom-rule wants `<path>#<name>`, e.g. \
                 `.paredit/rules/entity.lisp#entity-needs-table` — got `{spec}` (no `#`)"
            ))
        })?;
        if name.is_empty() {
            return Err(crate::error::XtaskError::refused(format!(
                "--from-custom-rule {spec} names no rule after `#`"
            )));
        }

        let text = fs::read_to_string(path)
            .map_err(crate::error::XtaskError::at("read", Path::new(path)))?;
        let parsed = ruleset::parse_ruleset(path, &text).map_err(|error| {
            crate::error::XtaskError::refused(format!(
                "{path} does not parse as a `.paredit/rules` file: {error}"
            ))
        })?;

        let rule = parsed
            .rules
            .into_iter()
            .find(|rule| rule.name == name)
            .ok_or_else(|| {
                crate::error::XtaskError::refused(format!(
                    "no `(defrule {name} ...)` (or `(deprecate {name} ...)`) found in {path}"
                ))
            })?;
        let tests = parsed
            .tests
            .into_iter()
            .filter(|test| test.rule == name)
            .collect();
        Ok(Self {
            spec: spec.to_owned(),
            rule,
            tests,
        })
    }
}

/// A one-line doc comment, since the seeded description/message/pattern text
/// came from a Lisp string that technically could carry an embedded newline.
fn oneline(text: &str) -> String {
    text.replace('\n', " ")
}

/// The doc-comment block placed just above the generated `examine()`, when a
/// seed was given: the seeded pattern/message/fix, rendered back to Lisp text
/// so the scaffold author has the custom rule's own words as a starting
/// point rather than starting from nothing.
fn seed_examine_notes(seed: &CustomRuleSeed) -> String {
    let spec = &seed.spec;
    let pattern_text = pattern::render(&seed.rule.pattern, &pattern::Bindings::new());
    let mut notes = format!(
        "/// Seeded from `{spec}`:\n\
         /// - pattern: `{}`\n\
         /// - message: {:?}\n",
        oneline(&pattern_text),
        oneline(&seed.rule.message),
    );
    if let Some(fix) = &seed.rule.fix {
        let fix_text = pattern::render(fix, &pattern::Bindings::new());
        notes.push_str(&format!(
            "/// - fix: `{}` (not ported — this scaffold has no fix support yet;\n\
             ///   see `Fixability` on `META` below if you add one)\n",
            oneline(&fix_text),
        ));
    }
    notes.push_str("///\n");
    notes
}

/// Generated `#[test]` functions for each seeded `:matches`/`:no-match` input,
/// appended inside the scaffold's `mod tests`.
///
/// Each one already compiles and passes: `examine` is still a stub that finds
/// nothing, so every generated assertion says exactly that, with a TODO
/// telling the scaffold author which way to flip it once `examine` is real.
/// `:fix` cases are not turned into tests — this scaffold has no fix support
/// to wire them against — and are listed instead as a trailing comment.
fn seed_test_functions(seed: &CustomRuleSeed, snake: &str) -> String {
    let mut out = String::new();
    let mut index = 0usize;
    for test in &seed.tests {
        for source in &test.matches {
            out.push_str(&format!(
                "\n    #[test]\n\
                 \x20   fn todo_seed_matches_{index}() {{\n\
                 \x20       // Seeded from a `(:matches ...)` case. TODO: once `examine` is\n\
                 \x20       // implemented, this input must be flagged — replace\n\
                 \x20       // `assert!(violations.is_empty())` below with the opposite.\n\
                 \x20       let source = {source:?};\n\
                 \x20       let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)\n\
                 \x20           .expect(\"parse\");\n\
                 \x20       let (_, violations) =\n\
                 \x20           collect_{snake}(&PathBuf::from(\"seed.lisp\"), Dialect::CommonLisp, &tree)\n\
                 \x20               .expect(\"collect {snake}\");\n\
                 \x20       assert!(violations.is_empty(), \"TODO: examine() is still a stub\");\n\
                 \x20   }}\n"
            ));
            index += 1;
        }
    }
    index = 0;
    for test in &seed.tests {
        for source in &test.no_match {
            out.push_str(&format!(
                "\n    #[test]\n\
                 \x20   fn todo_seed_no_match_{index}() {{\n\
                 \x20       // Seeded from a `(:no-match ...)` case: `examine` must keep not\n\
                 \x20       // flagging this input once it is implemented.\n\
                 \x20       let source = {source:?};\n\
                 \x20       let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)\n\
                 \x20           .expect(\"parse\");\n\
                 \x20       let (_, violations) =\n\
                 \x20           collect_{snake}(&PathBuf::from(\"seed.lisp\"), Dialect::CommonLisp, &tree)\n\
                 \x20               .expect(\"collect {snake}\");\n\
                 \x20       assert!(violations.is_empty(), \"TODO: examine() is still a stub\");\n\
                 \x20   }}\n"
            ));
            index += 1;
        }
    }

    let fixes: Vec<&(String, String)> = seed.tests.iter().flat_map(|test| &test.fixes).collect();
    if !fixes.is_empty() {
        out.push_str("\n    // TODO: this rule's `.paredit/rules` seed also declared fix cases,\n");
        out.push_str("    // which this scaffold has no fix support to turn into tests yet:\n");
        for (before, after) in fixes {
            out.push_str(&format!(
                "    // - {:?} -> {:?}\n",
                oneline(before),
                oneline(after),
            ));
        }
    }

    out
}

pub fn run(repo: &Repo, options: &NewLintRuleOptions) -> Result<()> {
    let package = format!("lint-{}", options.theme);
    let package_dir = repo.feature_package(&package)?;
    let crate_name = format!("paredit_feature_lint_{}", options.theme.replace('-', "_"));
    let snake = options.name.snake();
    let pascal = options.name.pascal();
    let kebab = options.name.kebab();
    let description = &options.description;
    let rule_dir = package_dir.join("src").join(&snake);

    if rule_dir.exists() {
        return Err(crate::error::XtaskError::refused(format!(
            "{} already exists",
            rule_dir.display()
        )));
    }

    // Absent a seed, this generates exactly what it always has:
    // `Suspicious`/`Warning`, no extra doc notes, no extra tests.
    let category_ident = options.seed.as_ref().map_or_else(
        || "Suspicious".to_owned(),
        |seed| format!("{:?}", seed.rule.category),
    );
    let severity_ident = options.seed.as_ref().map_or_else(
        || "Warning".to_owned(),
        |seed| format!("{:?}", seed.rule.severity),
    );
    // `catalog.rs` only tracks a `warning_count()` (see `bump_catalog_counts`
    // below) — an `Error`-severity seed must not bump it, or the pinned
    // assertion goes stale the moment this scaffold is generated.
    let severity_is_warning = severity_ident == "Warning";
    let seed_notes = options
        .seed
        .as_ref()
        .map_or_else(String::new, seed_examine_notes);
    let seed_tests = options
        .seed
        .as_ref()
        .map_or_else(String::new, |seed| seed_test_functions(seed, &snake));

    write_new_file(
        &rule_dir.join("mod.rs"),
        &format!(
            "//! The `{kebab}` lint rule: its adapter, detection, use case and command.\n\
             //!\n\
             //! One rule, one directory. `rule` is what the registry registers; the other\n\
             //! three are the report it drives.\n\n\
             pub mod cli;\n\
             pub mod domain;\n\
             pub mod rule;\n\
             pub mod usecase;\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("domain.rs"),
        &format!(
            "//! `{kebab}` detection: TODO — {description}.\n\
             //!\n\
             //! Generated by `cargo xtask new-lint-rule`. Replace `examine`'s body with the\n\
             //! real detection, then update the item/summary fields it needs to report.\n\n\
             use std::path::{{Path, PathBuf}};\n\n\
             use paredit_core_lint_engine::LintResult;\n\n\
             use paredit_core_syntax::dialect::Dialect;\n\
             use paredit_core_syntax::sexpr::{{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree}};\n\
             use paredit_core_syntax::view_query::for_each_subview;\n\n\
             #[derive(Debug, Clone)]\n\
             pub struct {pascal}Item {{\n\
             \x20   pub path: PathBuf,\n\
             \x20   pub span: ByteSpan,\n\
             }}\n\n\
             #[derive(Debug)]\n\
             pub struct {pascal}Summary {{\n\
             \x20   pub scanned_form_count: usize,\n\
             \x20   pub violations: Vec<{pascal}Item>,\n\
             }}\n\n\
             #[derive(Debug, Clone, Copy)]\n\
             pub struct {pascal}PolicyOptions {{\n\
             \x20   fail_on_violation: bool,\n\
             }}\n\n\
             impl {pascal}PolicyOptions {{\n\
             \x20   #[must_use]\n\
             \x20   pub const fn new(fail_on_violation: bool) -> Self {{\n\
             \x20       Self {{ fail_on_violation }}\n\
             \x20   }}\n\n\
             \x20   #[must_use]\n\
             \x20   pub const fn fail_on_violation(self) -> bool {{\n\
             \x20       self.fail_on_violation\n\
             \x20   }}\n\
             }}\n\n\
             #[derive(Debug)]\n\
             pub struct {pascal}Policy {{\n\
             \x20   pub fail_on_violation: bool,\n\
             \x20   pub scanned_form_count: usize,\n\
             \x20   pub violation_count: usize,\n\
             \x20   pub passed: bool,\n\
             \x20   pub violations: Vec<String>,\n\
             }}\n\n\
             /// Examines one node. Shared with the lint suite's rule, which reaches every\n\
             /// node through the single dispatch pass instead of walking the tree again.\n\
             ///\n\
             /// TODO: replace this stub with the real detection for {description}.\n\
             {seed_notes}pub fn examine(\n\
             \x20   _view: &ExpressionView,\n\
             \x20   _path: &Path,\n\
             \x20   _scanned_form_count: &mut usize,\n\
             \x20   _violations: &mut Vec<{pascal}Item>,\n\
             ) {{\n\
             }}\n\n\
             /// Collects every violation across a whole file, along with the total number of\n\
             /// forms scanned.\n\
             pub fn collect_{snake}(\n\
             \x20   path: &Path,\n\
             \x20   _dialect: Dialect,\n\
             \x20   tree: &SyntaxTree,\n\
             ) -> LintResult<(usize, Vec<{pascal}Item>)> {{\n\
             \x20   let mut scanned_form_count = 0;\n\
             \x20   let mut violations = Vec::new();\n\
             \x20   for index in 0..tree.root_children().len() {{\n\
             \x20       let view = tree.select_path(&SexprPath::root_child(index))?.view();\n\
             \x20       for_each_subview(&view, |subview| {{\n\
             \x20           examine(subview, path, &mut scanned_form_count, &mut violations);\n\
             \x20       }});\n\
             \x20   }}\n\
             \x20   Ok((scanned_form_count, violations))\n\
             }}\n\n\
             #[must_use]\n\
             pub const fn summarize_{snake}(\n\
             \x20   scanned_form_count: usize,\n\
             \x20   violations: Vec<{pascal}Item>,\n\
             ) -> {pascal}Summary {{\n\
             \x20   {pascal}Summary {{\n\
             \x20       scanned_form_count,\n\
             \x20       violations,\n\
             \x20   }}\n\
             }}\n\n\
             #[must_use]\n\
             pub fn evaluate_{snake}_policy(\n\
             \x20   options: {pascal}PolicyOptions,\n\
             \x20   summary: &{pascal}Summary,\n\
             ) -> {pascal}Policy {{\n\
             \x20   let violation_count = summary.violations.len();\n\
             \x20   let mut violations = Vec::new();\n\
             \x20   if options.fail_on_violation() && violation_count > 0 {{\n\
             \x20       violations.push(format!(\"violation_count {{violation_count}} exceeds 0\"));\n\
             \x20   }}\n\n\
             \x20   {pascal}Policy {{\n\
             \x20       fail_on_violation: options.fail_on_violation(),\n\
             \x20       scanned_form_count: summary.scanned_form_count,\n\
             \x20       violation_count,\n\
             \x20       passed: violations.is_empty(),\n\
             \x20       violations,\n\
             \x20   }}\n\
             }}\n\n\
             #[cfg(test)]\n\
             mod tests {{\n\
             \x20   use super::*;\n\n\
             \x20   #[test]\n\
             \x20   fn todo_replace_with_real_fixtures_once_examine_is_implemented() {{\n\
             \x20       let tree = SyntaxTree::parse_with_dialect(\"()\", Dialect::CommonLisp).expect(\"parse\");\n\
             \x20       let (count, violations) =\n\
             \x20           collect_{snake}(&PathBuf::from(\"test.lisp\"), Dialect::CommonLisp, &tree)\n\
             \x20               .expect(\"collect {snake}\");\n\
             \x20       assert_eq!(count, 0);\n\
             \x20       assert!(violations.is_empty(), \"TODO: this stub never finds anything yet\");\n\
             \x20   }}\n\
             {seed_tests}}}\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("usecase.rs"),
        &format!(
            "//! {pascal} ({description}) detection.\n\n\
             pub use crate::{snake}::domain::{{\n\
             \x20   {pascal}Item, {pascal}Policy, {pascal}PolicyOptions, {pascal}Summary, collect_{snake},\n\
             \x20   evaluate_{snake}_policy, summarize_{snake},\n\
             }};\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("rule.rs"),
        &format!(
            "//! `{kebab}`: TODO — {description}.\n\
             //!\n\
             //! The analysis lives in [`crate::{snake}::domain`], which also backs the\n\
             //! standalone `inspect {kebab}` command; this module only registers it with\n\
             //! the lint suite and phrases its findings.\n\n\
             use paredit_core_lint_engine::LintResult;\n\n\
             use crate::{snake}::domain::examine;\n\
             use paredit_core_lint_engine::engine::{{RuleContext, RuleSink}};\n\
             use paredit_core_lint_engine::model::{{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity}};\n\
             use paredit_core_lint_engine::rule::LintRule;\n\
             use paredit_core_syntax::sexpr::ExpressionView;\n\n\
             pub const META: RuleMeta = RuleMeta::new(\n\
             \x20   \"{kebab}\",\n\
             \x20   RuleCategory::{category_ident},\n\
             \x20   Severity::{severity_ident},\n\
             \x20   \"{description}\",\n\
             \x20   Fixability::ReportOnly,\n\
             );\n\n\
             #[derive(Debug)]\n\
             pub struct Rule;\n\n\
             pub const RULE: Rule = Rule;\n\n\
             impl LintRule for Rule {{\n\
             \x20   fn head_filter(&self) -> HeadFilter {{\n\
             \x20       // TODO: narrow this to `HeadFilter::Heads(&[...])` once `examine` only\n\
             \x20       // cares about specific list heads — `AllNodes` costs a call per node.\n\
             \x20       HeadFilter::AllNodes\n\
             \x20   }}\n\n\
             \x20   fn check(\n\
             \x20       &self,\n\
             \x20       context: &RuleContext<'_>,\n\
             \x20       view: &ExpressionView,\n\
             \x20       sink: &mut RuleSink<'_, '_>,\n\
             \x20   ) -> LintResult<()> {{\n\
             \x20       let mut scanned_form_count = 0;\n\
             \x20       let mut items = Vec::new();\n\
             \x20       examine(view, context.path(), &mut scanned_form_count, &mut items);\n\
             \x20       for item in items {{\n\
             \x20           sink.report(item.span, \"TODO: describe the violation\".to_owned());\n\
             \x20       }}\n\
             \x20       Ok(())\n\
             \x20   }}\n\
             }}\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("cli").join("mod.rs"),
        "pub mod args;\nmod render;\npub mod workflow;\n",
    )?;

    write_new_file(
        &rule_dir.join("cli").join("args.rs"),
        &format!(
            "use std::path::PathBuf;\n\n\
             use clap::Args;\n\n\
             use paredit_core_cli::args::{{DialectArg, OutputFormat}};\n\n\
             #[derive(Debug, Args)]\n\
             pub struct {pascal}ReportArgs {{\n\
             \x20   /// Files or directories to scan.\n\
             \x20   #[arg(required = true)]\n\
             \x20   pub files: Vec<PathBuf>,\n\
             \x20   /// Override extension-based dialect detection for every file.\n\
             \x20   #[arg(long)]\n\
             \x20   pub dialect: Option<DialectArg>,\n\
             \x20   /// Exit with failure when any violation is found.\n\
             \x20   #[arg(long)]\n\
             \x20   pub fail_on_violation: bool,\n\
             \x20   /// Output format for agent consumption.\n\
             \x20   #[arg(long, value_enum, default_value_t = OutputFormat::Json)]\n\
             \x20   pub output: OutputFormat,\n\
             }}\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("cli").join("render.rs"),
        &format!(
            "use paredit_core_cli::CliResult;\n\
             use paredit_core_cli::safe_text;\n\
             use serde_json::json;\n\n\
             use crate::{snake}::usecase::{{{pascal}Policy, {pascal}Summary}};\n\
             use paredit_core_cli::args::OutputFormat;\n\n\
             pub fn print_{snake}_report(\n\
             \x20   summary: &{pascal}Summary,\n\
             \x20   policy: &{pascal}Policy,\n\
             \x20   output: OutputFormat,\n\
             ) -> CliResult<()> {{\n\
             \x20   match output {{\n\
             \x20       OutputFormat::Text => {{\n\
             \x20           println!(\"scanned_form_count\\t{{}}\", summary.scanned_form_count);\n\
             \x20           println!(\"violation_count\\t{{}}\", summary.violations.len());\n\
             \x20           if policy.fail_on_violation {{\n\
             \x20               println!(\"policy\\tfail_on_violation=true\\tpassed={{}}\", policy.passed);\n\
             \x20           }}\n\
             \x20           for item in &summary.violations {{\n\
             \x20               println!(\n\
             \x20                   \"violation\\t{{}}\\t{{}}\",\n\
             \x20                   safe_text!(item.path.display()),\n\
             \x20                   item.span.start().get(),\n\
             \x20               );\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       OutputFormat::Json => {{\n\
             \x20           println!(\n\
             \x20               \"{{}}\",\n\
             \x20               serde_json::to_string_pretty(&json!({{\n\
             \x20                   \"schema_version\": 1,\n\
             \x20                   \"scanned_form_count\": summary.scanned_form_count,\n\
             \x20                   \"violation_count\": summary.violations.len(),\n\
             \x20                   \"policy\": {{\n\
             \x20                       \"fail_on_violation\": policy.fail_on_violation,\n\
             \x20                       \"passed\": policy.passed,\n\
             \x20                       \"violations\": &policy.violations,\n\
             \x20                   }},\n\
             \x20                   \"violations\": summary.violations\n\
             \x20                       .iter()\n\
             \x20                       .map(|item| json!({{\n\
             \x20                           \"path\": item.path.display().to_string(),\n\
             \x20                           \"span\": {{\n\
             \x20                               \"start\": item.span.start().get(),\n\
             \x20                               \"end\": item.span.end().get(),\n\
             \x20                           }},\n\
             \x20                       }}))\n\
             \x20                       .collect::<Vec<_>>(),\n\
             \x20               }}))?\n\
             \x20           );\n\
             \x20       }}\n\
             \x20   }}\n\n\
             \x20   Ok(())\n\
             }}\n"
        ),
    )?;

    write_new_file(
        &rule_dir.join("cli").join("workflow.rs"),
        &format!(
            "use paredit_core_cli::CommandResult;\n\n\
             use crate::{snake}::cli::args::{pascal}ReportArgs;\n\
             use crate::{snake}::cli::render::print_{snake}_report;\n\
             use crate::{snake}::usecase::{{\n\
             \x20   {pascal}PolicyOptions, collect_{snake}, evaluate_{snake}_policy, summarize_{snake},\n\
             }};\n\
             use paredit_core_cli::shared::{{expand_input_files, read_input_dialect_and_tree}};\n\n\
             pub fn {snake}_report(args: {pascal}ReportArgs) -> CommandResult {{\n\
             \x20   let files = expand_input_files(&args.files, args.dialect)?;\n\n\
             \x20   let mut scanned_form_count = 0;\n\
             \x20   let mut violations = Vec::new();\n\n\
             \x20   for file in &files {{\n\
             \x20       let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;\n\
             \x20       let (file_form_count, file_violations) = collect_{snake}(file, dialect, &tree)?;\n\
             \x20       scanned_form_count += file_form_count;\n\
             \x20       violations.extend(file_violations);\n\
             \x20   }}\n\n\
             \x20   let summary = summarize_{snake}(scanned_form_count, violations);\n\
             \x20   let policy = evaluate_{snake}_policy({pascal}PolicyOptions::new(args.fail_on_violation), &summary);\n\
             \x20   let policy_passed = policy.passed;\n\
             \x20   let policy_message = policy.violations.join(\"; \");\n\n\
             \x20   print_{snake}_report(&summary, &policy, args.output)?;\n\n\
             \x20   if !policy_passed {{\n\
             \x20       return Err(paredit_core_cli::gate::gate_failure(format!(\n\
             \x20           \"{kebab}-report policy failed: {{policy_message}}\"\n\
             \x20       )));\n\
             \x20   }}\n\n\
             \x20   Ok(())\n\
             }}\n"
        ),
    )?;

    insert_sorted_mod_line(
        &package_dir.join("src/lib.rs"),
        &format!("pub mod {snake};"),
    )?;

    let (old_count, new_count) = register_in_registry(repo, &crate_name, &snake)?;
    bump_catalog_counts(repo, false, severity_is_warning)?;

    println!();
    println!(
        "Generated `{snake}` inside {}, and registered it: RULE_COUNT {old_count} -> {new_count} \
         in src/lint/registry/mod.rs, {} assertion bumped to match (this \
         rule's META is Severity::{severity_ident} / Fixability::ReportOnly — if you change \
         either, fix the matching assertion in src/lint/registry/catalog.rs by hand).",
        package_dir.display(),
        if severity_is_warning {
            "warning_count()"
        } else {
            "no warning_count() (Severity::Error doesn't have its own counter)"
        }
    );
    if let Some(seed) = &options.seed {
        println!(
            "Seeded from `{}`: category RuleCategory::{category_ident}, severity \
             Severity::{severity_ident}, {} `:matches`/{} `:no-match` deftest case(s) \
             turned into TODO-marked unit tests in `domain.rs`{}.",
            seed.spec,
            seed.tests
                .iter()
                .map(|test| test.matches.len())
                .sum::<usize>(),
            seed.tests
                .iter()
                .map(|test| test.no_match.len())
                .sum::<usize>(),
            if seed.tests.iter().any(|test| !test.fixes.is_empty()) {
                "; its `:fix` case(s) are listed as a comment only — this scaffold has no fix \
                 support yet"
            } else {
                ""
            }
        );
    }
    println!("Fill in `domain.rs`'s `examine()` — everything else is plumbing.");

    let args_type = format!("{pascal}ReportArgs");
    let workflow_fn = format!("{snake}_report");
    checklist::print_command_wiring(
        repo,
        &checklist::CommandWiring {
            namespace: Namespace::Inspect,
            leaf: kebab,
            feature_crate: &crate_name,
            module_path: &snake,
            args_type: &args_type,
            variant: &pascal,
            workflow_fn: &workflow_fn,
        },
    )?;

    println!(
        "Also update the rule-count prose (currently {old_count}) in docs/src/reference/api.md, \
         docs/src/reference/architecture.md, docs/src/reference/configuration.md, and the doc comment in \
         tests/cli/determinism_contract.rs — none of them are test-enforced, so nothing catches \
         a stale number there but a reviewer."
    );

    Ok(())
}

fn register_in_registry(repo: &Repo, crate_name: &str, snake: &str) -> Result<(u32, u32)> {
    let path = repo.path("src/lint/registry/mod.rs");
    let mut text = fs::read_to_string(&path).map_err(crate::error::XtaskError::io(format!(
        "read {}",
        path.display()
    )))?;

    let marker = "pub const RULE_COUNT: usize = ";
    let start = text.find(marker).ok_or_else(|| {
        crate::error::XtaskError::refused(format!("`{marker}` not found in {}", path.display()))
    })? + marker.len();
    let end = start
        + text[start..]
            .find(';')
            .ok_or_else(|| crate::error::XtaskError::refused("no `;` after RULE_COUNT value"))?;
    let old_count: u32 = text[start..end]
        .trim()
        .parse()
        .map_err(|_| crate::error::XtaskError::refused("RULE_COUNT value is not a number"))?;
    let new_count = old_count + 1;
    text.replace_range(start..end, &new_count.to_string());

    let insertion = format!(
        "    RuleEntry::new(\n        &{crate_name}::{snake}::rule::META,\n        \
         &{crate_name}::{snake}::rule::RULE,\n    ),\n"
    );
    let closing = text
        .rfind("\n];")
        .ok_or_else(|| crate::error::XtaskError::refused("closing `];` of REGISTRY not found"))?;
    text.insert_str(closing + 1, &insertion);

    fs::write(&path, &text).map_err(crate::error::XtaskError::io(format!(
        "write {}",
        path.display()
    )))?;
    println!("  updated {} (RULE_COUNT, REGISTRY entry)", path.display());
    Ok((old_count, new_count))
}

fn bump_catalog_counts(repo: &Repo, is_fixable: bool, is_warning: bool) -> Result<()> {
    let path = repo.path("src/lint/registry/catalog.rs");
    let mut text = fs::read_to_string(&path).map_err(crate::error::XtaskError::io(format!(
        "read {}",
        path.display()
    )))?;

    text = bump_assert(&text, "assert!(RULE_COUNT == ")?;
    if is_fixable {
        text = bump_assert(&text, "assert!(fixable_count() == ")?;
    }
    if is_warning {
        text = bump_assert(&text, "assert!(warning_count() == ")?;
    }

    fs::write(&path, text).map_err(crate::error::XtaskError::io(format!(
        "write {}",
        path.display()
    )))?;
    println!("  updated {} (pinned-count assertions)", path.display());
    Ok(())
}

fn bump_assert(text: &str, marker: &str) -> Result<String> {
    let start = text
        .find(marker)
        .ok_or_else(|| crate::error::XtaskError::refused(format!("`{marker}` not found")))?
        + marker.len();
    let rest = &text[start..];
    let end_offset = rest
        .find(')')
        .ok_or_else(|| crate::error::XtaskError::refused("no closing `)` after assertion value"))?;
    let old: i64 = rest[..end_offset].trim().parse().map_err(|_| {
        crate::error::XtaskError::refused(format!("`{marker}` value is not a number"))
    })?;
    let mut result = text.to_owned();
    result.replace_range(start..start + end_offset, &(old + 1).to_string());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A throwaway directory shaped like enough of the real repository root
    /// for `run` to work against, so a hardcoded path drifting out of sync
    /// with the real tree (as `src/domain/lint/registry/*` once did) fails
    /// this test with an I/O error instead of only surfacing when a
    /// contributor runs `cargo xtask new-lint-rule` by hand.
    struct FixtureRepo {
        root: std::path::PathBuf,
    }

    impl Drop for FixtureRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn build_fixture_repo() -> FixtureRepo {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "xtask-new-lint-rule-test-{}-{unique}",
            std::process::id()
        ));

        let package_dir = root.join("packages/feature/lint-demo");
        fs::create_dir_all(package_dir.join("src")).expect("create package dir");
        fs::write(
            package_dir.join("Cargo.toml"),
            "[package]\nname = \"paredit_feature_lint_demo\"\n",
        )
        .expect("write package Cargo.toml");
        fs::write(package_dir.join("src/lib.rs"), "pub mod placeholder;\n")
            .expect("write package lib.rs");

        fs::create_dir_all(root.join("src/lint/registry")).expect("create registry dir");
        fs::write(
            root.join("src/lint/registry/mod.rs"),
            "pub const RULE_COUNT: usize = 1;\n\n\
             pub const REGISTRY: &[RuleEntry] = &[\n\
             \x20   RuleEntry::new(&existing::rule::META, &existing::rule::RULE),\n\
             ];\n",
        )
        .expect("write registry mod.rs");
        fs::write(
            root.join("src/lint/registry/catalog.rs"),
            "const _: () = assert!(RULE_COUNT == 1);\n\
             const _: () = assert!(fixable_count() == 0);\n\
             const _: () = assert!(warning_count() == 1);\n",
        )
        .expect("write registry catalog.rs");

        fs::create_dir_all(root.join("src/presentation/cli")).expect("create presentation dir");
        fs::write(root.join("src/presentation/cli/contract.rs"), "")
            .expect("write contract.rs stub");
        fs::create_dir_all(root.join("tests/cli")).expect("create tests/cli dir");
        fs::write(root.join("tests/cli/dialect_contract.rs"), "")
            .expect("write dialect_contract.rs stub");

        FixtureRepo { root }
    }

    #[test]
    fn scaffolds_a_rule_and_bumps_the_registry_against_the_real_path_shape() {
        let fixture = build_fixture_repo();
        let repo = Repo::for_test(fixture.root.clone());
        let options = NewLintRuleOptions {
            theme: "demo".to_owned(),
            name: Name::parse("char-case-fold").expect("valid rule name"),
            description: "flags a suspicious case fold".to_owned(),
            seed: None,
        };

        run(&repo, &options).expect("scaffolding must succeed against a well-formed fixture repo");

        let rule_dir = fixture
            .root
            .join("packages/feature/lint-demo/src/char_case_fold");
        assert!(rule_dir.join("mod.rs").is_file());
        assert!(rule_dir.join("domain.rs").is_file());
        assert!(rule_dir.join("usecase.rs").is_file());
        assert!(rule_dir.join("rule.rs").is_file());
        assert!(rule_dir.join("cli").join("args.rs").is_file());

        let registry = fs::read_to_string(fixture.root.join("src/lint/registry/mod.rs"))
            .expect("read bumped registry mod.rs");
        assert!(registry.contains("pub const RULE_COUNT: usize = 2;"));
        assert!(registry.contains("paredit_feature_lint_demo::char_case_fold::rule::META"));

        let catalog = fs::read_to_string(fixture.root.join("src/lint/registry/catalog.rs"))
            .expect("read bumped registry catalog.rs");
        assert!(catalog.contains("assert!(RULE_COUNT == 2)"));
        assert!(catalog.contains("assert!(warning_count() == 2)"));
        assert!(catalog.contains("assert!(fixable_count() == 0)"));
    }

    /// A real `.paredit/rules/*.lisp` fixture — the same shape as the example
    /// in `packages/feature/lint-custom/src/ruleset.rs`'s own module doc —
    /// written to a temp file so `CustomRuleSeed::load` reads it exactly the
    /// way a project's own custom rule file would be read.
    fn write_custom_rule_fixture() -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "xtask-new-lint-rule-seed-fixture-{}-{unique}.lisp",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"(defrule entity-needs-table
  :category malformed
  :severity error
  :description "a defentity with no :table option"
  :pattern (defentity ?name ...)
  :message "defentity needs a :table"
  :fix (defentity ?name :table "TODO"))

(deftest entity-needs-table
  (:matches "(defentity user)")
  (:no-match "(defentity user :table \"users\")")
  (:fix "(defentity user)" "(defentity user :table \"TODO\")"))
"#,
        )
        .expect("write custom rule fixture");
        path
    }

    #[test]
    fn seeds_category_severity_pattern_and_deftest_cases_from_a_real_custom_rule_file() {
        let fixture_path = write_custom_rule_fixture();
        let spec = format!("{}#entity-needs-table", fixture_path.display());
        let seed = CustomRuleSeed::load(&spec).expect("a well-formed seed spec must load");
        assert_eq!(seed.tests.len(), 1);
        let _ = fs::remove_file(&fixture_path);

        let fixture = build_fixture_repo();
        let repo = Repo::for_test(fixture.root.clone());
        let options = NewLintRuleOptions {
            theme: "demo".to_owned(),
            name: Name::parse("entity-needs-table").expect("valid rule name"),
            description: seed.rule.description.clone(),
            seed: Some(seed),
        };

        run(&repo, &options).expect("scaffolding from a seed must succeed against a fixture repo");

        let rule_dir = fixture
            .root
            .join("packages/feature/lint-demo/src/entity_needs_table");

        let rule_rs = fs::read_to_string(rule_dir.join("rule.rs")).expect("read rule.rs");
        assert!(rule_rs.contains("RuleCategory::Malformed"));
        assert!(rule_rs.contains("Severity::Error"));
        assert!(rule_rs.contains("a defentity with no :table option"));

        let domain_rs = fs::read_to_string(rule_dir.join("domain.rs")).expect("read domain.rs");
        assert!(domain_rs.contains("Seeded from `"));
        assert!(domain_rs.contains("pattern: `(defentity ?name ...)`"));
        assert!(domain_rs.contains("defentity needs a :table"));
        assert!(domain_rs.contains("fix: `(defentity ?name :table \"TODO\")`"));
        assert!(domain_rs.contains("fn todo_seed_matches_0()"));
        assert!(domain_rs.contains(r#"let source = "(defentity user)";"#));
        assert!(domain_rs.contains("fn todo_seed_no_match_0()"));
        assert!(domain_rs.contains(r#"let source = "(defentity user :table \"users\")";"#));
        assert!(domain_rs.contains(&format!(
            "{:?} -> {:?}",
            "(defentity user)", "(defentity user :table \"TODO\")"
        )));

        // `Severity::Error` must not bump `warning_count()` — only RULE_COUNT.
        let catalog = fs::read_to_string(fixture.root.join("src/lint/registry/catalog.rs"))
            .expect("read bumped registry catalog.rs");
        assert!(catalog.contains("assert!(RULE_COUNT == 2)"));
        assert!(catalog.contains("assert!(warning_count() == 1)"));
    }

    #[test]
    fn a_seed_spec_without_a_hash_is_a_clear_refusal() {
        let error =
            CustomRuleSeed::load("rules.lisp").expect_err("a spec with no `#name` must be refused");
        assert!(error.to_string().contains("<path>#<name>"));
    }

    #[test]
    fn a_seed_spec_naming_no_rule_is_a_clear_refusal() {
        let fixture_path = write_custom_rule_fixture();
        let error = CustomRuleSeed::load(&format!("{}#does-not-exist", fixture_path.display()))
            .expect_err("a name absent from the file must be refused");
        let _ = fs::remove_file(&fixture_path);
        assert!(error.to_string().contains("does-not-exist"));
    }

    #[test]
    fn a_seed_file_that_does_not_parse_is_a_clear_refusal() {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "xtask-new-lint-rule-seed-unreadable-{}-{unique}.lisp",
            std::process::id()
        ));
        fs::write(&path, "(defrule broken :pattern (f)").expect("write unreadable fixture");

        let error = CustomRuleSeed::load(&format!("{}#broken", path.display()))
            .expect_err("unbalanced parens must be refused, not panic");
        let _ = fs::remove_file(&path);
        assert!(error.to_string().contains("does not parse"));
    }

    #[test]
    fn a_seed_path_that_does_not_exist_is_a_clear_refusal() {
        let error = CustomRuleSeed::load("/no/such/directory/rules.lisp#some-rule")
            .expect_err("a missing file must be refused, not panic");
        assert!(!error.to_string().is_empty());
    }
}
