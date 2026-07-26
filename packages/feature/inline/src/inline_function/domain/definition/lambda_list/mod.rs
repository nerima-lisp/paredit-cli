use anyhow::{Context, Result};

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
    ) -> Result<bool> {
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

    fn push_dotted_tail(&mut self, children: &[ExpressionView], index: usize) -> Result<()> {
        if self.has_rest_or_body {
            anyhow::bail!("inline-function supports at most one &rest or &body parameter");
        }
        if matches!(
            self.section,
            InlineLambdaListSection::Keyword { .. } | InlineLambdaListSection::Aux
        ) {
            anyhow::bail!(
                "inline-function does not support dotted lambda lists after {}",
                self.section.label()
            );
        }
        if index == 0 {
            anyhow::bail!("inline-function dotted lambda lists must begin with a binding name");
        }

        let tail = children
            .get(index + 1)
            .context("inline-function dotted lambda lists must be followed by a binding name")?;
        if index + 2 != children.len() {
            anyhow::bail!("inline-function dotted lambda lists must end after the tail binding");
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

    fn handle_marker(&mut self, marker: &str, definition_kind: InlineDefinitionKind) -> Result<()> {
        if !self.supports_common_lisp_lambda_list {
            anyhow::bail!(
                "inline-function function parameter modifiers are not supported: {marker}"
            );
        }

        match self.pending_macro_parameter {
            Some(PendingMacroParameter::Whole) => {
                anyhow::bail!("inline-function &whole must be followed by a binding name");
            }
            Some(PendingMacroParameter::Environment) => {
                anyhow::bail!("inline-function &environment must be followed by a binding name");
            }
            None => {}
        }

        if matches!(
            self.section,
            InlineLambdaListSection::RestOrBody { consumed: false }
        ) {
            anyhow::bail!("inline-function &rest or &body must be followed by a binding name");
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
            _ => anyhow::bail!(
                "inline-function currently supports only required, &optional, &rest, &body, &whole, &environment, &aux, and simple &key parameters; found {marker}"
            ),
        }
    }

    fn enter_optional_section(&mut self) -> Result<()> {
        if !matches!(self.section, InlineLambdaListSection::Required) {
            anyhow::bail!(
                "inline-function does not support &optional parameters after {}",
                self.section.label()
            );
        }
        self.section = InlineLambdaListSection::Optional;
        Ok(())
    }

    fn enter_keyword_section(&mut self) -> Result<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required
                | InlineLambdaListSection::Optional
                | InlineLambdaListSection::RestOrBody { consumed: true }
        ) {
            anyhow::bail!(
                "inline-function does not support &key parameters after {}",
                self.section.label()
            );
        }
        self.section = InlineLambdaListSection::Keyword {
            allow_other_keys: false,
        };
        Ok(())
    }

    fn enter_rest_or_body_section(&mut self, marker: &str) -> Result<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required | InlineLambdaListSection::Optional
        ) {
            anyhow::bail!(
                "inline-function does not support {marker} parameters after {}",
                self.section.label()
            );
        }
        if self.has_rest_or_body {
            anyhow::bail!("inline-function supports at most one &rest or &body parameter");
        }

        self.has_rest_or_body = true;
        self.section = InlineLambdaListSection::RestOrBody { consumed: false };
        Ok(())
    }

    fn enter_aux_section(&mut self) -> Result<()> {
        if !matches!(
            self.section,
            InlineLambdaListSection::Required
                | InlineLambdaListSection::Optional
                | InlineLambdaListSection::RestOrBody { consumed: true }
                | InlineLambdaListSection::Keyword { .. }
        ) {
            anyhow::bail!(
                "inline-function does not support &aux parameters after {}",
                self.section.label()
            );
        }
        self.section = InlineLambdaListSection::Aux;
        Ok(())
    }

    fn begin_pending_macro_parameter(&mut self, pending: PendingMacroParameter) -> Result<()> {
        match pending {
            PendingMacroParameter::Whole => {
                if self.has_whole {
                    anyhow::bail!("inline-function supports at most one &whole parameter");
                }
                if self
                    .params
                    .iter()
                    .any(|param| !matches!(param.kind, InlineParameterKind::Environment))
                {
                    anyhow::bail!(
                        "inline-function currently supports &whole only before ordinary macro parameters"
                    );
                }
                self.has_whole = true;
            }
            PendingMacroParameter::Environment => {
                if self.has_environment {
                    anyhow::bail!("inline-function supports at most one &environment parameter");
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
    ) -> Result<InlineParameter> {
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
    ) -> Result<InlineParameter> {
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
            InlineLambdaListSection::RestOrBody { consumed: true } => anyhow::bail!(
                "inline-function does not support ordinary parameters after &rest or &body"
            ),
            InlineLambdaListSection::Keyword { .. } => {
                keyword_parameter(input, definition_kind, child)
            }
            InlineLambdaListSection::Aux => aux_parameter(input, child),
        }
    }

    fn finish(self) -> Result<(Vec<InlineParameter>, bool)> {
        match self.pending_macro_parameter {
            Some(PendingMacroParameter::Whole) => {
                anyhow::bail!("inline-function &whole must be followed by a binding name");
            }
            Some(PendingMacroParameter::Environment) => {
                anyhow::bail!("inline-function &environment must be followed by a binding name");
            }
            None => {}
        }

        if matches!(
            self.section,
            InlineLambdaListSection::RestOrBody { consumed: false }
        ) {
            anyhow::bail!("inline-function &rest or &body must be followed by a binding name");
        }

        Ok((self.params, self.accepts_other_keys))
    }
}

pub fn inline_parameter_names(
    dialect: Dialect,
    input: &str,
    definition_kind: InlineDefinitionKind,
    parameter_form: &ExpressionView,
) -> Result<(Vec<InlineParameter>, bool)> {
    match parameter_form.delimiter {
        Some(Delimiter::Paren | Delimiter::Bracket) => inline_parameter_names_from_children(
            dialect,
            input,
            definition_kind,
            &parameter_form.children,
        ),
        _ => anyhow::bail!("inline-function currently supports only flat symbol parameter lists"),
    }
}

pub fn inline_parameter_names_from_children(
    dialect: Dialect,
    input: &str,
    definition_kind: InlineDefinitionKind,
    children: &[ExpressionView],
) -> Result<(Vec<InlineParameter>, bool)> {
    let mut state = InlineLambdaListParseState::new(dialect, children.len());

    for (index, child) in children.iter().enumerate() {
        if state.parse_child(input, definition_kind, child, index, children)? {
            break;
        }
    }

    state.finish()
}
