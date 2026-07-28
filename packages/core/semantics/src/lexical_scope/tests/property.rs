use proptest::prelude::*;

use super::*;

proptest! {
    #[test]
    fn pbt_shadowed_lambda_references_are_not_counted(count in 1usize..12) {
        let lambdas = std::iter::repeat_n("(lambda (x) x)", count)
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!("(list x {lambdas})");

        prop_assert_eq!(reference_texts(&input, "x"), vec!["x"]);
    }

    #[test]
    fn pbt_sequential_let_counts_values_before_shadowing(count in 1usize..12) {
        let earlier_bindings = (0..count)
            .map(|index| format!("(y{index} x)"))
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!("(let* ({earlier_bindings} (x 2)) (list x))");

        prop_assert_eq!(reference_texts(&input, "x").len(), count);
    }

    #[test]
    fn pbt_clojure_vector_let_counts_values_before_shadowing(count in 1usize..12) {
        let earlier_bindings = (0..count)
            .map(|index| format!("y{index} x"))
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!("(let [{earlier_bindings} x 2] (list x))");

        prop_assert_eq!(reference_texts(&input, "x").len(), count);
    }
}
