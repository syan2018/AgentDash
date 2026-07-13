use std::fs;
use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

const PINNED_MAIN_MAPPER: &str = "../../../AgentDash-main-reference/crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs";
const PINNED_MAIN_CODEX_BRIDGE: &str =
    "../../../AgentDash-main-reference/crates/agentdash-executor/src/connectors/codex_bridge.rs";
const PINNED_MAIN_HUB_SUPPORT: &str = "../../../AgentDash-main-reference/crates/agentdash-application-runtime-session/src/session/hub_support.rs";
const PINNED_MAIN_LAUNCH_COMMIT: &str = "../../../AgentDash-main-reference/crates/agentdash-application-runtime-session/src/session/launch/commit.rs";
const PINNED_MAIN_RELAY_HANDLER: &str =
    "../../../AgentDash-main-reference/crates/agentdash-api/src/relay/ws_handler.rs";
const PINNED_MAIN_WORKSPACE_SURFACE: &str = "../../../AgentDash-main-reference/crates/agentdash-workspace-module/src/workspace_module/surface.rs";
const PINNED_MAIN_HOOK_TRACE: &str =
    "../../../AgentDash-main-reference/crates/agentdash-spi/src/hooks/trace.rs";
const PINNED_MAIN_SESSION_EVENTING: &str = "../../../AgentDash-main-reference/crates/agentdash-application-runtime-session/src/session/eventing.rs";
const PINNED_MAIN_ROOT: &str = "../../../AgentDash-main-reference";
const PINNED_MAIN_COMMIT: &str = "957fa9d60ea3d67efa1bb278fe5b376cf0c34598";
const PINNED_MAIN_MAPPER_PATH: &str =
    "crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs";
const PINNED_MAIN_MAPPER_BLOB: &str = "ec0cfdf485a231090ba538c546c9c681d6ece1fb";
const PINNED_MAIN_MAPPER_SHA256: &str =
    "d2e1cea154e40e8f66aa8e5ec36ef0cd57ebee78332f157a22c639a4db4bbb05";
const PINNED_MAIN_MAPPER_CAPTURE_SHA256: &str =
    "3be97477e40ae0d13c79b6e6959365bbd0192f74b5891fca6e7b90a792163512";
const PINNED_MAIN_CODEX_PATH: &str = "crates/agentdash-executor/src/connectors/codex_bridge.rs";
const PINNED_MAIN_CODEX_BLOB: &str = "dabd32d2afa7a9276c762c02ab356c33014d004e";
const PINNED_MAIN_CODEX_SHA256: &str =
    "454903440e33240cd6462ae663517a9d11a336ec04ca3be08285b8fa1c99f902";
const PINNED_MAIN_CODEX_CAPTURE_SHA256: &str =
    "1049aa30c1bd5f617cff1ecbe95a503b186a5c32220e251e2ba6ba38cf8bb880";
const PINNED_MAIN_HUB_SUPPORT_PATH: &str =
    "crates/agentdash-application-runtime-session/src/session/hub_support.rs";
const PINNED_MAIN_HUB_SUPPORT_BLOB: &str = "8ecdf70d9783574d9f5fb30e6852d71519423d52";
const PINNED_MAIN_HUB_SUPPORT_SHA256: &str =
    "0e451535a2d02f9185db5e75e9b7cca0dbf87e3a58ba18c9ba0358079adacd76";
const PINNED_MAIN_TERMINAL_CAPTURE_SHA256: &str =
    "9670ff694a9ac4629060d7a752dd4c7f5ebfa6264b5f4ffd11c70c9eeb14744d";
const PINNED_MAIN_INPUT_CAPTURE_SHA256: &str =
    "17c56839468de4c7941390a6243581d905d198433ee54ba87781657efabae75c";
const PINNED_MAIN_LAUNCH_COMMIT_PATH: &str =
    "crates/agentdash-application-runtime-session/src/session/launch/commit.rs";
const PINNED_MAIN_LAUNCH_COMMIT_BLOB: &str = "d454dd2980a8f7eb99a25cb424c7c46d0444bbda";
const PINNED_MAIN_LAUNCH_COMMIT_SHA256: &str =
    "dfc4e70d3d33ca7c79202b6181608776febb996eff5ada7375f3d9678f239f95";
const PINNED_MAIN_DELIVERY_CAPTURE_SHA256: &str =
    "d8dfc05d5916f366375c39c85ff3e96d2527eefc26d7fe63777681ed15aa06bd";
