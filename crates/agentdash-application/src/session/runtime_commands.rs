pub use agentdash_spi::session_persistence::{RuntimeCommandRecord, RuntimeCommandStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_command_status_round_trips_wire_values() {
        assert_eq!(
            RuntimeCommandStatus::try_from("requested"),
            Ok(RuntimeCommandStatus::Requested)
        );
        assert_eq!(RuntimeCommandStatus::Applied.as_str(), "applied");
        assert!(RuntimeCommandStatus::try_from("unknown").is_err());
    }
}
