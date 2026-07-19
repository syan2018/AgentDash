use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentPayloadDigest, AgentProfileDigest, AgentServiceDescriptor,
    AgentServiceInstanceId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ts_rs::TS;

use crate::{RuntimeWireEnvelope, RuntimeWireU64};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[schemars(transparent)]
#[ts(type = "RuntimeWireU64")]
pub struct RuntimeWireAdvertisementRevision(
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "RuntimeWireU64")]
    pub u64,
);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[schemars(transparent)]
#[ts(type = "RuntimeWireU64")]
pub struct RuntimeWirePlacementStreamId(
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "RuntimeWireU64")]
    pub u64,
);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[schemars(transparent)]
#[ts(type = "RuntimeWireU64")]
pub struct RuntimeWirePlacementSequence(
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "RuntimeWireU64")]
    pub u64,
);

/// The authenticated Relay transport coordinates are assigned by Cloud.
///
/// A Local advertisement can claim Agent coordinates, but it cannot choose or rewrite these
/// transport coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireAuthenticatedTransport {
    pub backend_id: String,
    pub transport_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireSignedDeploymentEvidence {
    pub signer: String,
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}

/// A Local endpoint claim. Host verification must independently pin every security-sensitive
/// coordinate before this claim can become a Runtime offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireServiceOfferAdvertisement {
    pub endpoint_id: String,
    pub revision: RuntimeWireAdvertisementRevision,
    pub digest: AgentPayloadDigest,
    pub host_incarnation_id: String,
    pub service_instance_id: AgentServiceInstanceId,
    pub binding_generation: AgentBindingGeneration,
    pub descriptor: AgentServiceDescriptor,
    pub publisher_integration: String,
    pub service_version: String,
    pub claimed_build_digest: AgentPayloadDigest,
    pub claimed_conformance_suite_revision: String,
    pub deployment_manifest_id: String,
    pub deployment_manifest_revision: String,
    pub advertised_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_deployment_evidence: Option<RuntimeWireSignedDeploymentEvidence>,
}

impl RuntimeWireServiceOfferAdvertisement {
    pub fn calculated_digest(&self) -> AgentPayloadDigest {
        let mut value =
            serde_json::to_value(self).expect("Runtime Wire advertisement must serialize");
        let object = value
            .as_object_mut()
            .expect("Runtime Wire advertisement must serialize as an object");
        object.remove("digest");
        object.remove("signed_deployment_evidence");
        sort_json_keys(&mut value);
        let canonical =
            serde_json::to_vec(&value).expect("canonical Runtime Wire advertisement must serialize");
        AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
            .expect("SHA-256 advertisement digest is non-empty")
    }

    pub fn validate_shape(&self) -> Result<(), RuntimeWirePlacementProtocolError> {
        for (coordinate, value) in [
            ("endpoint_id", self.endpoint_id.as_str()),
            ("host_incarnation_id", self.host_incarnation_id.as_str()),
            ("publisher_integration", self.publisher_integration.as_str()),
            ("service_version", self.service_version.as_str()),
            (
                "claimed_conformance_suite_revision",
                self.claimed_conformance_suite_revision.as_str(),
            ),
            ("deployment_manifest_id", self.deployment_manifest_id.as_str()),
            (
                "deployment_manifest_revision",
                self.deployment_manifest_revision.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(RuntimeWirePlacementProtocolError::InvalidCoordinate {
                    coordinate: coordinate.to_owned(),
                });
            }
        }
        if self.revision.0 == 0 {
            return Err(RuntimeWirePlacementProtocolError::InvalidCoordinate {
                coordinate: "revision".to_owned(),
            });
        }
        if self.binding_generation.0 == 0 {
            return Err(RuntimeWirePlacementProtocolError::InvalidCoordinate {
                coordinate: "binding_generation".to_owned(),
            });
        }
        if self.advertised_at_unix_ms >= self.expires_at_unix_ms {
            return Err(RuntimeWirePlacementProtocolError::InvalidFreshness);
        }
        if self.digest != self.calculated_digest() {
            return Err(RuntimeWirePlacementProtocolError::AdvertisementDigestMismatch);
        }
        Ok(())
    }

    pub fn is_fresh_at(&self, unix_ms: i64) -> bool {
        self.advertised_at_unix_ms <= unix_ms && unix_ms < self.expires_at_unix_ms
    }
}

/// Immutable coordinates of one placement connection epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementProvenance {
    pub transport: RuntimeWireAuthenticatedTransport,
    pub endpoint_id: String,
    pub host_incarnation_id: String,
    pub service_instance_id: AgentServiceInstanceId,
    pub binding_generation: AgentBindingGeneration,
    pub advertisement_revision: RuntimeWireAdvertisementRevision,
    pub advertisement_digest: AgentPayloadDigest,
    pub profile_digest: AgentProfileDigest,
}

impl RuntimeWirePlacementProvenance {
    pub fn matches_advertisement(
        &self,
        advertisement: &RuntimeWireServiceOfferAdvertisement,
    ) -> bool {
        self.endpoint_id == advertisement.endpoint_id
            && self.host_incarnation_id == advertisement.host_incarnation_id
            && self.service_instance_id == advertisement.service_instance_id
            && self.binding_generation == advertisement.binding_generation
            && self.advertisement_revision == advertisement.revision
            && self.advertisement_digest == advertisement.digest
            && self.profile_digest == advertisement.descriptor.profile_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementOpen {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub protocol_revision: u32,
    pub max_in_flight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementOpenAck {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub protocol_revision: u32,
    pub max_in_flight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementOpenRejected {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub code: RuntimeWirePlacementCloseCode,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementFrame {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub sequence: RuntimeWirePlacementSequence,
    pub envelope: RuntimeWireEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementAck {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub through_sequence: RuntimeWirePlacementSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementClosed {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub code: RuntimeWirePlacementCloseCode,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWirePlacementLost {
    pub stream_id: RuntimeWirePlacementStreamId,
    pub provenance: RuntimeWirePlacementProvenance,
    pub code: RuntimeWirePlacementLossCode,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeWirePlacementCloseCode {
    Completed,
    Rejected,
    EndpointWithdrawn,
    ProtocolViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeWirePlacementLossCode {
    TransportDisconnected,
    QueueOverflow,
    SequenceGap,
    StaleProvenance,
    EndpointUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeWirePlacementProtocolError {
    #[error("runtime wire placement coordinate is invalid: {coordinate}")]
    InvalidCoordinate {
        coordinate: String,
    },
    #[error("runtime wire advertisement freshness interval is invalid")]
    InvalidFreshness,
    #[error("runtime wire advertisement revision regressed")]
    AdvertisementRevisionRegression {
        current: RuntimeWireAdvertisementRevision,
        received: RuntimeWireAdvertisementRevision,
    },
    #[error("runtime wire advertisement revision conflicts with a prior claim")]
    AdvertisementRevisionConflict {
        revision: RuntimeWireAdvertisementRevision,
    },
    #[error("runtime wire advertisement digest does not match its canonical payload")]
    AdvertisementDigestMismatch,
    #[error("runtime wire placement provenance is stale")]
    StaleProvenance,
    #[error("runtime wire placement sequence contains a gap")]
    SequenceGap {
        expected: RuntimeWirePlacementSequence,
        received: RuntimeWirePlacementSequence,
    },
    #[error("runtime wire placement sequence is exhausted")]
    SequenceExhausted,
    #[error("runtime wire placement stream identity is exhausted")]
    StreamIdExhausted,
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(object) => {
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (_, value) in &mut entries {
                sort_json_keys(value);
            }
            object.extend(entries);
        }
        _ => {}
    }
}
