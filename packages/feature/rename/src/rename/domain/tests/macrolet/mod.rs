use super::*;

macro_rules! assert_macrolet_rename {
    (
        input: $input:expr,
        dialect: $dialect:expr,
        from: $from:expr,
        to: $to:expr,
        definitions: $definitions:expr,
        calls: $calls:expr,
        changed: $changed:expr,
        rewritten_contains: [$($fragment:expr),+ $(,)?]
    ) => {{
        let plan = plan_rename_macrolet(RenameMacroletRequest {
            input: $input,
            dialect: $dialect,
            from: SymbolName::new($from).unwrap(),
            to: SymbolName::new($to).unwrap(),
        })
        .unwrap();

        assert_eq!(plan.definitions.len(), $definitions);
        assert_eq!(plan.calls.len(), $calls);
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

mod basic_forms;
mod property;
mod quoting;
mod scoping;
