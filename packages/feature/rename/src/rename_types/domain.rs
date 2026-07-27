use paredit_core_syntax::sexpr::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionCallScope {
    AllCalls,
    ExplicitPaths(Vec<Path>),
}

impl FunctionCallScope {
    #[must_use]
    pub const fn all() -> Self {
        Self::AllCalls
    }

    #[must_use]
    pub const fn explicit(paths: Vec<Path>) -> Self {
        Self::ExplicitPaths(paths)
    }

    #[must_use]
    pub const fn is_all_calls(&self) -> bool {
        matches!(self, Self::AllCalls)
    }

    #[must_use]
    pub fn explicit_paths(&self) -> Option<&[Path]> {
        match self {
            Self::AllCalls => None,
            Self::ExplicitPaths(paths) => Some(paths),
        }
    }
}

pub type ReplaceFunctionCallsScope = FunctionCallScope;
pub type UnwrapFunctionCallsScope = FunctionCallScope;
pub type WrapFunctionCallsScope = FunctionCallScope;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_scope_intent_without_operation_specific_types() {
        let scope = FunctionCallScope::explicit(vec![Path::root_child(1)]);

        assert!(!scope.is_all_calls());
        assert_eq!(scope.explicit_paths(), Some(&[Path::root_child(1)][..]));
        assert!(FunctionCallScope::all().is_all_calls());
    }
}