const PINNED_MAIN_RELAY_HANDLER_PATH: &str = "crates/agentdash-api/src/relay/ws_handler.rs";
const PINNED_MAIN_RELAY_HANDLER_BLOB: &str = "08a279de204a2af19d3b8d5a7fc3e5d01c0c671b";
const PINNED_MAIN_RELAY_HANDLER_SHA256: &str =
    "bbc43e20680aa85853facbf27e298e2def10dc87a9c9e1f026f9c85151503fb4";
const PINNED_MAIN_PTY_CAPTURE_SHA256: &str =
    "fd8b65720f433371c1cdf45b61d9792b57f04d4c23baad29c48343238c0f3b72";
const PINNED_MAIN_WORKSPACE_SURFACE_PATH: &str =
    "crates/agentdash-workspace-module/src/workspace_module/surface.rs";
const PINNED_MAIN_WORKSPACE_SURFACE_BLOB: &str = "4e4a863f7a0517d0eb2a1e2ba17cc1eec941d6b3";
const PINNED_MAIN_WORKSPACE_SURFACE_SHA256: &str =
    "6f22eb9671788160a72a08e73b5636b5222fe2c2c6e7b9def75468f137a4031a";
const PINNED_MAIN_CONTROL_CAPTURE_SHA256: &str =
    "a6fa8b1646dce1aabcf734bebcd18bf1c69dfd476e74b1f8c1ad8b97b24a1ec2";
const PINNED_MAIN_HOOK_TRACE_PATH: &str = "crates/agentdash-spi/src/hooks/trace.rs";
const PINNED_MAIN_HOOK_TRACE_BLOB: &str = "1e83f11206fb99aa8ac0c2ddaf5cb287c8b42083";
const PINNED_MAIN_HOOK_TRACE_SHA256: &str =
    "3cecb368569b9cd73479d28acbd9e1fc0da5859b56272689e8ebe0b67ed258fb";
const PINNED_MAIN_HOOK_TRACE_CAPTURE_SHA256: &str =
    "98d92572f219b2891f2ebd688c0105fd2922ab770f7b875602e4405d227d99c3";
const PINNED_MAIN_SESSION_EVENTING_PATH: &str =
    "crates/agentdash-application-runtime-session/src/session/eventing.rs";
const PINNED_MAIN_SESSION_EVENTING_BLOB: &str = "682211e5efe476827c8d75b5f98fcef2cb80b8fc";
const PINNED_MAIN_SESSION_EVENTING_SHA256: &str =
    "834d5ae0c8d929e10081ee59f387c2287a3ec03e246d88c097e87e4b1ebf3e63";
const PINNED_MAIN_REWIND_CAPTURE_SHA256: &str =
    "be82213a9b1378459dcf773e980170a8866bbb6233c30dc38338a600cd307f7f";

fn git_output(args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(PINNED_MAIN_ROOT)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run pinned Main git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "pinned Main git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("pinned Main git output must be UTF-8")
        .trim()
        .to_string()
}

