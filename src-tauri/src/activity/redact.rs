pub fn prompt_text(raw: &str) -> String {
    raw.chars().take(500).collect()
}
