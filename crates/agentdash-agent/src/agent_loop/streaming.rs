pub(super) fn compute_suffix(existing: &str, incoming: &str) -> String {
    if incoming.is_empty() {
        return String::new();
    }
    if existing.is_empty() {
        return incoming.to_string();
    }
    if let Some(suffix) = incoming.strip_prefix(existing) {
        suffix.to_string()
    } else if existing == incoming || existing.ends_with(incoming) {
        String::new()
    } else {
        incoming.to_string()
    }
}
