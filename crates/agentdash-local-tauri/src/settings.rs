pub(crate) fn env_trimmed(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(normalize_optional_env_text)
}

pub(crate) fn normalize_optional_env_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
