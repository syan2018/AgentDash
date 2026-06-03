pub mod backbone;

// ─── 集中 re-export（保持外部 API 不变）───────────────────

pub use backbone::approval::ApprovalRequest;
pub use backbone::envelope::{BackboneEnvelope, SourceInfo, TraceInfo};
pub use backbone::event::BackboneEvent;
pub use backbone::item::{ItemCompletedNotification, ItemStartedNotification};
pub use backbone::platform::{
    HookTraceCompletion, HookTraceData, HookTraceDiagnostic, HookTraceInjection, HookTracePayload,
    HookTraceSeverity, HookTraceTrigger, PlatformEvent,
};
pub use backbone::usage::{
    ContextUsageSource, NormalizedContextUsage, ThreadTokenUsage,
    ThreadTokenUsageUpdatedNotification, TokenUsageBreakdown,
};
pub use backbone::user_input::{
    UserInputConversionError, UserInputSubmissionKind, UserInputSubmittedNotification,
    codex_user_input_to_text, content_block_to_codex_user_input,
    content_blocks_to_codex_user_input,
};

pub use codex_app_server_protocol;

pub use agentdash_agent_types::{AgentDashNativeThreadItem, AgentDashThreadItem, CodexThreadItem};

pub use agent_client_protocol::{ContentBlock, EmbeddedResourceResource, TextContent};

#[cfg(test)]
mod tests {
    use std::fs;

    use ts_rs::TS;

    use super::BackboneEnvelope;

    #[test]
    fn backbone_types_export_to_explicit_temp_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");

        BackboneEnvelope::export_all_to(dir.path()).expect("export backbone protocol types");

        assert!(dir.path().join("BackboneEnvelope.ts").exists());
        assert!(dir.path().join("BackboneEvent.ts").exists());
        assert!(dir.path().join("PlatformEvent.ts").exists());

        let generated = fs::read_to_string(dir.path().join("BackboneEnvelope.ts"))
            .expect("read generated envelope type");
        assert!(generated.contains("export type BackboneEnvelope"));
    }
}
