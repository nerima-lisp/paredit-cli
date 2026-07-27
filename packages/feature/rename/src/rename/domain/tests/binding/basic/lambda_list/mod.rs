use super::*;

macro_rules! assert_binding_rename {
    (
        input: $input:expr,
        dialect: $dialect:expr,
        from: $from:expr,
        to: $to:expr,
        form: $form:expr,
        references: $references:expr,
        shadowed_scope_count: $shadowed_scope_count:expr,
        rewritten: $rewritten:expr,
    ) => {{
        let plan = plan_rename_binding(RenameBindingRequest {
            input: $input,
            dialect: $dialect,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new($from).unwrap(),
            to: SymbolName::new($to).unwrap(),
        })
        .unwrap();

        assert!(plan.changed);
        assert_eq!(plan.form, $form);
        assert_eq!(plan.references.len(), $references);
        assert_eq!(plan.shadowed_scope_count, $shadowed_scope_count);
        assert_eq!(plan.rewritten, $rewritten);
        SyntaxTree::parse(&plan.rewritten).unwrap();
    }};
    (
        input: $input:expr,
        dialect: $dialect:expr,
        from: $from:expr,
        to: $to:expr,
        form: $form:expr,
        references: $references:expr,
        rewritten: $rewritten:expr,
    ) => {{
        let plan = plan_rename_binding(RenameBindingRequest {
            input: $input,
            dialect: $dialect,
            target: RenameTarget::Path(Path::from_indexes(vec![0])),
            from: SymbolName::new($from).unwrap(),
            to: SymbolName::new($to).unwrap(),
        })
        .unwrap();

        assert!(plan.changed);
        assert_eq!(plan.form, $form);
        assert_eq!(plan.references.len(), $references);
        assert_eq!(plan.rewritten, $rewritten);
        SyntaxTree::parse(&plan.rewritten).unwrap();
    }};
}

mod binding_forms;
mod callables;
mod local_callables;
mod macro_forms;
