use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;

pub const SESSION_TOOL_RESULT_CACHE_DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Default)]
pub struct SessionToolResultCache {
    inner: RwLock<HashMap<SessionToolResultCacheKey, SessionToolResultCacheEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionToolResultCacheKey {
    session_id: String,
    item_id: String,
}

#[derive(Debug, Clone)]
struct SessionToolResultCacheEntry {
    metadata: SessionToolResultCacheMetadata,
    text: Arc<str>,
    expires_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionToolResultCacheMetadata {
    pub session_id: String,
    pub item_id: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: String,
    pub lifecycle_path: String,
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub tool_name: String,
    pub original_bytes: usize,
    pub stored_bytes: usize,
    pub created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionToolResultCacheStatusKind {
    Missing,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionToolResultCacheStatus {
    pub status: SessionToolResultCacheStatusKind,
    pub session_id: String,
    pub item_id: String,
    pub lifecycle_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolResultCacheRead {
    Available {
        metadata: SessionToolResultCacheMetadata,
        text: String,
    },
    Unavailable(SessionToolResultCacheStatus),
}

#[derive(Debug, Clone)]
pub struct SessionToolResultCachePut {
    pub session_id: String,
    pub item_id: String,
    pub lifecycle_path: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: String,
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub tool_name: String,
    pub text: String,
    pub original_bytes: usize,
}

impl SessionToolResultCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn put_text(
        &self,
        session_id: impl Into<String>,
        item_id: impl Into<String>,
        text: impl Into<String>,
        original_bytes: usize,
    ) -> SessionToolResultCacheMetadata {
        let session_id = session_id.into();
        let item_id = item_id.into();
        let (turn_alias, body_alias) = readable_aliases_from_item_id(&item_id);
        let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
        self.put_text_entry(SessionToolResultCachePut {
            session_id,
            item_id,
            lifecycle_path,
            turn_alias,
            body_alias,
            body_kind: "tool_result".to_string(),
            raw_turn_id: String::new(),
            raw_tool_call_id: String::new(),
            tool_name: String::new(),
            text: text.into(),
            original_bytes,
        })
    }

    pub fn put_text_entry(&self, put: SessionToolResultCachePut) -> SessionToolResultCacheMetadata {
        self.put_text_entry_with_ttl(put, Some(SESSION_TOOL_RESULT_CACHE_DEFAULT_TTL))
    }

    pub fn put_text_entry_with_ttl(
        &self,
        put: SessionToolResultCachePut,
        ttl: Option<Duration>,
    ) -> SessionToolResultCacheMetadata {
        let SessionToolResultCachePut {
            session_id,
            item_id,
            lifecycle_path,
            turn_alias,
            body_alias,
            body_kind,
            raw_turn_id,
            raw_tool_call_id,
            tool_name,
            text,
            original_bytes,
        } = put;
        self.put_text_with_metadata_and_ttl(
            session_id,
            item_id,
            lifecycle_path,
            turn_alias,
            body_alias,
            body_kind,
            raw_turn_id,
            raw_tool_call_id,
            tool_name,
            text,
            original_bytes,
            ttl,
        )
    }

    pub fn put_text_with_ttl(
        &self,
        session_id: impl Into<String>,
        item_id: impl Into<String>,
        text: impl Into<String>,
        original_bytes: usize,
        ttl: Option<Duration>,
    ) -> SessionToolResultCacheMetadata {
        let session_id = session_id.into();
        let item_id = item_id.into();
        let (turn_alias, body_alias) = readable_aliases_from_item_id(&item_id);
        let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
        self.put_text_with_metadata_and_ttl(
            session_id,
            item_id,
            lifecycle_path,
            turn_alias,
            body_alias,
            "tool_result".to_string(),
            String::new(),
            String::new(),
            String::new(),
            text,
            original_bytes,
            ttl,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn put_text_with_metadata_and_ttl(
        &self,
        session_id: String,
        item_id: String,
        lifecycle_path: String,
        turn_alias: String,
        body_alias: String,
        body_kind: String,
        raw_turn_id: String,
        raw_tool_call_id: String,
        tool_name: String,
        text: impl Into<String>,
        original_bytes: usize,
        ttl: Option<Duration>,
    ) -> SessionToolResultCacheMetadata {
        let text = text.into();
        let now_ms = Utc::now().timestamp_millis();
        let ttl_ms = ttl.map(|ttl| ttl.as_millis().min(i64::MAX as u128) as i64);
        let metadata = SessionToolResultCacheMetadata {
            session_id: session_id.clone(),
            item_id: item_id.clone(),
            turn_alias,
            body_alias,
            body_kind,
            lifecycle_path,
            raw_turn_id,
            raw_tool_call_id,
            tool_name,
            original_bytes,
            stored_bytes: text.len(),
            created_at_ms: now_ms,
            expires_at_ms: ttl_ms.map(|ttl_ms| now_ms.saturating_add(ttl_ms)),
        };
        let entry = SessionToolResultCacheEntry {
            metadata: metadata.clone(),
            text: Arc::from(text),
            expires_at: ttl.map(|ttl| Instant::now() + ttl),
        };
        let key = SessionToolResultCacheKey {
            session_id,
            item_id,
        };
        self.inner.write().unwrap().insert(key, entry);
        metadata
    }

    pub fn read_text(&self, session_id: &str, item_id: &str) -> SessionToolResultCacheRead {
        let key = SessionToolResultCacheKey {
            session_id: session_id.to_string(),
            item_id: item_id.to_string(),
        };

        {
            let cache = self.inner.read().unwrap();
            if let Some(entry) = cache.get(&key) {
                if entry.is_expired() {
                    return SessionToolResultCacheRead::Unavailable(status_for(
                        SessionToolResultCacheStatusKind::Expired,
                        session_id,
                        item_id,
                    ));
                }
                return SessionToolResultCacheRead::Available {
                    metadata: entry.metadata.clone(),
                    text: entry.text.to_string(),
                };
            }
        }

        SessionToolResultCacheRead::Unavailable(status_for(
            SessionToolResultCacheStatusKind::Missing,
            session_id,
            item_id,
        ))
    }

    pub fn read_text_or_status_message(&self, session_id: &str, item_id: &str) -> String {
        match self.read_text(session_id, item_id) {
            SessionToolResultCacheRead::Available { text, .. } => text,
            SessionToolResultCacheRead::Unavailable(status) => status.message,
        }
    }

    pub fn remove_session(&self, session_id: &str) {
        self.inner
            .write()
            .unwrap()
            .retain(|key, _| key.session_id != session_id);
    }

    pub fn remove_expired(&self) -> usize {
        let mut removed = 0;
        self.inner.write().unwrap().retain(|_, entry| {
            let keep = !entry.is_expired();
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }
}

impl SessionToolResultCacheEntry {
    fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires_at| Instant::now() >= expires_at)
    }
}

pub fn readable_aliases_from_item_id(item_id: &str) -> (String, String) {
    item_id
        .split_once(':')
        .map(|(turn_alias, body_alias)| (turn_alias.to_string(), body_alias.to_string()))
        .unwrap_or_else(|| ("turn_unknown".to_string(), item_id.to_string()))
}

pub fn lifecycle_path_for_tool_result(turn_alias: &str, body_alias: &str) -> String {
    format!("lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt")
}

fn status_for(
    status: SessionToolResultCacheStatusKind,
    session_id: &str,
    item_id: &str,
) -> SessionToolResultCacheStatus {
    let (turn_alias, body_alias) = readable_aliases_from_item_id(item_id);
    let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
    let status_text = match status {
        SessionToolResultCacheStatusKind::Missing => "missing",
        SessionToolResultCacheStatusKind::Expired => "expired",
    };
    SessionToolResultCacheStatus {
        status,
        session_id: session_id.to_string(),
        item_id: item_id.to_string(),
        lifecycle_path: lifecycle_path.clone(),
        message: format!(
            "[tool result cache {status_text}]\nlifecycle_path: {lifecycle_path}\nitem_id: {item_id}\nThe original tool result is not available from the session cache."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trips_text_and_metadata() {
        let cache = SessionToolResultCache::default();
        let metadata = cache.put_text("session-1", "turn_001:tool_001", "large result", 12);

        assert_eq!(
            metadata.lifecycle_path,
            "lifecycle://session/tool-results/turn_001/tool_001/result.txt"
        );
        assert_eq!(metadata.original_bytes, 12);
        assert_eq!(metadata.stored_bytes, 12);

        let read = cache.read_text("session-1", "turn_001:tool_001");
        match read {
            SessionToolResultCacheRead::Available { metadata, text } => {
                assert_eq!(metadata.item_id, "turn_001:tool_001");
                assert_eq!(text, "large result");
            }
            SessionToolResultCacheRead::Unavailable(status) => {
                panic!("unexpected cache status: {status:?}")
            }
        }
    }

    #[test]
    fn missing_and_expired_reads_return_bounded_status() {
        let cache = SessionToolResultCache::default();

        let missing = cache.read_text("session-1", "missing");
        match missing {
            SessionToolResultCacheRead::Unavailable(status) => {
                assert_eq!(status.status, SessionToolResultCacheStatusKind::Missing);
                assert!(!status.message.contains("large result"));
            }
            SessionToolResultCacheRead::Available { .. } => panic!("expected missing status"),
        }

        cache.put_text_with_ttl(
            "session-1",
            "expired",
            "large result",
            12,
            Some(Duration::from_millis(0)),
        );
        let expired = cache.read_text("session-1", "expired");
        match expired {
            SessionToolResultCacheRead::Unavailable(status) => {
                assert_eq!(status.status, SessionToolResultCacheStatusKind::Expired);
                assert!(!status.message.contains("large result"));
            }
            SessionToolResultCacheRead::Available { .. } => panic!("expected expired status"),
        }
    }
}
