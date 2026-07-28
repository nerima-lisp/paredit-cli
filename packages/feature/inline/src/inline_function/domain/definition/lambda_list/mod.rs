use crate::error::{InlineError, InlineResult, UnsupportedLambdaList};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionView};

use super::super::syntax::atom_text;
use super::types::{
    InlineDefinitionKind, InlineParameter, InlineParameterBinding, InlineParameterKind,
};

pub(in super::super) mod parameters;
use parameters::{
    aux_parameter, dotted_tail_parameter_name, environment_parameter_name,
    is_dotted_list_separator, keyword_parameter, parse_required_parameter, rest_parameter_name,
    whole_parameter_name,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineLambdaListSection {
    Required,
    Optional,
    RestOrBody { consumed: bool },
    Keyword { allow_other_keys: bool },
    Aux,
}

impl InlineLambdaListSection {
    const fn label(self) -> &'static str {
        match self {
            Self::Required => "required parameters",
            Self::Optional => "&optional",
            Self::RestOrBody { .. } => "&rest or &body",
            Self::Keyword { .. } => "&key",
            Self::Aux => "&aux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingMacroParameter {
    Whole,
    Environment,
}

struct InlineLambdaListParseState {
    params: Vec<InlineParameter>,
    section: InlineLambdaListSection,
    pending_macro_parameter: Option<PendingMacroParameter>,
    accepts_other_keys: bool,
    has_rest_or_body: bool,
    has_whole: bool,
    has_environment: bool,
    supports_common_lisp_lambda_list: bool,
}

impl InlineLambdaListParseState {
    fn new(dialect: Dialect, capacity: usize) -> Self {
        Self {
            params: Vec::with_capacity(capacity),
            section: InlineLambdaListSection::Required,
            pending_macro_parameter: None,
            accepts_other_keys: false,
            has_rest_or_body: false,
            has_whole: false,
            has_environment: false,
            supports_common_lisp_lambda_list: dialect
                .supports_common_lisp_lambda_list_refactor_model(),
        }
    }

    fn parse_child(
        &mut self,
        input: &str,
        definition_kind: InlineDefinitionKind,
        child: &ExpressionView,
        index: usize,
        children: &[ExpressionView],
    ) -> InlineResult<bool> {
        if is_dotted_list_separator(child) {
            self.push_dotted_tail(children, index)?;
            return Ok(true);
        }

        if let Some(marker) = atom_text(child).filter(|name| name.starts_with('&')) {
            self.handle_marker(marker, definition_kind)?;
            return Ok(false);
        }

        let parameter = self.parse_parameter(input, definition_kind, child)?;
        self.params.push(parameter);
        Ok(false)
    }

    fn push_dotted_tail(&mut self, children: &[ExpressionView], index: usize) -> InlineResult<()> {
        if self.has_rest_or_body {
            return Err(UnsupportedLambdaList::AtMostOne {
                construct: "&rest or &body parameter".to_owned(),
            }
            .into());
        }
        if matches!(
            self.section,
            InlineLambdaListSection::Keyword { .. } | InlineLambdaListSection::Aux
        ) {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: "dotted lambda lists".to_owned(),
                after: self.section.label().to_string(),
            }
            .into());
        }
        if index == 0 {
            return Err(UnsupportedLambdaList::Requirement {
                subject: "dotted lambda lists".to_owned(),
                requirement: "begin with a binding name".to_owned(),
            }
            .into());
        }

        let tail = children.get(index + 1).ok_or_else(|| {
            InlineError::from(UnsupportedLambdaList::MustBeFollowedBy {
                marker: "dotted lambda lists".to_owned(),
                expected: "a binding name".to_owned(),
            })
        })?;
        if index + 2 != children.len() {
            return Err(UnsupportedLambdaList::Requirement {
                subject: "dotted lambda lists".to_owned(),
                requirement: "end after the tail binding".to_owned(),
            }
            .into());
        }

        self.params.push(InlineParameter {
            binding: InlineParameterBinding::Name(dotted_tail_parameter_name(tail)?.to_owned()),
            kind: InlineParameterKind::Rest,
            default_value: None,
            supplied_p: None,
        });
        self.has_rest_or_body = true;
        self.section = InlineLambdaListSection::RestOrBody { consumed: true };
        Ok(())
    }

    fn handle_marker(
        &mut self,
        marker: &str,
        definition_kind: InlineDefinitionKind,
    ) -> InlineResult<()> {
        if !self.supports_common_lisp_lambda_list {
            return Err(UnsupportedLambdaList::ModifierNotSupported {
                marker: marker.to_string(),
            }
            .into());
        }

        match self.pending_macro_parameter {
            Some(PendingMacroParameter::Whole) => {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "&whole".to_owned(),
                    expected: "a binding name".to_owned(),
                }
                .into());
            }
            Some(PendingMacroParameter::Environment) => {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "&environment".to_owned(),
                    expected: "a binding name".to_owned(),
                }
                .into());
            }
            None => {}
        }

        if matches!(
            self.section,
            InlineLambdaListSection::RestOrBody { consumed: false }
        ) {
            return Err(UnsupportedLambdaList::MustBeFollowedBy {
                marker: "&rest or &body".to_owned(),
                expected: "a binding name".to_owned(),
            }
            .into());
        }

        match marker {
            "&optional" => self.enter_optional_section(),
            "&key" => self.enter_keyword_section(),
            "&rest" | "&body" => self.enter_rest_or_body_section(marker),
            "&aux" => self.enter_aux_section(),
            "&whole" if definition_kind == InlineDefinitionKind::Macro => {
                self.begin_pending_macro_parameter(PendingMacroParameter::Whole)
            }
            "&environment" if definition_kind == InlineDefinitionKind::Macro => {
                self.begin_pending_macro_parameter(PendingMacroParameter::Environment)
            }
            "&allow-other-keys"
                if matches!(self.section, InlineLambdaListSection::Keyword { .. }) =>
            {
                self.accepts_other_keys = true;
                self.section = InlineLambdaListSection::Keyword {
                    allow_other_keys: true,
                };
                Ok(())
            }
            _ => Err(UnsupportedLambdaList::SupportsOnly {
                supported: format!("required, &optional, &rest, &body, &whole, &environment, &aux, and simple &key parameters; found {marker}"),
            }
            .into()),
        }
    }

    fn enter_optional_section(&mut self) -> InlineResult<()> {
        if !matches!(self.section, InlineLambdaListSection::Required) {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: "&optional parameters".to_owned(),
                after: self.section.label().to_string(),
            }
            .into());
        }
        self.section = InlineLambdaListSection::Optional;
        Ok(())
    }

    fn enter_keyword_section(&mut self) -> InlineResult<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required
                | InlineLambdaListSection::Optional
                | InlineLambdaListSection::RestOrBody { consumed: true }
        ) {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: "&key parameters".to_owned(),
                after: self.section.label().to_string(),
            }
            .into());
        }
        self.section = InlineLambdaListSection::Keyword {
            allow_other_keys: false,
        };
        Ok(())
    }

    fn enter_rest_or_body_section(&mut self, marker: &str) -> InlineResult<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required | InlineLambdaListSection::Optional
        ) {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: format!("{marker} parameters"),
                after: self.section.label().to_string(),
            }
            .into());
        }
        if self.has_rest_or_body {
            return Err(UnsupportedLambdaList::AtMostOne {
                construct: "&rest or &body parameter".to_owned(),
            }
            .into());
        }

        self.has_rest_or_body = true;
        self.section = InlineLambdaListSection::RestOrBody { consumed: false };
        Ok(())
    }

    fn enter_aux_section(&mut self) -> InlineResult<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required
                | InlineLambdaListSection::Optional
                | InlineLambdaListSection::RestOrBody { consumed: true }
                | InlineLambdaListSection::Keyword { .. }
        ) {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: "&aux parameters".to_owned(),
                after: self.section.label().to_string(),
            }
            .into());
        }
        self.section = InlineLambdaListSection::Aux;
        Ok(())
    }

    fn begin_pending_macro_parameter(
        &mut self,
        pending: PendingMacroParameter,
    ) -> InlineResult<()> {
        match pending {
            PendingMacroParameter::Whole => {
                if self.has_whole {
                    return Err(UnsupportedLambdaList::AtMostOne {
                        construct: "&whole parameter".to_owned(),
                    }
                    .into());
                }
                if self
                    .params
                    .iter()
                    .any(|param| !matches!(param.kind, InlineParameterKind::Environment))
                {
                    return Err(UnsupportedLambdaList::SupportsOnlyWhen {
                        construct: "&whole".to_owned(),
                        restriction: "before ordinary macro parameters".to_owned(),
                    }
                    .into());
                }
                self.has_whole = true;
            }
            PendingMacroParameter::Environment => {
                if self.has_environment {
                    return Err(UnsupportedLambdaList::AtMostOne {
                        construct: "&environment parameter".to_owned(),
                    }
                    .into());
                }
                self.has_environment = true;
            }
        }

        self.pending_macro_parameter = Some(pending);
        self.section = InlineLambdaListSection::Required;
        Ok(())
    }

    fn parse_parameter(
        &mut self,
        input: &str,
        definition_kind: InlineDefinitionKind,
        child: &ExpressionView,
    ) -> InlineResult<InlineParameter> {
        match self.pending_macro_parameter.take() {
            Some(PendingMacroParameter::Whole) => Ok(InlineParameter {
                binding: InlineParameterBinding::Name(whole_parameter_name(child)?.to_owned()),
                kind: InlineParameterKind::Whole,
                default_value: None,
                supplied_p: None,
            }),
            Some(PendingMacroParameter::Environment) => Ok(InlineParameter {
                binding: InlineParameterBinding::Name(
                    environment_parameter_name(child)?.to_owned(),
                ),
                kind: InlineParameterKind::Environment,
                default_value: None,
                supplied_p: None,
            }),
            None => self.parse_section_parameter(input, definition_kind, child),
        }
    }

    fn parse_section_parameter(
        &mut self,
        input: &str,
        definition_kind: InlineDefinitionKind,
        child: &ExpressionView,
    ) -> InlineResult<InlineParameter> {
        match self.section {
            InlineLambdaListSection::Required => {
                parse_required_parameter(input, definition_kind, child)
            }
            InlineLambdaListSection::Optional => {
                parameters::optional_parameter(input, definition_kind, child)
            }
            InlineLambdaListSection::RestOrBody { consumed: false } => {
                self.has_rest_or_body = true;
                self.section = InlineLambdaListSection::RestOrBody { consumed: true };
                Ok(InlineParameter {
                    binding: InlineParameterBinding::Name(rest_parameter_name(child)?.to_owned()),
                    kind: InlineParameterKind::Rest,
                    default_value: None,
                    supplied_p: None,
                })
            }
            InlineLambdaListSection::RestOrBody { consumed: true } => {
                Err(UnsupportedLambdaList::NotSupportedAfter {
                    construct: "ordinary parameters".to_owned(),
                    after: "&rest or &body".to_owned(),
                }
                .into())
            }
            InlineLambdaListSection::Keyword { .. } => {
                keyword_parameter(input, definition_kind, child)
            }
            InlineLambdaListSection::Aux => aux_parameter(input, child),
        }
    }

    fn finish(self) -> InlineResult<(Vec<InlineParameter>, bool)> {
        match self.pending_macro_parameter {
            Some(PendingMacroParameter::Whole) => {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "&whole".to_owned(),
                    expected: "a binding name".to_owned(),
                }
                .into());
            }
            Some(PendingMacroParameter::Environment) => {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "&environment".to_owned(),
                    expected: "a binding name".to_owned(),
                }
                .into());
            }
            None => {}
        }

        if matches!(
            self.section,
            InlineLambdaListSection::RestOrBody { consumed: false }
        ) {
            return Err(UnsupportedLambdaList::MustBeFollowedBy {
                marker: "&rest or &body".to_owned(),
                expected: "a binding name".to_owned(),
            }
            .into());
        }

        Ok((self.params, self.accepts_other_keys))
    }
}

pub fn inline_parameter_names(
    dialect: Dialect,
    input: &str,
    definition_kind: InlineDefinitionKind,
    parameter_form: &ExpressionView,
) -> InlineResult<(Vec<InlineParameter>, bool)> {
    match parameter_form.delimiter {
        Some(Delimiter::Paren | Delimiter::Bracket) => inline_parameter_names_from_children(
            dialect,
            input,
            definition_kind,
            &parameter_form.children,
        ),
        _ => Err(UnsupportedLambdaList::SupportsOnly {
            supported: "flat symbol parameter lists".to_owned(),
        }
        .into()),
    }
}

pub fn inline_parameter_names_from_children(
    dialect: Dialect,
    input: &str,
    definition_kind: InlineDefinitionKind,
    children: &[ExpressionView],
) -> InlineResult<(Vec<InlineParameter>, bool)> {
    let mut state = InlineLambdaListParseState::new(dialect, children.len());

    for (index, child) in children.iter().enumerate() {
        if state.parse_child(input, definition_kind, child, index, children)? {
            break;
        }
    }

    state.finish()
}
