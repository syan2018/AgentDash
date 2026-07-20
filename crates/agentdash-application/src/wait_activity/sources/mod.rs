mod exec;
mod lifecycle_gate;

pub(crate) use exec::{exec_item_from_terminal, terminal_belongs_to_scope};
pub(crate) use lifecycle_gate::{gate_belongs_to_scope, gate_item_from_gate};

fn bound_string(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let bounded = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}...")
    } else {
        bounded
    }
}
