use paredit_core_cli::CliResult;
use paredit_core_syntax::sexpr::ByteSpan;

pub fn validate_manifest_edits(input: &str, edits: &[(ByteSpan, String)]) -> CliResult<()> {
    for (span, _) in edits {
        let start = span.start().get();
        let end = span.end().get();
        if start > end || end > input.len() {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::RefusalSpanOutOfBounds,
                format!(
                    "edit span {}..{} is outside input length {}",
                    start,
                    end,
                    input.len()
                ),
            )
            .into());
        }
        if !input.is_char_boundary(start) || !input.is_char_boundary(end) {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::RefusalSpanOutOfBounds,
                format!("edit span {start}..{end} is not on UTF-8 character boundaries"),
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use paredit_core_syntax::sexpr::ByteOffset;
    use proptest::prelude::*;

    use super::*;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(96))]

        #[test]
        fn pbt_accepts_ascii_spans_when_bounds_are_ordered_and_in_range(
            input in "[a-z]{0,48}",
            start_seed in 0usize..64,
            end_seed in 0usize..64,
        ) {
            let start = start_seed.min(input.len());
            let end = end_seed.min(input.len());
            let (start, end) = if start <= end { (start, end) } else { (end, start) };
            let edits = vec![(span(start, end), "replacement".to_string())];

            prop_assert!(validate_manifest_edits(&input, &edits).is_ok());
        }

        #[test]
        fn pbt_rejects_spans_past_the_input_end(
            input in "[a-z]{0,48}",
            overflow in 1usize..16,
        ) {
            let edits = vec![(span(0, input.len() + overflow), String::new())];

            prop_assert!(validate_manifest_edits(&input, &edits).is_err());
        }

        #[test]
        fn pbt_rejects_reversed_spans(
            input in "[a-z]{1,48}",
            end in 0usize..48,
            distance in 1usize..16,
        ) {
            let end = end.min(input.len() - 1);
            let start = end + distance;
            let edits = vec![(span(start, end), String::new())];

            prop_assert!(validate_manifest_edits(&input, &edits).is_err());
        }
    }

    #[test]
    fn rejects_spans_that_split_utf8_characters() {
        let edits = vec![(span(1, 2), String::new())];

        assert!(validate_manifest_edits("é", &edits).is_err());
    }
}
