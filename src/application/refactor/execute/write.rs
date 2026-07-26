use super::types::{RefactorWriteCandidate, RefactorWritePlan, RefactorWriteRefusal};

#[must_use]
pub fn build_refactor_write_plan(
    write_requested: bool,
    candidates: &[RefactorWriteCandidate],
) -> RefactorWritePlan {
    if !write_requested {
        return RefactorWritePlan::not_requested();
    }

    let parse_error_count = candidates
        .iter()
        .filter(|candidate| !candidate.output_parse_ok)
        .count();
    if parse_error_count > 0 {
        return RefactorWritePlan::refused(RefactorWriteRefusal::UnparsableOutputs {
            count: parse_error_count,
        });
    }

    let writable_indexes = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| candidate.changed.then_some(index))
        .collect();
    RefactorWritePlan::allowed(writable_indexes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_plan_is_not_requested_when_write_is_disabled() {
        let plan = build_refactor_write_plan(
            false,
            &[RefactorWriteCandidate {
                changed: true,
                output_parse_ok: false,
            }],
        );

        assert!(!plan.write_requested());
        assert!(!plan.write_allowed());
        assert!(plan.writable_indexes().is_empty());
        assert_eq!(plan.refusal(), None);
    }

    #[test]
    fn write_plan_is_refused_when_an_output_is_unparsable() {
        let plan = build_refactor_write_plan(
            true,
            &[
                RefactorWriteCandidate {
                    changed: true,
                    output_parse_ok: false,
                },
                RefactorWriteCandidate {
                    changed: true,
                    output_parse_ok: true,
                },
            ],
        );

        assert!(plan.write_requested());
        assert!(!plan.write_allowed());
        assert!(plan.writable_indexes().is_empty());
        assert_eq!(
            plan.refusal(),
            Some(&RefactorWriteRefusal::UnparsableOutputs { count: 1 })
        );
    }

    #[test]
    fn write_plan_is_allowed_with_only_changed_candidate_indexes() {
        let plan = build_refactor_write_plan(
            true,
            &[
                RefactorWriteCandidate {
                    changed: false,
                    output_parse_ok: true,
                },
                RefactorWriteCandidate {
                    changed: true,
                    output_parse_ok: true,
                },
            ],
        );

        assert!(plan.write_requested());
        assert!(plan.write_allowed());
        assert_eq!(plan.writable_indexes(), &[1]);
        assert_eq!(plan.refusal(), None);
    }
}
