use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

use crate::session::context_frame::{self, ContextFramePayload};

#[derive(Debug, Clone)]
struct AutoResumeFrame {
    reason: String,
    prompt: String,
}

impl AutoResumeFrame {
    fn new(reason: impl Into<String>, prompt: impl Into<String>) -> Option<Self> {
        let reason = reason.into();
        let prompt = prompt.into();
        (!prompt.trim().is_empty()).then_some(Self { reason, prompt })
    }
}

impl ContextFramePayload for AutoResumeFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("auto-resume-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "auto_resume"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "queued_as_user_prompt".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "user_prompt"
    }

    fn message_role(&self) -> &'static str {
        "user"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::AutoResume {
            title: "Auto Resume".to_string(),
            summary: "系统根据 Hook stop gate 自动发起续跑提示。".to_string(),
            reason: self.reason.clone(),
            prompt: self.prompt.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        self.prompt.clone()
    }
}

pub(crate) fn build_auto_resume_context_frame(
    reason: impl Into<String>,
    prompt: impl Into<String>,
) -> Option<ContextFrame> {
    let metadata = AutoResumeFrame::new(reason, prompt)?;
    Some(context_frame::build_context_frame(&metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resume_frame_records_prompt_verbatim() {
        let frame = build_auto_resume_context_frame("before_stop_continue", "继续处理")
            .expect("auto resume frame");

        assert_eq!(frame.kind, "auto_resume");
        assert_eq!(frame.delivery_channel, "user_prompt");
        assert_eq!(frame.rendered_text, "继续处理");
        assert!(matches!(
            frame.sections.first(),
            Some(ContextFrameSection::AutoResume { prompt, .. }) if prompt == "继续处理"
        ));
    }
}
