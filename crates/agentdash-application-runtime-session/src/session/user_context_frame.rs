use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};
use agentdash_spi::platform::auth::AuthIdentity;

use super::context_frame::{self, ContextFramePayload};

/// user_context 帧输入。
///
/// 承载操作者（人类用户）身份信息，由 `AuthIdentity` 投影而来。
/// 当 session 无认证身份时不产出帧。
pub(crate) struct UserContextFrameInput<'a> {
    pub auth_identity: Option<&'a AuthIdentity>,
}

pub(crate) fn build_user_context_frame(input: &UserContextFrameInput<'_>) -> Option<ContextFrame> {
    let payload = UserContextFrame::from_input(input)?;
    Some(context_frame::build_context_frame(&payload))
}

#[derive(Debug, Clone)]
struct UserContextFrame {
    user_id: String,
    display_name: Option<String>,
    email: Option<String>,
    groups: Vec<String>,
    provider: Option<String>,
    extra: serde_json::Value,
}

impl UserContextFrame {
    fn from_input(input: &UserContextFrameInput<'_>) -> Option<Self> {
        let identity = input.auth_identity?;

        // system routine 触发的身份不作为人类用户上下文投递
        if identity.user_id.starts_with("system:") {
            return None;
        }

        Some(Self {
            user_id: identity.user_id.clone(),
            display_name: identity.display_name.clone(),
            email: identity.email.clone(),
            groups: identity
                .groups
                .iter()
                .map(|g| g.display_name.as_deref().unwrap_or(&g.group_id).to_string())
                .collect(),
            provider: identity.provider.clone(),
            extra: identity.extra.clone(),
        })
    }
}

impl ContextFramePayload for UserContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("user_context-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "user_context"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "prepared_for_connector".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "connector_context"
    }

    fn message_role(&self) -> &'static str {
        "system"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::UserContext {
            title: "User Context".to_string(),
            summary: "操作者身份信息（人类用户）。".to_string(),
            user_id: Some(self.user_id.clone()),
            display_name: self.display_name.clone(),
            email: self.email.clone(),
            groups: self.groups.clone(),
            provider: self.provider.clone(),
            extra: self.extra.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        let mut lines = vec!["## User Context".to_string()];
        if let Some(name) = &self.display_name {
            lines.push(format!("- Name: {name}"));
        }
        lines.push(format!("- User ID: {}", self.user_id));
        if let Some(email) = &self.email {
            lines.push(format!("- Email: {email}"));
        }
        if !self.groups.is_empty() {
            lines.push(format!("- Groups: {}", self.groups.join(", ")));
        }
        if let Some(provider) = &self.provider {
            lines.push(format!("- Provider: {provider}"));
        }
        if !self.extra.is_null()
            && let Some(obj) = self.extra.as_object()
        {
            for (key, value) in obj {
                let val_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                lines.push(format!("- {key}: {val_str}"));
            }
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use agentdash_spi::hooks::ContextFrameSection;
    use agentdash_spi::platform::auth::{AuthGroup, AuthIdentity, AuthMode};

    use super::{UserContextFrameInput, build_user_context_frame};

    fn sample_identity() -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "u-12345".to_string(),
            subject: "sub-12345".to_string(),
            display_name: Some("Zhang San".to_string()),
            email: Some("zhangsan@example.com".to_string()),
            avatar_url: None,
            groups: vec![
                AuthGroup {
                    group_id: "backend-team".to_string(),
                    display_name: Some("Backend Team".to_string()),
                },
                AuthGroup {
                    group_id: "admin".to_string(),
                    display_name: None,
                },
            ],
            is_admin: false,
            provider: Some("oidc".to_string()),
            extra: serde_json::json!({}),
        }
    }

    #[test]
    fn build_frame_from_valid_identity() {
        let identity = sample_identity();
        let frame = build_user_context_frame(&UserContextFrameInput {
            auth_identity: Some(&identity),
        })
        .expect("should produce frame");

        assert_eq!(frame.kind, "user_context");
        assert_eq!(frame.delivery_channel, "connector_context");
        assert_eq!(frame.message_role, "system");
        assert!(frame.rendered_text.contains("- Name: Zhang San"));
        assert!(
            frame
                .rendered_text
                .contains("- Email: zhangsan@example.com")
        );
        assert!(frame.rendered_text.contains("Backend Team, admin"));
    }

    #[test]
    fn no_frame_when_no_identity() {
        let frame = build_user_context_frame(&UserContextFrameInput {
            auth_identity: None,
        });
        assert!(frame.is_none());
    }

    #[test]
    fn no_frame_for_system_routine() {
        let identity = AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "system:routine:cron-daily".to_string(),
            subject: "system:routine:cron-daily".to_string(),
            display_name: Some("System Routine".to_string()),
            email: None,
            avatar_url: None,
            groups: vec![],
            is_admin: false,
            provider: Some("system.routine".to_string()),
            extra: serde_json::Value::Null,
        };
        let frame = build_user_context_frame(&UserContextFrameInput {
            auth_identity: Some(&identity),
        });
        assert!(frame.is_none());
    }

    #[test]
    fn section_carries_structured_fields() {
        let identity = sample_identity();
        let frame = build_user_context_frame(&UserContextFrameInput {
            auth_identity: Some(&identity),
        })
        .unwrap();

        let Some(ContextFrameSection::UserContext {
            user_id,
            display_name,
            email,
            groups,
            ..
        }) = frame.sections.first()
        else {
            panic!("expected UserContext section");
        };
        assert_eq!(user_id.as_deref(), Some("u-12345"));
        assert_eq!(display_name.as_deref(), Some("Zhang San"));
        assert_eq!(email.as_deref(), Some("zhangsan@example.com"));
        assert_eq!(groups, &["Backend Team", "admin"]);
    }
}
