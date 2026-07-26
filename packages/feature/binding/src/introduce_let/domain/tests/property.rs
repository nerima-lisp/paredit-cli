use super::*;
use proptest::prelude::*;

fn repeated_products(count: usize) -> String {
    let terms = (0..count)
        .map(|_| "(* width height)")
        .collect::<Vec<_>>()
        .join(" ");
    format!("(defun render () (+ {terms}))")
}

fn repeated_products_with_shadowed_duplicate(count: usize) -> String {
    let terms = (0..count)
        .map(|_| "(* width height)")
        .chain(std::iter::once("(let ((product 0)) (* width height))"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("(defun render () (+ {terms}))")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn all_occurrences_replaces_every_generated_duplicate(count in 1usize..10) {
        let input = repeated_products(count);
        let plan = plan_introduce_let(request(&input, "0.3.1", true)).expect("plan");

        prop_assert_eq!(plan.occurrence_spans.len(), count);
        prop_assert_eq!(plan.skipped_shadowed_occurrence_spans.len(), 0);
        prop_assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
        prop_assert_eq!(plan.rewritten.matches("(* width height)").count(), 1);
        prop_assert_eq!(plan.rewritten.matches("product").count(), count + 1);
    }

    #[test]
    fn all_occurrences_skips_generated_shadowed_duplicates(count in 1usize..10) {
        let input = repeated_products_with_shadowed_duplicate(count);
        let plan = plan_introduce_let(request(&input, "0.3.1", true)).expect("plan");

        prop_assert_eq!(plan.occurrence_spans.len(), count);
        prop_assert_eq!(plan.skipped_shadowed_occurrence_spans.len(), 1);
        prop_assert!(SyntaxTree::parse(&plan.rewritten).is_ok());
        prop_assert_eq!(plan.rewritten.matches("(* width height)").count(), 2);
        prop_assert_eq!(plan.rewritten.matches("product").count(), count + 2);
    }
}
