pub mod backbone;
pub mod compat;

// ─── 集中 re-export（保持外部 API 不变）───────────────────

pub use backbone::approval::ApprovalRequest;
pub use backbone::envelope::{BackboneEnvelope, SourceInfo, TraceInfo};
pub use backbone::event::BackboneEvent;
pub use backbone::platform::{
    HookTraceCompletion, HookTraceData, HookTraceDiagnostic, HookTraceInjection, HookTracePayload,
    HookTraceSeverity, HookTraceTrigger, PlatformEvent,
};

pub use compat::{envelope_to_session_notification, session_notification_to_envelope};

pub use codex_app_server_protocol;

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
