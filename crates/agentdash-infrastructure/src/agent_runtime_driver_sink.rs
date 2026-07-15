use agentdash_agent_runtime::DriverEventAdmission;
use agentdash_agent_runtime_contract::DriverError;

pub(crate) fn admit_driver_event_to_pump(
    admission: DriverEventAdmission,
) -> Result<(), DriverError> {
    match admission {
        DriverEventAdmission::Terminalized { sequence } => Err(DriverError::Terminalized {
            reason: format!(
                "Managed Runtime committed a critical violation at event sequence {}",
                sequence.0
            ),
        }),
        DriverEventAdmission::Durable { .. }
        | DriverEventAdmission::Transient
        | DriverEventAdmission::Observed
        | DriverEventAdmission::Quarantined => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminalized_admission_stops_the_driver_event_pump() {
        assert!(matches!(
            admit_driver_event_to_pump(DriverEventAdmission::Terminalized {
                sequence: agentdash_agent_runtime_contract::EventSequence(19),
            }),
            Err(DriverError::Terminalized { reason })
                if reason.contains("event sequence 19")
        ));
        assert!(admit_driver_event_to_pump(DriverEventAdmission::Transient).is_ok());
    }
}
