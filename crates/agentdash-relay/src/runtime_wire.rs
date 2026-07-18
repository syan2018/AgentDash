//! Ordered, replayable placement transport for AgentDash-owned Runtime Wire frames.

use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    HostIncarnationId, ProfileDigest, RuntimeDriverGeneration, RuntimeProfile,
    RuntimeServiceInstanceId,
};
use agentdash_agent_runtime_wire::{RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireEnvelope};
use agentdash_integration_api::{AgentRuntimePlacementId, AgentServiceDefinitionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeRelayStreamId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayProvenance {
    pub service_definition_id: AgentServiceDefinitionId,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub driver_generation: RuntimeDriverGeneration,
    pub host_incarnation_id: HostIncarnationId,
    pub host_id: String,
    pub transport_id: AgentRuntimePlacementId,
}

/// Verified Agent service offer advertised by a Local Integration Host.
///
/// This inventory deliberately excludes service config, credential references and secrets. Cloud
/// may create a remote proxy offer only for this exact instance/generation/profile evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeOfferAdvertisement {
    pub definition_id: AgentServiceDefinitionId,
    pub publisher_integration: String,
    pub service_version: String,
    pub build_digest: String,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub instance_revision: u64,
    pub driver_generation: RuntimeDriverGeneration,
    pub host_incarnation_id: HostIncarnationId,
    pub protocol_revision: u32,
    pub effective_profile: agentdash_agent_runtime_contract::EffectiveRuntimeProfile,
    pub profile_digest: ProfileDigest,
    pub conformance_suite_revision: String,
    pub conformance_driver_build_digest: String,
    pub conformance_verified_profile_digest: ProfileDigest,
    pub conformance_verified_at: chrono::DateTime<chrono::Utc>,
    pub transport_id: AgentRuntimePlacementId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayTransportDescriptor {
    pub supported_protocol_revisions: Vec<u32>,
    pub profile: RuntimeProfile,
    pub profile_digest: ProfileDigest,
    pub max_in_flight_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayOpen {
    pub stream_id: RuntimeRelayStreamId,
    pub provenance: RuntimeRelayProvenance,
    pub supported_protocol_revisions: Vec<u32>,
    pub resume_after_sequence: u64,
    pub max_in_flight_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayOpenAck {
    pub stream_id: RuntimeRelayStreamId,
    pub selected_protocol_revision: u32,
    pub accepted_after_sequence: u64,
    pub transport_profile: RuntimeProfile,
    pub transport_profile_digest: ProfileDigest,
    pub max_in_flight_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayFrame {
    pub stream_id: RuntimeRelayStreamId,
    pub sequence: u64,
    pub provenance: RuntimeRelayProvenance,
    pub envelope: RuntimeWireEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRelayAck {
    pub stream_id: RuntimeRelayStreamId,
    pub through_sequence: u64,
}

/// A placement-loss signal. Driver Host/Managed Runtime consumes this once and performs the
/// canonical binding/active-turn Lost transition; Relay itself does not synthesize a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRelayDisconnect {
    pub stream_id: RuntimeRelayStreamId,
    pub provenance: RuntimeRelayProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeRelayReceive {
    Accepted(Box<RuntimeWireEnvelope>),
    Duplicate { sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeRelayTransportError {
    #[error("runtime relay stream is disconnected")]
    Disconnected,
    #[error("runtime relay stream provenance changed")]
    ProvenanceMismatch,
    #[error("runtime relay protocol revision is unsupported")]
    UnsupportedProtocolRevision,
    #[error("runtime relay frame sequence gap: expected {expected}, received {received}")]
    SequenceGap { expected: u64, received: u64 },
    #[error("runtime relay ack {received} exceeds last sent sequence {last_sent}")]
    InvalidAck { received: u64, last_sent: u64 },
    #[error("runtime relay backpressure limit {limit} reached")]
    Backpressure { limit: usize },
}

#[derive(Debug, Clone)]
pub struct RuntimeRelayStream {
    stream_id: RuntimeRelayStreamId,
    provenance: RuntimeRelayProvenance,
    protocol_revision: u32,
    max_in_flight: usize,
    next_outbound_sequence: u64,
    next_inbound_sequence: u64,
    acknowledged_outbound_sequence: u64,
    outbound: BTreeMap<u64, RuntimeRelayFrame>,
    connected: bool,
}

impl RuntimeRelayStream {
    pub fn connect(
        open: RuntimeRelayOpen,
        ack: &RuntimeRelayOpenAck,
    ) -> Result<Self, RuntimeRelayTransportError> {
        if ack.stream_id != open.stream_id {
            return Err(RuntimeRelayTransportError::ProvenanceMismatch);
        }
        if !open
            .supported_protocol_revisions
            .contains(&ack.selected_protocol_revision)
            || ack.selected_protocol_revision != RUNTIME_WIRE_PROTOCOL_REVISION
        {
            return Err(RuntimeRelayTransportError::UnsupportedProtocolRevision);
        }
        if ack.max_in_flight_frames == 0 || ack.max_in_flight_frames > open.max_in_flight_frames {
            return Err(RuntimeRelayTransportError::Backpressure {
                limit: ack.max_in_flight_frames,
            });
        }
        Ok(Self {
            stream_id: open.stream_id,
            provenance: open.provenance,
            protocol_revision: ack.selected_protocol_revision,
            max_in_flight: ack.max_in_flight_frames,
            next_outbound_sequence: open.resume_after_sequence.saturating_add(1),
            next_inbound_sequence: ack.accepted_after_sequence.saturating_add(1),
            acknowledged_outbound_sequence: ack.accepted_after_sequence,
            outbound: BTreeMap::new(),
            connected: true,
        })
    }

    pub fn negotiate(
        open: RuntimeRelayOpen,
        descriptor: &RuntimeRelayTransportDescriptor,
    ) -> Result<(Self, RuntimeRelayOpenAck), RuntimeRelayTransportError> {
        let selected = open
            .supported_protocol_revisions
            .iter()
            .copied()
            .filter(|revision| descriptor.supported_protocol_revisions.contains(revision))
            .max()
            .filter(|revision| *revision == RUNTIME_WIRE_PROTOCOL_REVISION)
            .ok_or(RuntimeRelayTransportError::UnsupportedProtocolRevision)?;
        let max_in_flight = open
            .max_in_flight_frames
            .min(descriptor.max_in_flight_frames);
        if max_in_flight == 0 {
            return Err(RuntimeRelayTransportError::Backpressure { limit: 0 });
        }
        let ack = RuntimeRelayOpenAck {
            stream_id: open.stream_id.clone(),
            selected_protocol_revision: selected,
            accepted_after_sequence: open.resume_after_sequence,
            transport_profile: descriptor.profile.clone(),
            transport_profile_digest: descriptor.profile_digest.clone(),
            max_in_flight_frames: max_in_flight,
        };
        Ok((
            Self {
                stream_id: open.stream_id,
                provenance: open.provenance,
                protocol_revision: selected,
                max_in_flight,
                next_outbound_sequence: open.resume_after_sequence + 1,
                next_inbound_sequence: open.resume_after_sequence + 1,
                acknowledged_outbound_sequence: open.resume_after_sequence,
                outbound: BTreeMap::new(),
                connected: true,
            },
            ack,
        ))
    }

    pub fn enqueue(
        &mut self,
        envelope: RuntimeWireEnvelope,
    ) -> Result<RuntimeRelayFrame, RuntimeRelayTransportError> {
        if !self.connected {
            return Err(RuntimeRelayTransportError::Disconnected);
        }
        if envelope.protocol_revision != self.protocol_revision {
            return Err(RuntimeRelayTransportError::UnsupportedProtocolRevision);
        }
        if self.outbound.len() >= self.max_in_flight {
            return Err(RuntimeRelayTransportError::Backpressure {
                limit: self.max_in_flight,
            });
        }
        let sequence = self.next_outbound_sequence;
        self.next_outbound_sequence += 1;
        let frame = RuntimeRelayFrame {
            stream_id: self.stream_id.clone(),
            sequence,
            provenance: self.provenance.clone(),
            envelope,
        };
        self.outbound.insert(sequence, frame.clone());
        Ok(frame)
    }

    pub fn resume(
        &mut self,
        open: &RuntimeRelayOpen,
        descriptor: &RuntimeRelayTransportDescriptor,
    ) -> Result<(RuntimeRelayOpenAck, Vec<RuntimeRelayFrame>), RuntimeRelayTransportError> {
        if open.stream_id != self.stream_id || open.provenance != self.provenance {
            return Err(RuntimeRelayTransportError::ProvenanceMismatch);
        }
        if !open
            .supported_protocol_revisions
            .contains(&self.protocol_revision)
            || !descriptor
                .supported_protocol_revisions
                .contains(&self.protocol_revision)
        {
            return Err(RuntimeRelayTransportError::UnsupportedProtocolRevision);
        }
        let max_in_flight = open
            .max_in_flight_frames
            .min(descriptor.max_in_flight_frames);
        if max_in_flight == 0 || self.outbound.len() > max_in_flight {
            return Err(RuntimeRelayTransportError::Backpressure {
                limit: max_in_flight,
            });
        }
        self.max_in_flight = max_in_flight;
        self.connected = true;
        self.acknowledge(RuntimeRelayAck {
            stream_id: self.stream_id.clone(),
            through_sequence: open.resume_after_sequence,
        })?;
        Ok((
            RuntimeRelayOpenAck {
                stream_id: self.stream_id.clone(),
                selected_protocol_revision: self.protocol_revision,
                accepted_after_sequence: self.next_inbound_sequence.saturating_sub(1),
                transport_profile: descriptor.profile.clone(),
                transport_profile_digest: descriptor.profile_digest.clone(),
                max_in_flight_frames: max_in_flight,
            },
            self.replay_unacknowledged(),
        ))
    }

    pub fn receive(
        &mut self,
        frame: RuntimeRelayFrame,
    ) -> Result<RuntimeRelayReceive, RuntimeRelayTransportError> {
        if !self.connected {
            return Err(RuntimeRelayTransportError::Disconnected);
        }
        if frame.stream_id != self.stream_id || frame.provenance != self.provenance {
            return Err(RuntimeRelayTransportError::ProvenanceMismatch);
        }
        if frame.envelope.protocol_revision != self.protocol_revision {
            return Err(RuntimeRelayTransportError::UnsupportedProtocolRevision);
        }
        if frame.sequence < self.next_inbound_sequence {
            return Ok(RuntimeRelayReceive::Duplicate {
                sequence: frame.sequence,
            });
        }
        if frame.sequence > self.next_inbound_sequence {
            return Err(RuntimeRelayTransportError::SequenceGap {
                expected: self.next_inbound_sequence,
                received: frame.sequence,
            });
        }
        self.next_inbound_sequence += 1;
        Ok(RuntimeRelayReceive::Accepted(Box::new(frame.envelope)))
    }

    pub fn acknowledge(&mut self, ack: RuntimeRelayAck) -> Result<(), RuntimeRelayTransportError> {
        if ack.stream_id != self.stream_id {
            return Err(RuntimeRelayTransportError::ProvenanceMismatch);
        }
        let last_sent = self.next_outbound_sequence.saturating_sub(1);
        if ack.through_sequence > last_sent {
            return Err(RuntimeRelayTransportError::InvalidAck {
                received: ack.through_sequence,
                last_sent,
            });
        }
        if ack.through_sequence <= self.acknowledged_outbound_sequence {
            return Ok(());
        }
        self.acknowledged_outbound_sequence = ack.through_sequence;
        self.outbound
            .retain(|sequence, _| *sequence > ack.through_sequence);
        Ok(())
    }

    pub fn replay_unacknowledged(&self) -> Vec<RuntimeRelayFrame> {
        self.outbound.values().cloned().collect()
    }

    pub fn abandon_unacknowledged(&mut self) {
        self.outbound.clear();
        self.acknowledged_outbound_sequence = self.next_outbound_sequence.saturating_sub(1);
    }

    pub fn disconnect(&mut self) -> Option<RuntimeRelayDisconnect> {
        if !self.connected {
            return None;
        }
        self.connected = false;
        Some(RuntimeRelayDisconnect {
            stream_id: self.stream_id.clone(),
            provenance: self.provenance.clone(),
        })
    }

    pub fn reconnect(
        &mut self,
        provenance: &RuntimeRelayProvenance,
    ) -> Result<Vec<RuntimeRelayFrame>, RuntimeRelayTransportError> {
        if provenance != &self.provenance {
            return Err(RuntimeRelayTransportError::ProvenanceMismatch);
        }
        self.connected = true;
        Ok(self.replay_unacknowledged())
    }

    pub fn inbound_ack(&self) -> RuntimeRelayAck {
        RuntimeRelayAck {
            stream_id: self.stream_id.clone(),
            through_sequence: self.next_inbound_sequence.saturating_sub(1),
        }
    }

    pub fn provenance(&self) -> &RuntimeRelayProvenance {
        &self.provenance
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use agentdash_agent_runtime_contract::*;
    use agentdash_agent_runtime_wire::{
        RuntimeWireAck as WireAck, RuntimeWireFrame, RuntimeWireFrameId,
    };

    use super::*;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid id")
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            reference_class: ReferenceRuntimeClass::Conversation,
            input: InputProfile {
                modalities: [InputModality::Text].into(),
            },
            instruction: InstructionProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            tools: ToolProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
                cancellation: true,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::HostAdaptedBoundary,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: true,
            },
            lifecycle: [
                LifecycleCapability::ThreadStart,
                LifecycleCapability::TurnStart,
                LifecycleCapability::TurnInterrupt,
            ]
            .into(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            context: ContextProfile {
                capabilities: BTreeSet::new(),
                fidelity: ContextFidelity::EventProjected,
                activation_idempotent: false,
            },
            telemetry_config: BTreeSet::new(),
        }
    }

    fn provenance() -> RuntimeRelayProvenance {
        RuntimeRelayProvenance {
            service_definition_id: AgentServiceDefinitionId::new("service-definition-remote")
                .expect("definition id"),
            service_instance_id: id("service-remote"),
            driver_generation: RuntimeDriverGeneration(4),
            host_incarnation_id: HostIncarnationId::new("host-incarnation-1")
                .expect("host incarnation id"),
            host_id: "backend-local-a".to_string(),
            transport_id: AgentRuntimePlacementId::new("runtime-wire").expect("transport id"),
        }
    }

    fn descriptor(limit: usize) -> RuntimeRelayTransportDescriptor {
        RuntimeRelayTransportDescriptor {
            supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
            profile: profile(),
            profile_digest: id("transport-profile"),
            max_in_flight_frames: limit,
        }
    }

    fn envelope(frame_id: u64) -> RuntimeWireEnvelope {
        RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: agentdash_agent_runtime_wire::RuntimeWireFrameId(frame_id),
            critical: true,
            frame: RuntimeWireFrame::Ack(WireAck {
                through_frame_id: agentdash_agent_runtime_wire::RuntimeWireFrameId(frame_id),
            }),
        }
    }

    fn stream(limit: usize) -> RuntimeRelayStream {
        RuntimeRelayStream::negotiate(
            RuntimeRelayOpen {
                stream_id: RuntimeRelayStreamId("runtime-stream".to_string()),
                provenance: provenance(),
                supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                resume_after_sequence: 0,
                max_in_flight_frames: limit,
            },
            &descriptor(limit),
        )
        .expect("negotiate")
        .0
    }

    #[test]
    fn relay_negotiates_only_the_canonical_revision_four() {
        assert_eq!(RUNTIME_WIRE_PROTOCOL_REVISION, 4);
        let error = RuntimeRelayStream::negotiate(
            RuntimeRelayOpen {
                stream_id: RuntimeRelayStreamId("legacy-stream".to_owned()),
                provenance: provenance(),
                supported_protocol_revisions: vec![3],
                resume_after_sequence: 0,
                max_in_flight_frames: 4,
            },
            &descriptor(4),
        )
        .expect_err("revision three must not negotiate after the hard cut");

        assert_eq!(
            error,
            RuntimeRelayTransportError::UnsupportedProtocolRevision
        );
    }

    #[test]
    fn ack_replay_and_reconnect_preserve_unacknowledged_order() {
        let mut stream = stream(4);
        let first = stream.enqueue(envelope(11)).expect("first");
        let second = stream.enqueue(envelope(12)).expect("second");
        stream
            .acknowledge(RuntimeRelayAck {
                stream_id: first.stream_id,
                through_sequence: first.sequence,
            })
            .expect("ack first");
        assert!(stream.disconnect().is_some());
        assert!(
            stream.disconnect().is_none(),
            "disconnect loss must be emitted once"
        );
        let replay = stream.reconnect(&provenance()).expect("reconnect");
        assert_eq!(replay, vec![second]);
    }

    #[test]
    fn same_provenance_resume_prunes_acknowledged_and_replays_the_rest() {
        let mut stream = stream(4);
        let first = stream.enqueue(envelope(1)).expect("first");
        let second = stream.enqueue(envelope(2)).expect("second");
        stream.disconnect();
        let (ack, replay) = stream
            .resume(
                &RuntimeRelayOpen {
                    stream_id: first.stream_id,
                    provenance: provenance(),
                    supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                    resume_after_sequence: first.sequence,
                    max_in_flight_frames: 4,
                },
                &descriptor(4),
            )
            .expect("resume");
        assert_eq!(ack.accepted_after_sequence, 0);
        assert_eq!(replay, vec![second]);
    }

    #[test]
    fn inbound_duplicates_are_idempotent_and_gaps_are_rejected() {
        let mut stream = stream(4);
        let frame = RuntimeRelayFrame {
            stream_id: RuntimeRelayStreamId("runtime-stream".to_string()),
            sequence: 1,
            provenance: provenance(),
            envelope: envelope(1),
        };
        assert!(matches!(
            stream.receive(frame.clone()),
            Ok(RuntimeRelayReceive::Accepted(_))
        ));
        assert_eq!(
            stream.receive(frame),
            Ok(RuntimeRelayReceive::Duplicate { sequence: 1 })
        );
        let mut gap = RuntimeRelayFrame {
            stream_id: RuntimeRelayStreamId("runtime-stream".to_string()),
            sequence: 3,
            provenance: provenance(),
            envelope: envelope(3),
        };
        assert!(matches!(
            stream.receive(gap.clone()),
            Err(RuntimeRelayTransportError::SequenceGap {
                expected: 2,
                received: 3
            })
        ));
        gap.sequence = 2;
        stream.receive(gap).expect("fill gap");
    }

    #[test]
    fn in_flight_limit_applies_backpressure_before_dropping_frames() {
        let mut stream = stream(1);
        stream.enqueue(envelope(1)).expect("first");
        assert_eq!(
            stream.enqueue(envelope(2)),
            Err(RuntimeRelayTransportError::Backpressure { limit: 1 })
        );
        assert_eq!(stream.replay_unacknowledged().len(), 1);
    }

    #[test]
    fn relay_message_carries_the_owned_wire_envelope_without_value_conversion() {
        let message = crate::RelayMessage::RuntimeWireFrame {
            id: "runtime-wire-1".to_string(),
            payload: Box::new(RuntimeRelayFrame {
                stream_id: RuntimeRelayStreamId("runtime-stream".to_string()),
                sequence: 1,
                provenance: provenance(),
                envelope: envelope(9),
            }),
        };
        let encoded = serde_json::to_string(&message).expect("serialize typed Runtime Wire frame");
        let decoded: crate::RelayMessage =
            serde_json::from_str(&encoded).expect("deserialize typed Runtime Wire frame");
        let crate::RelayMessage::RuntimeWireFrame { payload, .. } = decoded else {
            panic!("expected Runtime Wire frame")
        };
        assert_eq!(payload.envelope.frame_id, RuntimeWireFrameId(9));
    }
}
