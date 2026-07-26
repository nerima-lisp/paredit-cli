use super::types::BlockReplacement;

pub(super) fn apply_replacements(input: &str, replacements: &[BlockReplacement]) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for replacement in replacements {
        output.push_str(&input[cursor..replacement.start]);
        output.push_str(&replacement.text);
        cursor = replacement.end;
    }
    output.push_str(&input[cursor..]);
    output
}
