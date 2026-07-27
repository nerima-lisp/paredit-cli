use super::*;

macro_rules! assert_function_rename {
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
        let plan = plan_rename_function(RenameFunctionRequest {
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

mod generic_forms;
mod macro_forms;
mod property;
mod scoping;
mod setf_scoping;
