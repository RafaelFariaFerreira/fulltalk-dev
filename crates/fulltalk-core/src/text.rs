//! Post-processing of raw model output.

/// Collapses whitespace and drops output that carries no words (e.g. a lone `"."`
/// the model sometimes emits for silence). Returns an empty string in that case.
pub fn normalize(raw: &str) -> String {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().any(char::is_alphanumeric) {
        joined
    } else {
        String::new()
    }
}

/// Shortens `text` to at most `max_chars` characters, ending with `…` when cut.
/// Counts Unicode scalar values so accented Portuguese text is never split mid-char.
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let keep = max_chars.saturating_sub(1);
    let cut: String = text.chars().take(keep).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("  olá   mundo  ", "olá mundo")]
    #[case("linha\num\tdois", "linha um dois")]
    #[case(".", "")]
    #[case(" ... ? ", "")]
    #[case("", "")]
    #[case("Ok.", "Ok.")]
    #[case("15h", "15h")]
    fn normalizes_model_output(#[case] raw: &str, #[case] expected: &str) {
        assert_eq!(normalize(raw), expected);
    }

    #[rstest]
    #[case("curto", 10, "curto")]
    #[case("exatamente", 10, "exatamente")]
    #[case("ação é ótima demais", 8, "ação é…")]
    #[case("abc", 0, "…")]
    fn ellipsizes_by_characters(#[case] text: &str, #[case] max: usize, #[case] expected: &str) {
        assert_eq!(ellipsize(text, max), expected);
    }
}
