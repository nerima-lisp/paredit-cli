use super::*;

macro_rules! assert_symbol_macro_rename {
    (
        input: $input:expr,
        from: $from:expr,
        to: $to:expr,
        definitions: $definitions:expr,
        references: $references:expr,
        changed: $changed:expr,
        rewritten_contains: [$($fragment:expr),+ $(,)?]
    ) => {{
        let plan = plan_rename_symbol_macro(RenameSymbolMacroRequest {
            input: $input,
            dialect: Dialect::CommonLisp,
            from: SymbolName::new($from).unwrap(),
            to: SymbolName::new($to).unwrap(),
        })
        .unwrap();

        assert_eq!(plan.definitions.len(), $definitions);
        assert_eq!(plan.references.len(), $references);
        assert_eq!(plan.changed, $changed);
        $(
            assert!(
                plan.rewritten.contains($fragment),
                "missing fragment `{}` in rewritten output: {}",
                $fragment,
                plan.rewritten
            );
        )+
    }};
}

mod definition_forms;
mod property;
mod scoping;
