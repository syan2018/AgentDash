use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{InteractionError, InteractionResult};

pub const MAX_PRESENTATION_STATE_BYTES: usize = 64 * 1024;
pub const MAX_RENDERER_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionPresentationState {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub user_id: String,
    pub presentation_key: String,
    pub revision: u64,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
}

impl InteractionPresentationState {
    pub fn new(
        instance_id: Uuid,
        user_id: impl Into<String>,
        presentation_key: impl Into<String>,
        value: Value,
        now: DateTime<Utc>,
    ) -> InteractionResult<Self> {
        let state = Self {
            id: Uuid::new_v4(),
            instance_id,
            user_id: user_id.into(),
            presentation_key: presentation_key.into(),
            revision: 1,
            value,
            updated_at: now,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn replace(
        &mut self,
        expected_revision: u64,
        value: Value,
        now: DateTime<Utc>,
    ) -> InteractionResult<()> {
        if self.revision != expected_revision {
            return Err(InteractionError::StateRevisionConflict {
                instance_id: self.instance_id,
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut next = self.clone();
        next.value = value;
        next.revision = self
            .revision
            .checked_add(1)
            .ok_or(InteractionError::InvalidField {
                field: "interaction_presentation_state.revision",
                reason: "revision 已达上限",
            })?;
        next.updated_at = now;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> InteractionResult<()> {
        let bytes = serde_json::to_vec(&self.value)
            .map_err(|error| InteractionError::Serialization {
                context: "interaction_presentation_state.value",
                message: error.to_string(),
            })?
            .len();
        if self.id.is_nil()
            || self.instance_id.is_nil()
            || self.user_id.trim().is_empty()
            || !valid_scoped_key(
                &self.presentation_key,
                &["canvas:", "interaction:", "presentation:"],
            )
            || self.revision == 0
            || bytes > MAX_PRESENTATION_STATE_BYTES
        {
            return Err(InteractionError::InvalidField {
                field: "interaction_presentation_state",
                reason: "identity、revision 与 bounded value 必须有效",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRendererLease {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub renderer_key: String,
    pub user_id: String,
    pub revision: u64,
    pub acquired_at: DateTime<Utc>,
    pub renewed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl InteractionRendererLease {
    pub fn acquire(
        instance_id: Uuid,
        renderer_key: impl Into<String>,
        user_id: impl Into<String>,
        now: DateTime<Utc>,
        ttl: Duration,
    ) -> InteractionResult<Self> {
        let lease = Self {
            id: Uuid::new_v4(),
            instance_id,
            renderer_key: renderer_key.into(),
            user_id: user_id.into(),
            revision: 1,
            acquired_at: now,
            renewed_at: now,
            expires_at: now
                .checked_add_signed(ttl)
                .ok_or(InteractionError::InvalidField {
                    field: "interaction_renderer_lease.ttl",
                    reason: "TTL 超出可表示范围",
                })?,
        };
        lease.validate(now)?;
        Ok(lease)
    }

    pub fn renew(&mut self, now: DateTime<Utc>, ttl: Duration) -> InteractionResult<()> {
        if !self.is_active_at(now) {
            return Err(InteractionError::InvalidStatusTransition {
                from: "expired",
                to: "renewed",
            });
        }
        let expires_at = now
            .checked_add_signed(ttl)
            .ok_or(InteractionError::InvalidField {
                field: "interaction_renderer_lease.ttl",
                reason: "TTL 超出可表示范围",
            })?;
        let mut next = self.clone();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(InteractionError::InvalidField {
                field: "interaction_renderer_lease.revision",
                reason: "revision 已达上限",
            })?;
        next.renewed_at = now;
        next.expires_at = expires_at;
        next.validate(now)?;
        *self = next;
        Ok(())
    }

    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at > now
    }

    pub fn validate(&self, now: DateTime<Utc>) -> InteractionResult<()> {
        let ttl = self.expires_at - self.renewed_at;
        if self.id.is_nil()
            || self.instance_id.is_nil()
            || self.revision == 0
            || !valid_scoped_key(&self.renderer_key, &["renderer:"])
            || self.user_id.trim().is_empty()
            || ttl <= Duration::zero()
            || ttl > Duration::seconds(MAX_RENDERER_LEASE_SECONDS)
            || self.renewed_at < self.acquired_at
            || self.expires_at <= now
        {
            return Err(InteractionError::InvalidField {
                field: "interaction_renderer_lease",
                reason: "identity 与 TTL 必须有效且有界",
            });
        }
        Ok(())
    }
}

fn valid_scoped_key(value: &str, prefixes: &[&str]) -> bool {
    value.len() <= 128
        && value.trim() == value
        && prefixes.iter().any(|prefix| {
            value.strip_prefix(prefix).is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix.chars().all(|character| {
                        character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '_' | '.' | ':' | '/')
                    })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_state_is_bounded_and_revisioned() {
        let now = Utc::now();
        let mut state = InteractionPresentationState::new(
            Uuid::new_v4(),
            "u",
            "presentation:canvas-editor",
            serde_json::json!({"panel":"source"}),
            now,
        )
        .expect("state");
        state
            .replace(1, serde_json::json!({"panel":"preview"}), now)
            .expect("replace");
        assert_eq!(state.revision, 2);
    }

    #[test]
    fn renderer_lease_rejects_unbounded_ttl() {
        let now = Utc::now();
        let error = InteractionRendererLease::acquire(
            Uuid::new_v4(),
            "renderer:1",
            "u",
            now,
            Duration::seconds(MAX_RENDERER_LEASE_SECONDS + 1),
        )
        .expect_err("ttl");
        assert!(matches!(error, InteractionError::InvalidField { .. }));
    }

    #[test]
    fn presentation_revision_cannot_overflow() {
        let now = Utc::now();
        let mut state = InteractionPresentationState::new(
            Uuid::new_v4(),
            "u",
            "presentation:canvas-editor",
            serde_json::json!({}),
            now,
        )
        .expect("state");
        state.revision = u64::MAX;
        assert!(state.replace(u64::MAX, serde_json::json!({}), now).is_err());
    }

    #[test]
    fn expired_renderer_lease_cannot_be_renewed() {
        let now = Utc::now();
        let mut lease = InteractionRendererLease::acquire(
            Uuid::new_v4(),
            "renderer:1",
            "u",
            now,
            Duration::seconds(1),
        )
        .expect("lease");
        let error = lease
            .renew(now + Duration::seconds(2), Duration::seconds(1))
            .expect_err("expired");
        assert!(matches!(
            error,
            InteractionError::InvalidStatusTransition {
                from: "expired",
                ..
            }
        ));
    }

    #[test]
    fn presentation_and_renderer_keys_require_canonical_prefixes() {
        let now = Utc::now();
        assert!(
            InteractionPresentationState::new(
                Uuid::new_v4(),
                "u",
                "canvas:editor",
                serde_json::json!({}),
                now,
            )
            .is_ok()
        );
        assert!(
            InteractionPresentationState::new(
                Uuid::new_v4(),
                "u",
                "editor",
                serde_json::json!({}),
                now,
            )
            .is_err()
        );
        assert!(
            InteractionRendererLease::acquire(
                Uuid::new_v4(),
                "canvas:renderer",
                "u",
                now,
                Duration::seconds(1),
            )
            .is_err()
        );
    }

    #[test]
    fn renderer_renew_advances_fencing_revision_and_expired_reacquire_has_new_id() {
        let now = Utc::now();
        let mut lease = InteractionRendererLease::acquire(
            Uuid::new_v4(),
            "renderer:tab",
            "u",
            now,
            Duration::seconds(10),
        )
        .expect("lease");
        let original_id = lease.id;
        lease
            .renew(now + Duration::seconds(1), Duration::seconds(10))
            .expect("renew");
        assert_eq!(lease.id, original_id);
        assert_eq!(lease.revision, 2);
        assert_eq!(lease.renewed_at, now + Duration::seconds(1));

        let replacement = InteractionRendererLease::acquire(
            lease.instance_id,
            "renderer:tab",
            "u",
            lease.expires_at,
            Duration::seconds(10),
        )
        .expect("replacement");
        assert_ne!(replacement.id, original_id);
        assert_eq!(replacement.revision, 1);
    }
}