fn main() {
    if std::env::var_os("CARGO_FEATURE_PINNED_MAIN_CAPTURE").is_none() {
        return;
    }
    assert_eq!(
        git_output(&["rev-parse", "HEAD"]),
        PINNED_MAIN_COMMIT,
        "pinned Main reference HEAD drifted"
    );
    assert!(
        git_output(&["status", "--porcelain"]).is_empty(),
        "pinned Main reference must remain clean"
    );
    println!("cargo:rerun-if-changed={PINNED_MAIN_MAPPER}");
    let source = fs::read_to_string(PINNED_MAIN_MAPPER).expect("read pinned main stream mapper");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_MAPPER_PATH}"),
        ]),
        PINNED_MAIN_MAPPER_BLOB,
        "pinned Main Native mapper blob drifted"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        PINNED_MAIN_MAPPER_SHA256,
        "pinned Main Native mapper bytes drifted"
    );
    let transforms = [
        (
            "            codex_error_info,\n",
            "            codex_error_info: Some(codex_error_info),\n",
        ),
        (
            "            additional_details,\n",
            "            additional_details: Some(additional_details),\n",
        ),
        (
            "                    http_status_code: error.http_status,\n",
            "                    http_status_code: Some(error.http_status),\n",
        ),
    ];
    let mut compatible = source;
    for (old, replacement) in transforms {
        assert_eq!(
            compatible.matches(old).count(),
            1,
            "pinned mapper compatibility transform must match exactly once: {old:?}"
        );
        compatible = compatible.replacen(old, replacement, 1);
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(compatible.as_bytes())),
        PINNED_MAIN_MAPPER_CAPTURE_SHA256,
        "pinned Main Native mapper compatibility overlay drifted"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_stream_mapper.rs");
    fs::write(output, compatible).expect("write compatible pinned main mapper");

    println!("cargo:rerun-if-changed={PINNED_MAIN_CODEX_BRIDGE}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_CODEX_PATH}"),
        ]),
        PINNED_MAIN_CODEX_BLOB,
        "pinned Main Codex source blob drifted"
    );
    let codex_source =
        fs::read_to_string(PINNED_MAIN_CODEX_BRIDGE).expect("read pinned main Codex bridge");
    assert_eq!(
        format!("{:x}", Sha256::digest(codex_source.as_bytes())),
        PINNED_MAIN_CODEX_SHA256,
        "pinned Main Codex source bytes drifted"
    );
    let helper_start = codex_source
        .find("fn make_envelope(")
        .expect("pinned Main Codex make_envelope helper");
    let helper_end = codex_source[helper_start..]
        .find("\nfn make_thread_source_title_envelope(")
        .map(|offset| helper_start + offset)
        .expect("pinned Main Codex title helper boundary");
    let mapper_start = codex_source
        .find("async fn handle_server_notification(")
        .expect("pinned Main Codex notification mapper");
    let mapper_end = codex_source[mapper_start..]
        .find("\n/// \u{5904}\u{7406} server \u{2192} client \u{8bf7}\u{6c42}")
        .map(|offset| mapper_start + offset)
        .expect("pinned Main Codex notification mapper boundary");
    let executable_capture = format!(
        "{}\n\n{}\n",
        &codex_source[helper_start..helper_end],
        &codex_source[mapper_start..mapper_end]
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(executable_capture.as_bytes())),
        PINNED_MAIN_CODEX_CAPTURE_SHA256,
        "pinned Main Codex capture boundaries drifted"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_codex_notification.rs");
    fs::write(output, executable_capture).expect("write pinned Main Codex notification mapper");

    println!("cargo:rerun-if-changed={PINNED_MAIN_HUB_SUPPORT}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_HUB_SUPPORT_PATH}"),
        ]),
        PINNED_MAIN_HUB_SUPPORT_BLOB,
        "pinned Main session hub source blob drifted"
    );
    let hub_source =
        fs::read_to_string(PINNED_MAIN_HUB_SUPPORT).expect("read pinned Main session hub source");
    assert_eq!(
        format!("{:x}", Sha256::digest(hub_source.as_bytes())),
        PINNED_MAIN_HUB_SUPPORT_SHA256,
        "pinned Main session hub source bytes drifted"
    );
    let terminal_function_start = hub_source
        .find("pub(super) fn build_turn_terminal_envelope_with_timing(")
        .expect("pinned Main terminal builder");
    let terminal_function_end = hub_source[terminal_function_start..]
        .find("\n/// \u{4ece} BackboneEnvelope")
        .map(|offset| terminal_function_start + offset)
        .expect("pinned Main terminal builder boundary");
    let timing_struct = hub_source
        .find("pub(super) struct TurnTiming")
        .expect("pinned Main turn timing");
    let timing_start = hub_source[..timing_struct]
        .rfind("#[derive(")
        .expect("pinned Main turn timing derive");
    let timing_end = hub_source[timing_start..]
        .find("\npub struct SessionEventSubscription")
        .map(|offset| timing_start + offset)
        .expect("pinned Main turn timing boundary");
    let terminal_kind = hub_source
        .find("pub enum TurnTerminalKind")
        .expect("pinned Main terminal kind");
    let terminal_kind_start = hub_source[..terminal_kind]
        .rfind("#[derive(")
        .expect("pinned Main terminal kind derive");
    let terminal_kind_end = hub_source[terminal_kind_start..]
        .find("\nimpl From<TurnTerminalKind>")
        .map(|offset| terminal_kind_start + offset)
        .expect("pinned Main terminal kind boundary");
    let terminal_capture = format!(
        "{}\n\n{}\n\n{}\n",
        &hub_source[timing_start..timing_end],
        &hub_source[terminal_kind_start..terminal_kind_end],
        &hub_source[terminal_function_start..terminal_function_end]
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(terminal_capture.as_bytes())),
        PINNED_MAIN_TERMINAL_CAPTURE_SHA256,
        "pinned Main terminal builder capture boundaries drifted"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_turn_terminal.rs");
    fs::write(output, terminal_capture).expect("write pinned Main terminal builder");

    let input_builder_start = hub_source
        .find("pub(super) fn build_user_input_submitted_envelope(")
        .expect("pinned Main user input builder");
    let turn_started_end = hub_source[input_builder_start..]
        .find("\npub(super) fn build_turn_terminal_notification_with_timing(")
        .map(|offset| input_builder_start + offset)
        .expect("pinned Main input and turn-start builder boundary");
    let raw_input_capture = format!("{}\n", &hub_source[input_builder_start..turn_started_end]);
    assert_eq!(
        format!("{:x}", Sha256::digest(raw_input_capture.as_bytes())),
        PINNED_MAIN_INPUT_CAPTURE_SHA256,
        "pinned Main input/turn-start capture boundaries drifted"
    );
    let input_capture = raw_input_capture
        .replace(
            "codex::TurnItemsView::NotLoaded",
            "agentdash_agent_protocol::generated::codex_v2::server_notification::TurnItemsView::NotLoaded",
        )
        .replace(
            "started_at: Some(started_at_ms.div_euclid(1000)),",
            "started_at: Some(Some(started_at_ms.div_euclid(1000))),",
        )
        .replace("error: None,", "error: Some(None),")
        .replace("completed_at: None,", "completed_at: Some(None),")
        .replace("duration_ms: None,", "duration_ms: Some(None),");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_input_turn.rs");
    fs::write(output, input_capture).expect("write pinned Main input/turn-start builders");

    println!("cargo:rerun-if-changed={PINNED_MAIN_LAUNCH_COMMIT}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_LAUNCH_COMMIT_PATH}"),
        ]),
        PINNED_MAIN_LAUNCH_COMMIT_BLOB,
        "pinned Main launch commit blob drifted"
    );
    let launch_commit_source = fs::read_to_string(PINNED_MAIN_LAUNCH_COMMIT)
        .expect("read pinned Main launch commit source");
    assert_eq!(
        format!("{:x}", Sha256::digest(launch_commit_source.as_bytes())),
        PINNED_MAIN_LAUNCH_COMMIT_SHA256,
        "pinned Main launch commit source bytes drifted"
    );
    let delivery_start = launch_commit_source
        .find("fn contains_project_subagent_notification_marker(")
        .expect("pinned Main delivery marker helper");
    let delivery_end = launch_commit_source[delivery_start..]
        .find("\nfn apply_turn_start_meta(")
        .map(|offset| delivery_start + offset)
        .expect("pinned Main delivery builder boundary");
    let delivery_capture = launch_commit_source[delivery_start..delivery_end].to_string();
    assert_eq!(
        format!("{:x}", Sha256::digest(delivery_capture.as_bytes())),
        PINNED_MAIN_DELIVERY_CAPTURE_SHA256,
        "pinned Main delivery capture boundaries drifted"
    );
    let generated = format!(
        "#[derive(Clone, Copy)]\nenum LaunchSource {{ HttpPrompt, LifecycleAgentUserMessage, HookAutoResume, CompanionDispatch, CompanionParentResume, SystemDelivery, WorkflowOrchestrator, RoutineExecutor, LocalRelayPrompt, ContextCompaction }}\n\n{}\n\npub fn capture(session_id: &str, turn_id: &str, source_kind: &str, text_prompt: &str) -> BackboneEvent {{\n    let launch_source = match source_kind {{\n        \"system\" => LaunchSource::SystemDelivery,\n        \"workflow\" => LaunchSource::WorkflowOrchestrator,\n        \"routine\" => LaunchSource::RoutineExecutor,\n        \"companion_marker\" => LaunchSource::CompanionDispatch,\n        other => panic!(\"unsupported pinned delivery source: {{other}}\"),\n    }};\n    let source = SourceInfo {{ connector_id: \"fixture-connector\".into(), connector_type: \"native\".into(), executor_id: None }};\n    build_system_delivery_envelope(session_id, &source, turn_id, launch_source, text_prompt).event\n}}\n",
        delivery_capture
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_delivery.rs");
    fs::write(output, generated).expect("write pinned Main delivery builder");

    println!("cargo:rerun-if-changed={PINNED_MAIN_RELAY_HANDLER}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_RELAY_HANDLER_PATH}"),
        ]),
        PINNED_MAIN_RELAY_HANDLER_BLOB,
        "pinned Main relay handler blob drifted"
    );
    let relay_source = fs::read_to_string(PINNED_MAIN_RELAY_HANDLER)
        .expect("read pinned Main relay handler source");
    assert_eq!(
        format!("{:x}", Sha256::digest(relay_source.as_bytes())),
        PINNED_MAIN_RELAY_HANDLER_SHA256,
        "pinned Main relay handler source bytes drifted"
    );
    let terminal_branch = relay_source
        .find("RelayMessage::EventTerminalOutput")
        .expect("pinned Main terminal output branch");
    let terminal_statement_start = relay_source[terminal_branch..]
        .find("                let envelope = agentdash_agent_protocol::BackboneEnvelope::new(")
        .map(|offset| terminal_branch + offset)
        .expect("pinned Main terminal output envelope");
    let terminal_statement_end = relay_source[terminal_statement_start..]
        .find("\n                if let Err(e)")
        .map(|offset| terminal_statement_start + offset)
        .expect("pinned Main terminal output envelope boundary");
    let pty_branch = relay_source[terminal_statement_end..]
        .find("RelayMessage::EventPtyTerminalStateChanged")
        .map(|offset| terminal_statement_end + offset)
        .expect("pinned Main PTY state branch");
    let state_statement_start = relay_source[pty_branch..]
        .find("            let state_str = match payload.state")
        .map(|offset| pty_branch + offset)
        .expect("pinned Main PTY state mapping");
    let state_statement_end = relay_source[state_statement_start..]
        .find("\n            state.services.terminal_registry.update_state")
        .map(|offset| state_statement_start + offset)
        .expect("pinned Main PTY state mapping boundary");
    let pty_statement_start = relay_source[state_statement_end..]
        .find("                let envelope = agentdash_agent_protocol::BackboneEnvelope::new(")
        .map(|offset| state_statement_end + offset)
        .expect("pinned Main PTY envelope");
    let pty_statement_end = relay_source[pty_statement_start..]
        .find("\n                if let Err(e)")
        .map(|offset| pty_statement_start + offset)
        .expect("pinned Main PTY envelope boundary");
    let helper_start = relay_source[pty_statement_end..]
        .find("fn terminal_output_event_data(")
        .map(|offset| pty_statement_end + offset)
        .expect("pinned Main terminal output helper");
    let helper_end = relay_source[helper_start..]
        .find("\nfn notify_backend_runtime_changed")
        .map(|offset| helper_start + offset)
        .expect("pinned Main terminal output helper boundary");
    let extracted = format!(
        "{}\n\n{}\n\n{}\n\n{}\n",
        &relay_source[helper_start..helper_end],
        &relay_source[terminal_statement_start..terminal_statement_end],
        &relay_source[state_statement_start..state_statement_end],
        &relay_source[pty_statement_start..pty_statement_end]
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(extracted.as_bytes())),
        PINNED_MAIN_PTY_CAPTURE_SHA256,
        "pinned Main terminal/PTY capture boundaries drifted"
    );
    let generated = format!(
        "{}\n\npub fn capture_terminal_output(payload: &agentdash_relay::TerminalOutputPayload) -> agentdash_agent_protocol::BackboneEvent {{\n    let terminal_id = &payload.terminal_id;\n    let source = agentdash_agent_protocol::SourceInfo {{ connector_id: \"platform\".into(), connector_type: \"terminal\".into(), executor_id: None }};\n    let session_id = \"session-pty-0001\".to_string();\n{}\n    envelope.event\n}}\n\npub fn capture_pty_state(payload: &agentdash_relay::PtyTerminalStateChangedPayload) -> agentdash_agent_protocol::BackboneEvent {{\n{}\n    let source = agentdash_agent_protocol::SourceInfo {{ connector_id: \"platform\".into(), connector_type: \"terminal\".into(), executor_id: None }};\n    let session_id = \"session-pty-0001\".to_string();\n{}\n    envelope.event\n}}\n",
        &relay_source[helper_start..helper_end],
        &relay_source[terminal_statement_start..terminal_statement_end],
        &relay_source[state_statement_start..state_statement_end],
        &relay_source[pty_statement_start..pty_statement_end]
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_terminal_pty.rs");
    fs::write(output, generated).expect("write pinned Main terminal/PTY capture");

    println!("cargo:rerun-if-changed={PINNED_MAIN_WORKSPACE_SURFACE}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_WORKSPACE_SURFACE_PATH}"),
        ]),
        PINNED_MAIN_WORKSPACE_SURFACE_BLOB,
        "pinned Main workspace surface blob drifted"
    );
    let workspace_surface_source = fs::read_to_string(PINNED_MAIN_WORKSPACE_SURFACE)
        .expect("read pinned Main workspace module surface source");
    assert_eq!(
        format!("{:x}", Sha256::digest(workspace_surface_source.as_bytes())),
        PINNED_MAIN_WORKSPACE_SURFACE_SHA256,
        "pinned Main workspace surface source bytes drifted"
    );
    let control_function_start = workspace_surface_source
        .find("async fn build_present_projection_notification(")
        .expect("pinned Main workspace presentation projection builder");
    let control_body_start = workspace_surface_source[control_function_start..]
        .find("    let source = SourceInfo {")
        .map(|offset| control_function_start + offset)
        .expect("pinned Main workspace presentation projection body");
    let control_body_end = workspace_surface_source[control_body_start..]
        .find("\n}\n\n#[cfg(test)]")
        .map(|offset| control_body_start + offset)
        .expect("pinned Main workspace presentation projection boundary");
    let mut control_body =
        workspace_surface_source[control_body_start..control_body_end].to_string();
    for (old, replacement) in [
        (
            "    Ok(BackboneEnvelope::new(",
            "    let envelope = BackboneEnvelope::new(",
        ),
        (
            "PlatformEvent::ControlPlaneProjectionChanged(\n            ControlPlaneProjectionChanged {",
            "PlatformEvent::ControlPlaneProjectionChanged(Box::new(\n            ControlPlaneProjectionChanged {",
        ),
        (
            "            },\n        )),",
            "            },\n        ))),",
        ),
        (
            "    .with_trace(TraceInfo {\n        turn_id: Some(turn_id.to_string()),\n        entry_index: None,\n    }))",
            "    .with_trace(TraceInfo {\n        turn_id: Some(turn_id.to_string()),\n        entry_index: None,\n    });\n    envelope.event",
        ),
    ] {
        assert_eq!(
            control_body.matches(old).count(),
            1,
            "pinned Main control projection compatibility transform must match once: {old:?}"
        );
        control_body = control_body.replacen(old, replacement, 1);
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(control_body.as_bytes())),
        PINNED_MAIN_CONTROL_CAPTURE_SHA256,
        "pinned Main control projection capture boundaries drifted"
    );
    let generated = format!(
        "struct Anchor {{ run_id: String, agent_id: String, launch_frame_id: String }}\nstruct Presentation {{ module_id: String, view_key: String, renderer_kind: String, presentation_uri: String, title: String }}\n\npub fn capture(payload: serde_json::Value) -> BackboneEvent {{\n    let delivery_runtime_session_id = \"presentation-session-control-0001\";\n    let turn_id = \"turn-control-0001\";\n    let anchor = Anchor {{ run_id: \"11111111-1111-1111-1111-111111111111\".into(), agent_id: \"22222222-2222-2222-2222-222222222222\".into(), launch_frame_id: \"frame-control-0001\".into() }};\n    let presentation = Presentation {{ module_id: \"module-dashboard\".into(), view_key: \"dashboard\".into(), renderer_kind: \"canvas\".into(), presentation_uri: \"agentdash://workspace-module/module-dashboard/dashboard\".into(), title: \"Dashboard\".into() }};\n{}\n}}\n",
        control_body
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_control_projection.rs");
    fs::write(output, generated).expect("write pinned Main control projection capture");

    println!("cargo:rerun-if-changed={PINNED_MAIN_HOOK_TRACE}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_HOOK_TRACE_PATH}"),
        ]),
        PINNED_MAIN_HOOK_TRACE_BLOB,
        "pinned Main hook trace blob drifted"
    );
    let hook_trace_source =
        fs::read_to_string(PINNED_MAIN_HOOK_TRACE).expect("read pinned Main hook trace source");
    assert_eq!(
        format!("{:x}", Sha256::digest(hook_trace_source.as_bytes())),
        PINNED_MAIN_HOOK_TRACE_SHA256,
        "pinned Main hook trace source bytes drifted"
    );
    let hook_capture_end = hook_trace_source
        .find("\n#[cfg(test)]")
        .expect("pinned Main hook trace production boundary");
    let mut hook_capture = hook_trace_source[..hook_capture_end].to_string();
    for (old, replacement) in [
        (
            "use crate::{HookTraceEntry, HookTraceTrigger as HookTrigger};",
            "use agentdash_spi::{HookTraceEntry, HookTraceTrigger as HookTrigger};",
        ),
        (
            "fn is_substantive_hook_diagnostic(item: &crate::HookDiagnosticEntry)",
            "fn is_substantive_hook_diagnostic(item: &agentdash_spi::HookDiagnosticEntry)",
        ),
    ] {
        assert_eq!(
            hook_capture.matches(old).count(),
            1,
            "pinned Main hook trace compatibility transform must match once: {old:?}"
        );
        hook_capture = hook_capture.replacen(old, replacement, 1);
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(hook_capture.as_bytes())),
        PINNED_MAIN_HOOK_TRACE_CAPTURE_SHA256,
        "pinned Main hook trace capture boundaries drifted"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_hook_trace.rs");
    fs::write(output, hook_capture).expect("write pinned Main hook trace capture");

    println!("cargo:rerun-if-changed={PINNED_MAIN_SESSION_EVENTING}");
    assert_eq!(
        git_output(&[
            "rev-parse",
            &format!("{PINNED_MAIN_COMMIT}:{PINNED_MAIN_SESSION_EVENTING_PATH}"),
        ]),
        PINNED_MAIN_SESSION_EVENTING_BLOB,
        "pinned Main session eventing blob drifted"
    );
    let eventing_source = fs::read_to_string(PINNED_MAIN_SESSION_EVENTING)
        .expect("read pinned Main session eventing source");
    assert_eq!(
        format!("{:x}", Sha256::digest(eventing_source.as_bytes())),
        PINNED_MAIN_SESSION_EVENTING_SHA256,
        "pinned Main session eventing source bytes drifted"
    );
    let bounded_start = eventing_source
        .find("const SESSION_REWOUND_MESSAGE_LIMIT")
        .expect("pinned Main rewind message boundary");
    let bounded_end = eventing_source[bounded_start..]
        .find("\nfn build_context_delivery_record_envelope(")
        .map(|offset| bounded_start + offset)
        .expect("pinned Main rewind message helper boundary");
    let reason_start = eventing_source
        .find("fn session_rewind_reason_from_str(")
        .expect("pinned Main rewind reason helper");
    let reason_end = eventing_source[reason_start..]
        .find("\nfn latest_stable_terminal_before(")
        .map(|offset| reason_start + offset)
        .expect("pinned Main rewind reason helper boundary");
    let writer_start = eventing_source
        .find("        let message = bounded_session_rewound_message(message);")
        .expect("pinned Main rewind writer body");
    let writer_end = eventing_source[writer_start..]
        .find("\n        self.persist_notification_inner(")
        .map(|offset| writer_start + offset)
        .expect("pinned Main rewind writer boundary");
    let writer_body = eventing_source[writer_start..writer_end]
        .strip_prefix("        ")
        .expect("pinned Main rewind writer indentation")
        .replace("\n        ", "\n");
    let rewind_capture = format!(
        "{}\n\n{}\n\n#[derive(Clone)]\nstruct StableTerminalBoundary {{ event_seq: u64, turn_id: String }}\n\npub fn capture(discarded_turn_id: &str, discarded_entry_index: Option<u32>, stable_event_seq: Option<u64>, stable_turn_id: Option<&str>, reason: &str, message: Option<String>) -> BackboneEvent {{\n    let session_id = \"session-terminal-0001\";\n    let source = SourceInfo {{ connector_id: \"application\".into(), connector_type: \"runtime\".into(), executor_id: None }};\n    let stable = stable_event_seq.map(|event_seq| StableTerminalBoundary {{ event_seq, turn_id: stable_turn_id.expect(\"stable turn id\").to_string() }});\n{}\n    envelope.event\n}}\n",
        &eventing_source[bounded_start..bounded_end],
        &eventing_source[reason_start..reason_end],
        writer_body
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(rewind_capture.as_bytes())),
        PINNED_MAIN_REWIND_CAPTURE_SHA256,
        "pinned Main rewind capture boundaries drifted"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("pinned_main_session_rewind.rs");
    fs::write(output, rewind_capture).expect("write pinned Main session rewind capture");
}
