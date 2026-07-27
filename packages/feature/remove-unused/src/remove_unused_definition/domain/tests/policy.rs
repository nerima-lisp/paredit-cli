use super::*;

#[test]
fn skips_protected_definition_categories_by_default() {
    let text = "(in-package #:app)\n(deftest stale-test () (is t))\n";
    let form = "(deftest stale-test () (is t))";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "stale-test",
            DefinitionCategory::Test,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 1);
    assert_eq!(
        plan.files[0].skipped[0].reason,
        SkippedDefinitionRemovalReason::ProtectedDefinitionCategory
    );
}

#[test]
fn skips_unrecognized_define_style_macro_invocations_by_default() {
    // A custom `define-*` macro (a strategy DSL, a DB-schema DSL, ...) is
    // free to derive other exported symbol names from its argument via
    // string concatenation, so the argument itself having no direct
    // references does not mean the definition is safe to delete.
    let text = "(in-package #:app)\n\
                (define-trading-strategy d1-momentum :parameters 42)\n";
    let form = "(define-trading-strategy d1-momentum :parameters 42)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "d1-momentum",
            DefinitionCategory::UnknownMacro,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 1);
    assert_eq!(
        plan.files[0].skipped[0].reason,
        SkippedDefinitionRemovalReason::ProtectedDefinitionCategory
    );
    assert!(plan.files[0].rewritten.contains(form));
}

#[test]
fn skips_defstruct_definitions_by_default() {
    // A `defstruct` implicitly derives a constructor (`make-<name>` or an
    // explicit `(:constructor other-name)`), a predicate (`<name>-p`), and
    // per-slot accessors from the structure name, none of which contain the
    // structure-name symbol itself. The type name having zero direct
    // references does not mean the structure is unused: it may still be
    // constructed and inspected purely through those derived symbols.
    let text = "(in-package #:app)\n\
                (defstruct widget a b)\n";
    let form = "(defstruct widget a b)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(text, form, "widget", DefinitionCategory::Struct)],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 1);
    assert_eq!(
        plan.files[0].skipped[0].reason,
        SkippedDefinitionRemovalReason::ProtectedDefinitionCategory
    );
    assert!(plan.files[0].rewritten.contains(form));
}

#[test]
fn other_category_definitions_remain_bulk_removable() {
    // `Other` covers a dialect's own recognized definition forms (Emacs
    // Lisp `defun`, Clojure `defn`, ...) that are not broken out into a
    // more specific category. Unlike `UnknownMacro`, these are known,
    // non-generative shapes, so they should stay bulk-removable by default.
    let text = "(defun stale-helper () 1)\n(defun live () 2)\n(live)\n";
    let stale_form = "(defun stale-helper () 1)";
    let live_form = "(defun live () 2)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![
            definition(text, stale_form, "stale-helper", DefinitionCategory::Other),
            definition(text, live_form, "live", DefinitionCategory::Function),
        ],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(stale_form));
}

#[test]
fn skips_exported_definition_by_default() {
    let text = "(in-package #:app)\n(defun public-entry () 1)\n";
    let form = "(defun public-entry () 1)";
    let mut request = request_for(
        text,
        vec![definition(
            text,
            form,
            "public-entry",
            DefinitionCategory::Function,
        )],
    );
    request.package_definitions.push(PackageDefinitionReport {
        path: "0".to_owned(),
        span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0)),
        name: "#:app".to_owned(),
        nicknames: Vec::new(),
        uses: Vec::new(),
        exports: vec!["#:public-entry".to_owned()],
        imports: Vec::new(),
        option_count: 1,
    });

    let plan = plan_remove_unused_definitions(request).expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 1);
    assert_eq!(
        plan.files[0].skipped[0].reason,
        SkippedDefinitionRemovalReason::ExportedDefinition
    );
}

#[test]
fn skips_exported_definition_when_file_uses_package_nickname() {
    let text = "(in-package #:main)\n(defun public-entry () 1)\n";
    let form = "(defun public-entry () 1)";
    let mut public_entry = definition(text, form, "public-entry", DefinitionCategory::Function);
    public_entry.package = Some("main".to_owned());
    let mut request = request_for(text, vec![public_entry]);
    request.files[0].package = Some("main".to_owned());
    request.package_definitions.push(PackageDefinitionReport {
        path: "0".to_owned(),
        span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0)),
        name: "#:app.main".to_owned(),
        nicknames: vec!["#:main".to_owned()],
        uses: Vec::new(),
        exports: vec!["#:public-entry".to_owned()],
        imports: Vec::new(),
        option_count: 2,
    });

    let plan = plan_remove_unused_definitions(request).expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 1);
    assert_eq!(
        plan.files[0].skipped[0].reason,
        SkippedDefinitionRemovalReason::ExportedDefinition
    );
}

#[test]
fn common_lisp_package_exports_do_not_protect_non_common_lisp_definitions() {
    let text = "(define public-entry (lambda () 1))\n";
    let form = "(define public-entry (lambda () 1))";
    let mut public_entry = definition(text, form, "public-entry", DefinitionCategory::Function);
    public_entry.head = "define".to_owned();
    let request = RemoveUnusedDefinitionsRequest {
        files: vec![file_with_dialect(
            PathBuf::from("public.scm"),
            Dialect::Scheme,
            Some("app"),
            text,
            vec![public_entry],
        )],
        package_definitions: vec![PackageDefinitionReport {
            path: "0".to_owned(),
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0)),
            name: "#:app".to_owned(),
            nicknames: Vec::new(),
            uses: Vec::new(),
            exports: vec!["#:public-entry".to_owned()],
            imports: Vec::new(),
            option_count: 1,
        }],
        include_protected: false,
        include_exported: false,
    };

    let plan = plan_remove_unused_definitions(request).expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
}
