use std::path::Path;

use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

use super::context_frame::{self, ContextFramePayload};

pub(crate) struct EnvironmentFrameInput<'a> {
    pub date_utc: &'a str,
    pub platform: &'a str,
    pub arch: &'a str,
    pub model_id: Option<&'a str>,
    pub executor: &'a str,
    pub working_directory: Option<&'a Path>,
}

pub(crate) fn build_environment_context_frame(
    input: &EnvironmentFrameInput<'_>,
) -> Option<ContextFrame> {
    let payload = EnvironmentContextFrame::from_input(input)?;
    Some(context_frame::build_context_frame(&payload))
}

#[derive(Debug, Clone)]
struct EnvironmentContextFrame {
    date_utc: String,
    platform: String,
    arch: String,
    model_id: Option<String>,
    executor: String,
    working_directory: Option<String>,
}

impl EnvironmentContextFrame {
    fn from_input(input: &EnvironmentFrameInput<'_>) -> Option<Self> {
        if input.date_utc.is_empty() {
            return None;
        }
        Some(Self {
            date_utc: input.date_utc.to_string(),
            platform: input.platform.to_string(),
            arch: input.arch.to_string(),
            model_id: input
                .model_id
                .filter(|s| !s.is_empty())
                .map(ToString::to_string),
            executor: input.executor.to_string(),
            working_directory: input
                .working_directory
                .map(|p| p.display().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}

impl ContextFramePayload for EnvironmentContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("environment-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "environment"
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
        vec![ContextFrameSection::Environment {
            title: "Environment".to_string(),
            summary: format!("{} {} | {}", self.platform, self.arch, self.date_utc),
            date: Some(self.date_utc.clone()),
            platform: Some(format!("{} {}", self.platform, self.arch)),
            model_id: self.model_id.clone(),
            executor: Some(self.executor.clone()),
            working_directory: self.working_directory.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        let mut lines = vec!["## Environment".to_string()];
        lines.push(format!("- Date: {} (UTC)", self.date_utc));
        lines.push(format!("- Platform: {} {}", self.platform, self.arch));
        if let Some(model_id) = &self.model_id {
            lines.push(format!("- Model: {model_id}"));
        }
        lines.push(format!("- Executor: {}", self.executor));
        if let Some(dir) = &self.working_directory {
            lines.push(format!("- Working directory: {dir}"));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn environment_frame_renders_all_fields() {
        let dir = PathBuf::from("/workspace/project");
        let frame = build_environment_context_frame(&EnvironmentFrameInput {
            date_utc: "2026-07-01",
            platform: "linux",
            arch: "x86_64",
            model_id: Some("claude-sonnet-4-20250514"),
            executor: "PI_AGENT",
            working_directory: Some(&dir),
        })
        .expect("environment frame");

        assert_eq!(frame.kind, "environment");
        assert_eq!(frame.delivery_channel, "connector_context");
        assert_eq!(frame.message_role, "system");
        assert!(frame.rendered_text.contains("## Environment"));
        assert!(frame.rendered_text.contains("Date: 2026-07-01 (UTC)"));
        assert!(frame.rendered_text.contains("Platform: linux x86_64"));
        assert!(
            frame
                .rendered_text
                .contains("Model: claude-sonnet-4-20250514")
        );
        assert!(frame.rendered_text.contains("Executor: PI_AGENT"));
        assert!(
            frame
                .rendered_text
                .contains("Working directory: /workspace/project")
        );
    }

    #[test]
    fn environment_frame_none_when_empty_date() {
        let frame = build_environment_context_frame(&EnvironmentFrameInput {
            date_utc: "",
            platform: "linux",
            arch: "x86_64",
            model_id: None,
            executor: "PI_AGENT",
            working_directory: None,
        });
        assert!(frame.is_none());
    }

    #[test]
    fn environment_frame_optional_fields() {
        let frame = build_environment_context_frame(&EnvironmentFrameInput {
            date_utc: "2026-07-01",
            platform: "windows",
            arch: "x86_64",
            model_id: None,
            executor: "PI_AGENT",
            working_directory: None,
        })
        .expect("environment frame");

        assert!(!frame.rendered_text.contains("Model:"));
        assert!(!frame.rendered_text.contains("Working directory:"));
    }
}
