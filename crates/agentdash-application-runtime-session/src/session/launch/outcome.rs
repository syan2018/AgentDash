#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchCommandOutcome {
    pub turn_id: String,
    pub context_sources: Vec<String>,
}
