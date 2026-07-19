use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use codex_app_server_protocol::{
    GenerateTsOptions, generate_json_with_experimental, generate_ts_with_options,
};
use schemars08::schema::RootSchema;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use typify::{TypeSpace, TypeSpaceSettings};

#[allow(dead_code, clippy::all)]
#[path = "../vendor_generated/codex_v2.rs"]
mod owned_codex_v2;

const CODEX_VERSION: &str = "0.144.1";
const CODEX_TAG: &str = "rust-v0.144.1";
const CODEX_COMMIT: &str = "44918ea10c0f99151c6710411b4322c2f5c96bea";
const TYPIFY_VERSION: &str = "0.7.0";
const GENERATOR_REVISION: &str = "agentdash-integration-codex-private-vendor-generator-v1";
const PROJECTION_REVISION: &str = "codex-v2-to-agent-service-presentation-v1";
const FLAT_ROOTS: &[&str] = &["ThreadItem", "ServerNotification"];
const REQUEST_ROOTS: &[&str] = &[
    "CommandExecutionRequestApprovalParams",
    "FileChangeRequestApprovalParams",
    "PermissionsRequestApprovalParams",
    "ToolRequestUserInputParams",
    "DynamicToolCallParams",
    "McpServerElicitationRequestParams",
];
const MCP_OVERRIDE_ID: &str = "mcp_elicitation_distribute_root_intersection_v1";
const NULLABLE_OVERLAY_ID: &str = "owned_optional_nullable_fields_v1";
const RUST_SERIALIZATION_OVERLAY_ID: &str = "owned_default_vec_explicit_empty_array_v1";
const RUST_DEFAULT_VEC_SERIALIZATION_PATHS: &[(&str, &str, &str)] = &[
    (
        "AppInfo.pluginDisplayNames",
        "/definitions/AppInfo/properties/pluginDisplayNames",
        "app-server-protocol/src/protocol/v2/apps.rs",
    ),
    (
        "SandboxPolicy.WorkspaceWrite.writableRoots",
        "/definitions/SandboxPolicy/oneOf/3/properties/writableRoots",
        "app-server-protocol/src/protocol/v2/permissions.rs",
    ),
    (
        "ThreadItem.Reasoning.content",
        "/definitions/ThreadItem/oneOf/4/properties/content",
        "app-server-protocol/src/protocol/v2/item.rs",
    ),
    (
        "ThreadItem.Reasoning.summary",
        "/definitions/ThreadItem/oneOf/4/properties/summary",
        "app-server-protocol/src/protocol/v2/item.rs",
    ),
    (
        "UserInput.Text.text_elements",
        "/definitions/UserInput/oneOf/0/properties/text_elements",
        "app-server-protocol/src/protocol/v2/turn.rs",
    ),
];
const SESSION_NOTIFICATION_ROOTS: &[&str] = &[
    "AgentMessageDeltaNotification",
    "ReasoningTextDeltaNotification",
    "ReasoningSummaryTextDeltaNotification",
    "ItemGuardianApprovalReviewStartedNotification",
    "ItemGuardianApprovalReviewCompletedNotification",
    "CommandExecutionOutputDeltaNotification",
    "FileChangeOutputDeltaNotification",
    "McpToolCallProgressNotification",
    "TerminalInteractionNotification",
    "FileChangePatchUpdatedNotification",
    "ServerRequestResolvedNotification",
    "TurnStartedNotification",
    "TurnCompletedNotification",
    "TurnDiffUpdatedNotification",
    "TurnPlanUpdatedNotification",
    "PlanDeltaNotification",
    "ReasoningSummaryPartAddedNotification",
    "ThreadStatusChangedNotification",
    "ContextCompactedNotification",
    "ModelReroutedNotification",
    "ModelVerificationNotification",
    "TurnModerationMetadataNotification",
    "ModelSafetyBufferingUpdatedNotification",
    "WarningNotification",
    "GuardianWarningNotification",
    "DeprecationNoticeNotification",
    "ConfigWarningNotification",
    "ErrorNotification",
];
const SESSION_NOTIFICATION_NULLABLE_PATHS: &[&str] = &[
    "/definitions/TurnError/properties/additionalDetails",
    "/definitions/TurnError/properties/codexErrorInfo",
    "/definitions/GuardianApprovalReviewAction/oneOf/4/properties/connectorId",
    "/definitions/GuardianApprovalReviewAction/oneOf/4/properties/connectorName",
    "/definitions/GuardianApprovalReviewAction/oneOf/4/properties/toolTitle",
    "/definitions/GuardianApprovalReviewAction/oneOf/5/properties/reason",
    "/definitions/RequestPermissionProfile/properties/fileSystem",
    "/definitions/RequestPermissionProfile/properties/network",
    "/definitions/AdditionalFileSystemPermissions/properties/read",
    "/definitions/AdditionalFileSystemPermissions/properties/write",
    "/definitions/FileSystemSpecialPath/oneOf/2/properties/subpath",
    "/definitions/FileSystemSpecialPath/oneOf/5/properties/subpath",
    "/definitions/AdditionalNetworkPermissions/properties/enabled",
    "/definitions/GuardianApprovalReview/properties/rationale",
    "/definitions/GuardianApprovalReview/properties/riskLevel",
    "/definitions/GuardianApprovalReview/properties/userAuthorization",
    "/definitions/ItemGuardianApprovalReviewStartedNotification/properties/targetItemId",
    "/definitions/ItemGuardianApprovalReviewCompletedNotification/properties/targetItemId",
    "/definitions/PatchChangeKind/oneOf/2/properties/move_path",
    "/definitions/ModelSafetyBufferingUpdatedNotification/properties/fasterModel",
    "/definitions/WarningNotification/properties/threadId",
    "/definitions/DeprecationNoticeNotification/properties/details",
    "/definitions/ConfigWarningNotification/properties/details",
    "/definitions/AdditionalFileSystemPermissions/properties/entries",
    "/definitions/AdditionalFileSystemPermissions/properties/globScanMaxDepth",
    "/definitions/ConfigWarningNotification/properties/path",
    "/definitions/ConfigWarningNotification/properties/range",
    "/definitions/Turn/properties/completedAt",
    "/definitions/Turn/properties/durationMs",
    "/definitions/Turn/properties/error",
    "/definitions/Turn/properties/startedAt",
    "/definitions/CodexErrorInfo/oneOf/1/properties/httpConnectionFailed/properties/httpStatusCode",
    "/definitions/CodexErrorInfo/oneOf/2/properties/responseStreamConnectionFailed/properties/httpStatusCode",
    "/definitions/CodexErrorInfo/oneOf/3/properties/responseStreamDisconnected/properties/httpStatusCode",
    "/definitions/CodexErrorInfo/oneOf/4/properties/responseTooManyFailedAttempts/properties/httpStatusCode",
    "/definitions/TextElement/properties/placeholder",
    "/definitions/UserInput/oneOf/1/properties/detail",
    "/definitions/UserInput/oneOf/2/properties/detail",
    "/definitions/CommandAction/oneOf/1/properties/path",
    "/definitions/CommandAction/oneOf/2/properties/path",
    "/definitions/CommandAction/oneOf/2/properties/query",
    "/definitions/McpToolCallAppContext/properties/actionName",
    "/definitions/McpToolCallAppContext/properties/appName",
    "/definitions/McpToolCallAppContext/properties/linkId",
    "/definitions/McpToolCallAppContext/properties/resourceUri",
    "/definitions/McpToolCallAppContext/properties/templateId",
    "/definitions/WebSearchAction/oneOf/0/properties/queries",
    "/definitions/WebSearchAction/oneOf/0/properties/query",
    "/definitions/WebSearchAction/oneOf/1/properties/url",
    "/definitions/WebSearchAction/oneOf/2/properties/pattern",
    "/definitions/WebSearchAction/oneOf/2/properties/url",
    "/definitions/TurnPlanUpdatedNotification/properties/explanation",
];
const REQUEST_EXPLICIT_NULL_PATHS: &[(&str, &str)] = &[
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/environmentId",
    ),
    ("FileChangeRequestApprovalParams", "/properties/grantRoot"),
    ("FileChangeRequestApprovalParams", "/properties/reason"),
    (
        "PermissionsRequestApprovalParams",
        "/properties/environmentId",
    ),
    ("PermissionsRequestApprovalParams", "/properties/reason"),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/RequestPermissionProfile/properties/fileSystem",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/RequestPermissionProfile/properties/network",
    ),
    ("ToolRequestUserInputParams", "/properties/autoResolutionMs"),
    (
        "ToolRequestUserInputParams",
        "/definitions/ToolRequestUserInputQuestion/properties/options",
    ),
    ("DynamicToolCallParams", "/properties/namespace"),
];
const REQUEST_OMITTED_NULL_PATHS: &[(&str, &str)] = &[
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/entries",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/globScanMaxDepth",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/read",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/write",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalNetworkPermissions/properties/enabled",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalPermissionProfile/properties/fileSystem",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/AdditionalPermissionProfile/properties/network",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/CommandAction/oneOf/1/properties/path",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/CommandAction/oneOf/2/properties/path",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/CommandAction/oneOf/2/properties/query",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/FileSystemSpecialPath/oneOf/2/properties/subpath",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/definitions/FileSystemSpecialPath/oneOf/5/properties/subpath",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/approvalId",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/command",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/commandActions",
    ),
    ("CommandExecutionRequestApprovalParams", "/properties/cwd"),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/networkApprovalContext",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/proposedExecpolicyAmendment",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/proposedNetworkPolicyAmendments",
    ),
    (
        "CommandExecutionRequestApprovalParams",
        "/properties/reason",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/entries",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/globScanMaxDepth",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/read",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/AdditionalFileSystemPermissions/properties/write",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/AdditionalNetworkPermissions/properties/enabled",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/FileSystemSpecialPath/oneOf/2/properties/subpath",
    ),
    (
        "PermissionsRequestApprovalParams",
        "/definitions/FileSystemSpecialPath/oneOf/5/properties/subpath",
    ),
];
const THREAD_ITEM_NULLABLE_PATHS: &[&str] = &[
    "UserMessage.clientId",
    "AgentMessage.memoryCitation",
    "AgentMessage.phase",
    "CommandExecution.aggregatedOutput",
    "CommandExecution.durationMs",
    "CommandExecution.exitCode",
    "CommandExecution.processId",
    "McpToolCall.appContext",
    "McpToolCall.durationMs",
    "McpToolCall.error",
    "McpToolCall.mcpAppResourceUri",
    "McpToolCall.pluginId",
    "McpToolCall.result",
    "DynamicToolCall.contentItems",
    "DynamicToolCall.durationMs",
    "DynamicToolCall.namespace",
    "DynamicToolCall.success",
    "CollabAgentToolCall.model",
    "CollabAgentToolCall.prompt",
    "CollabAgentToolCall.reasoningEffort",
    "WebSearch.action",
    "ImageGeneration.revisedPrompt",
    "ImageGeneration.savedPath",
];

fn main() -> Result<()> {
    let mode = env::args().nth(1).unwrap_or_else(|| "check".to_string());
    if mode != "write" && mode != "check" {
        bail!("usage: generate_codex_vendor_protocol [write|check]");
    }
    let root = workspace_root()?;
    let generated = generate(&root)?;
    let expected = generated.keys().cloned().collect::<BTreeSet<_>>();
    let mut drift = Vec::new();
    for (relative, bytes) in generated {
        let target = root.join(&relative);
        if mode == "write" {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
                fs::write(&target, bytes)?;
                println!("write {}", relative.display());
            }
        } else if fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
            drift.push(relative);
        }
    }
    for (directory, extension, exclusions) in [
        (
            "crates/agentdash-integration-codex/protocol-fixtures/schemas",
            "json",
            &[][..],
        ),
        (
            "crates/agentdash-integration-codex/src/vendor_generated",
            "rs",
            &["mod.rs"][..],
        ),
    ] {
        let managed = root.join(directory);
        if !managed.exists() {
            continue;
        }
        for existing in walk_files(&managed, extension)? {
            if existing
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| exclusions.contains(&name))
            {
                continue;
            }
            let relative = existing.strip_prefix(&root)?.to_path_buf();
            if !expected.contains(&relative) {
                if mode == "write" {
                    fs::remove_file(&existing)?;
                    println!("remove {}", relative.display());
                } else {
                    drift.push(PathBuf::from(format!("extra: {}", relative.display())));
                }
            }
        }
    }
    if !drift.is_empty() {
        bail!(
            "generated protocol drift:\n{}",
            drift
                .iter()
                .map(|path| format!("- {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    if mode == "check" {
        audit_owned_roundtrips()?;
    }
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Ok(manifest
        .parent()
        .and_then(Path::parent)
        .context("codegen crate must live under workspace/crates")?
        .to_path_buf())
}

fn generate(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let scratch = tempfile::tempdir()?;
    let json_dir = scratch.path().join("json");
    let ts_dir = scratch.path().join("typescript");
    generate_json_with_experimental(&json_dir, false)?;
    generate_ts_with_options(
        &ts_dir,
        None,
        GenerateTsOptions {
            generate_indices: true,
            ensure_headers: true,
            run_prettier: false,
            experimental_api: false,
        },
    )?;

    let flat_path = json_dir.join("codex_app_server_protocol.v2.schemas.json");
    let mut flat: serde_json::Value = serde_json::from_slice(&fs::read(&flat_path)?)?;
    canonicalize_json(&mut flat);
    let flat_bytes = format!("{}\n", serde_json::to_string_pretty(&flat)?).into_bytes();
    let discovered_nullable = thread_item_nullable_fields(&flat)?;
    let audited_nullable = THREAD_ITEM_NULLABLE_PATHS
        .iter()
        .map(|field| field.to_string())
        .collect::<BTreeSet<_>>();
    if discovered_nullable != audited_nullable {
        bail!(
            "ThreadItem nullable overlay drift; missing={:?}, extra={:?}",
            discovered_nullable
                .difference(&audited_nullable)
                .collect::<Vec<_>>(),
            audited_nullable
                .difference(&discovered_nullable)
                .collect::<Vec<_>>()
        );
    }
    let discovered_session_nullable = session_notification_nullable_paths(&flat)?;
    let mut audited_session_nullable = SESSION_NOTIFICATION_NULLABLE_PATHS
        .iter()
        .map(|path| (*path).to_string())
        .collect::<BTreeSet<_>>();
    for entry in nullable_schema_path_manifest(&flat)? {
        let path = entry["schema_path"]
            .as_str()
            .context("ThreadItem nullable manifest lost schema_path")?;
        audited_session_nullable.insert(
            path.strip_prefix('#')
                .context("ThreadItem nullable schema path lost fragment prefix")?
                .to_string(),
        );
    }
    if discovered_session_nullable != audited_session_nullable {
        bail!(
            "session notification nullable overlay drift; missing={:?}, extra={:?}",
            discovered_session_nullable
                .difference(&audited_session_nullable)
                .collect::<Vec<_>>(),
            audited_session_nullable
                .difference(&discovered_session_nullable)
                .collect::<Vec<_>>()
        );
    }
    let mcp_path = json_dir.join("McpServerElicitationRequestParams.json");
    let mut mcp_schema: serde_json::Value = serde_json::from_slice(&fs::read(&mcp_path)?)?;
    canonicalize_json(&mut mcp_schema);
    let mcp_bytes = format!("{}\n", serde_json::to_string_pretty(&mcp_schema)?).into_bytes();
    let mcp_nullable_schema_paths = mcp_nullable_schema_path_manifest(&mcp_schema)?;
    let mut modules = Vec::new();
    let mut root_schemas = BTreeMap::new();
    for root_name in FLAT_ROOTS {
        let selected = select_roots(&flat, &[*root_name])?;
        let selected_bytes = canonical_json_bytes(&selected)?;
        root_schemas.insert((*root_name).to_string(), selected_bytes);
        modules.push((snake(root_name), generate_rust(selected, root_name)?));
    }
    for root_name in REQUEST_ROOTS {
        let path = json_dir.join(format!("{root_name}.json"));
        let mut schema: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        if *root_name == "McpServerElicitationRequestParams" {
            normalize_mcp_elicitation_root(&mut schema)?;
        }
        canonicalize_json(&mut schema);
        root_schemas.insert((*root_name).to_string(), canonical_json_bytes(&schema)?);
        modules.push((snake(root_name), generate_rust(schema, root_name)?));
    }
    let rust = render_rust(modules)?;

    let mut output = BTreeMap::new();
    output.insert(
        PathBuf::from(
            "crates/agentdash-integration-codex/protocol-fixtures/schemas/codex-app-server-v2.schemas.json",
        ),
        flat_bytes.clone(),
    );
    output.insert(
        PathBuf::from(
            "crates/agentdash-integration-codex/protocol-fixtures/schemas/McpServerElicitationRequestParams.json",
        ),
        mcp_bytes.clone(),
    );
    output.insert(
        PathBuf::from("crates/agentdash-integration-codex/src/vendor_generated/codex_v2.rs"),
        rust.into_bytes(),
    );
    for (root_name, bytes) in &root_schemas {
        output.insert(
            PathBuf::from(
                "crates/agentdash-integration-codex/protocol-fixtures/schemas/codex-app-server-roots",
            )
                .join(format!("{root_name}.json")),
            bytes.clone(),
        );
    }
    let ts_files = collect_ts_closure(&ts_dir)?;
    let canonical_ts_dir = fs::canonicalize(&ts_dir)?;
    for source in ts_files {
        let relative = source.strip_prefix(&canonical_ts_dir)?;
        let mut bytes = fs::read(&source)?;
        apply_owned_ts_overlays(relative, &mut bytes)?;
        if String::from_utf8_lossy(&bytes).contains("bigint") {
            bail!(
                "generated private vendor TypeScript audit contains JSON-incompatible bigint: {}",
                relative.display()
            );
        }
    }
    let root_schema_sha256 = root_schemas
        .iter()
        .map(|(name, bytes)| (name.clone(), serde_json::Value::String(sha256(bytes))))
        .collect::<serde_json::Map<_, _>>();
    let nullable_schema_paths = nullable_schema_path_manifest(&flat)?;
    let server_notification_nullable_schema_paths =
        explicit_null_schema_path_manifest(&flat, SESSION_NOTIFICATION_NULLABLE_PATHS)?;
    let request_nullable_schema_paths = request_nullable_schema_path_manifest(&root_schemas)?;
    let request_omitted_nullable_schema_paths =
        request_omitted_nullable_schema_path_manifest(&root_schemas)?;
    let rust_serialization_overlay =
        default_vec_serialization_overlay_manifest(&flat, &root_schemas)?;
    let lock = serde_json::json!({
        "codex_crate_version": CODEX_VERSION,
        "codex_git_tag": CODEX_TAG,
        "codex_commit": CODEX_COMMIT,
        "experimental_api": false,
        "generator_revision": GENERATOR_REVISION,
        "canonical_projection_revision": PROJECTION_REVISION,
        "upstream_v2_schema_sha256": sha256(&flat_bytes),
        "mcp_elicitation_schema_sha256": sha256(&mcp_bytes),
        "root_types": FLAT_ROOTS.iter().chain(REQUEST_ROOTS).collect::<Vec<_>>(),
        "root_schema_sha256": root_schema_sha256,
        "typify_version": TYPIFY_VERSION,
        "schema_overrides": [{
            "id": MCP_OVERRIDE_ID,
            "schema": "McpServerElicitationRequestParams",
            "schema_path": "crates/agentdash-integration-codex/protocol-fixtures/schemas/McpServerElicitationRequestParams.json",
            "schema_sha256": sha256(&mcp_bytes),
            "reason": "typify cannot consume the upstream root intersection without distributing its base fields into each tagged branch"
        }],
        "owned_nullable_overlays": [{
            "id": NULLABLE_OVERLAY_ID,
            "codex_source": "app-server-protocol/src/protocol/v2/item.rs + mcp.rs@rust-v0.144.1",
            "thread_item_paths": THREAD_ITEM_NULLABLE_PATHS,
            "thread_item_schema_paths": nullable_schema_paths,
            "server_notification_schema_paths": server_notification_nullable_schema_paths,
            "server_notification_session_roots": SESSION_NOTIFICATION_ROOTS,
            "request_schema_paths": request_nullable_schema_paths,
            "request_omitted_schema_paths": request_omitted_nullable_schema_paths,
            "mcp_elicitation_fields": ["turnId", "_meta"],
            "mcp_elicitation_schema_paths": mcp_nullable_schema_paths,
            "wire_policy": "accept omitted or null; serialize canonical explicit null"
        }],
        "owned_rust_serialization_overlays": [{
            "id": RUST_SERIALIZATION_OVERLAY_ID,
            "codex_source_revision": "rust-v0.144.1",
            "paths": rust_serialization_overlay,
            "wire_policy": "accept omitted as default empty array; serialize canonical explicit empty array"
        }],
        "roundtrip_roots": FLAT_ROOTS.iter().chain(REQUEST_ROOTS).collect::<Vec<_>>(),
    });
    output.insert(
        PathBuf::from("crates/agentdash-integration-codex/codex-protocol-codegen.lock.json"),
        format!("{}\n", serde_json::to_string_pretty(&lock)?).into_bytes(),
    );
    for (path, bytes) in &output {
        if path.extension().and_then(|value| value.to_str()) == Some("ts")
            && String::from_utf8_lossy(bytes).contains("bigint")
        {
            bail!(
                "generated TypeScript contains JSON-incompatible bigint: {}",
                path.display()
            );
        }
    }
    let _ = root;
    Ok(output)
}

fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>> {
    let mut value = value.clone();
    canonicalize_json(&mut value);
    Ok(format!("{}\n", serde_json::to_string_pretty(&value)?).into_bytes())
}

fn default_vec_serialization_overlay_manifest(
    flat: &serde_json::Value,
    root_schemas: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<serde_json::Value>> {
    let mut discovered = BTreeSet::new();
    for root in FLAT_ROOTS {
        let schema: serde_json::Value = serde_json::from_slice(
            root_schemas
                .get(*root)
                .with_context(|| format!("default Vec serialization audit lost root {root}"))?,
        )?;
        collect_default_vec_schema_paths(&schema, "", &mut discovered);
    }
    let audited = RUST_DEFAULT_VEC_SERIALIZATION_PATHS
        .iter()
        .map(|(_, path, _)| (*path).to_string())
        .collect::<BTreeSet<_>>();
    if discovered != audited {
        bail!(
            "default Vec serialization overlay drift; missing={:?}, extra={:?}",
            discovered.difference(&audited).collect::<Vec<_>>(),
            audited.difference(&discovered).collect::<Vec<_>>()
        );
    }
    RUST_DEFAULT_VEC_SERIALIZATION_PATHS
        .iter()
        .map(|(qualified_path, schema_path, codex_source)| {
            let field = flat.pointer(schema_path).with_context(|| {
                format!("default Vec serialization overlay lost {qualified_path} at {schema_path}")
            })?;
            let (owner_path, field_name) = schema_path
                .rsplit_once("/properties/")
                .context("default Vec serialization path must point to a property")?;
            let owner = flat.pointer(owner_path).with_context(|| {
                format!("default Vec serialization overlay lost owner for {qualified_path}")
            })?;
            if owner["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|name| name == field_name))
            {
                bail!("default Vec serialization field became required: {qualified_path}");
            }
            let expected_variant_title = match *qualified_path {
                "SandboxPolicy.WorkspaceWrite.writableRoots" => Some("WorkspaceWriteSandboxPolicy"),
                "ThreadItem.Reasoning.content" | "ThreadItem.Reasoning.summary" => {
                    Some("ReasoningThreadItem")
                }
                "UserInput.Text.text_elements" => Some("TextUserInput"),
                "AppInfo.pluginDisplayNames" => None,
                _ => bail!("unreviewed default Vec serialization path: {qualified_path}"),
            };
            if expected_variant_title.is_some_and(|title| owner["title"] != title) {
                bail!("default Vec serialization variant drifted: {qualified_path}");
            }
            Ok(serde_json::json!({
                "qualified_path": qualified_path,
                "schema_path": format!("#{schema_path}"),
                "schema_sha256": sha256(&canonical_json_bytes(field)?),
                "codex_source": codex_source,
                "vendor_serde_attribute": "#[serde(default)]",
            }))
        })
        .collect()
}

fn collect_default_vec_schema_paths(
    value: &serde_json::Value,
    pointer: &str,
    output: &mut BTreeSet<String>,
) {
    match value {
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                collect_default_vec_schema_paths(child, &format!("{pointer}/{index}"), output);
            }
        }
        serde_json::Value::Object(object) => {
            if value["type"] == "array" && value["default"] == serde_json::json!([]) {
                output.insert(pointer.to_string());
            }
            for (key, child) in object {
                collect_default_vec_schema_paths(child, &format!("{pointer}/{key}"), output);
            }
        }
        _ => {}
    }
}

fn nullable_schema_path_manifest(flat: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
    let variants = flat["definitions"]["ThreadItem"]["oneOf"]
        .as_array()
        .context("ThreadItem schema lost oneOf variants")?;
    let mut output = Vec::new();
    for qualified in THREAD_ITEM_NULLABLE_PATHS {
        let (variant_name, field) = qualified
            .split_once('.')
            .context("nullable path must be variant-qualified")?;
        let title = format!("{variant_name}ThreadItem");
        let (variant_index, variant) = variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant["title"] == title)
            .with_context(|| format!("nullable overlay lost variant {variant_name}"))?;
        let schema = variant["properties"]
            .get(field)
            .with_context(|| format!("nullable overlay lost field {qualified}"))?;
        if !schema_is_nullable(schema) {
            bail!("nullable overlay path is no longer nullable: {qualified}");
        }
        output.push(serde_json::json!({
            "qualified_path": qualified,
            "schema_path": format!("#/definitions/ThreadItem/oneOf/{variant_index}/properties/{field}"),
            "schema_sha256": sha256(&canonical_json_bytes(schema)?),
        }));
    }
    Ok(output)
}

fn explicit_null_schema_path_manifest(
    schema_root: &serde_json::Value,
    paths: &[&str],
) -> Result<Vec<serde_json::Value>> {
    paths
        .iter()
        .map(|path| {
            let schema = schema_root
                .pointer(path)
                .with_context(|| format!("explicit-null overlay lost schema path {path}"))?;
            if !schema_is_nullable(schema) {
                bail!("explicit-null overlay path is no longer nullable: {path}");
            }
            Ok(serde_json::json!({
                "schema_path": format!("#{path}"),
                "schema_sha256": sha256(&canonical_json_bytes(schema)?),
            }))
        })
        .collect()
}

fn mcp_nullable_schema_path_manifest(schema: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
    let turn_id = schema["properties"]
        .get("turnId")
        .context("MCP elicitation schema lost turnId")?;
    if !schema_is_nullable(turn_id) {
        bail!("MCP elicitation turnId is no longer nullable");
    }
    let mut output = vec![serde_json::json!({
        "qualified_path": "McpServerElicitationRequestParams.turnId",
        "schema_path": "#/properties/turnId",
        "schema_sha256": sha256(&canonical_json_bytes(turn_id)?),
    })];
    let branches = schema["oneOf"]
        .as_array()
        .context("MCP elicitation schema lost oneOf")?;
    for (index, branch) in branches.iter().enumerate() {
        let meta = branch["properties"]
            .get("_meta")
            .with_context(|| format!("MCP elicitation branch {index} lost _meta"))?;
        output.push(serde_json::json!({
            "qualified_path": format!("McpServerElicitationRequestParams.oneOf[{index}]._meta"),
            "schema_path": format!("#/oneOf/{index}/properties/_meta"),
            "schema_sha256": sha256(&canonical_json_bytes(meta)?),
        }));
    }
    Ok(output)
}

fn request_nullable_schema_path_manifest(
    root_schemas: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<serde_json::Value>> {
    audit_request_nullable_paths(root_schemas)?;
    request_nullable_path_manifest(root_schemas, REQUEST_EXPLICIT_NULL_PATHS)
}

fn request_omitted_nullable_schema_path_manifest(
    root_schemas: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<serde_json::Value>> {
    request_nullable_path_manifest(root_schemas, REQUEST_OMITTED_NULL_PATHS)
}

fn request_nullable_path_manifest(
    root_schemas: &BTreeMap<String, Vec<u8>>,
    paths: &[(&str, &str)],
) -> Result<Vec<serde_json::Value>> {
    let mut output = Vec::new();
    for (root, pointer) in paths {
        let bytes = root_schemas
            .get(*root)
            .with_context(|| format!("nullable overlay lost root {root}"))?;
        let schema: serde_json::Value = serde_json::from_slice(bytes)?;
        let field = schema
            .pointer(pointer)
            .with_context(|| format!("nullable overlay lost {root}#{pointer}"))?;
        if !schema_is_nullable(field) {
            bail!("request nullable overlay path is no longer nullable: {root}#{pointer}");
        }
        output.push(serde_json::json!({
            "qualified_path": format!("{root}#{pointer}"),
            "schema_path": format!(
                "crates/agentdash-integration-codex/protocol-fixtures/schemas/codex-app-server-roots/{root}.json#{pointer}"
            ),
            "schema_sha256": sha256(&canonical_json_bytes(field)?),
        }));
    }
    Ok(output)
}

fn audit_request_nullable_paths(root_schemas: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let mut discovered = BTreeSet::new();
    for root in REQUEST_ROOTS
        .iter()
        .copied()
        .filter(|root| *root != "McpServerElicitationRequestParams")
    {
        let bytes = root_schemas
            .get(root)
            .with_context(|| format!("nullable audit lost root {root}"))?;
        let schema: serde_json::Value = serde_json::from_slice(bytes)?;
        collect_nullable_property_paths(&schema, "", |pointer| {
            discovered.insert((root.to_string(), pointer));
        });
    }
    let audited = REQUEST_EXPLICIT_NULL_PATHS
        .iter()
        .chain(REQUEST_OMITTED_NULL_PATHS)
        .map(|(root, pointer)| ((*root).to_string(), (*pointer).to_string()))
        .collect::<BTreeSet<_>>();
    if discovered != audited {
        bail!(
            "request nullable policy drift; missing={:?}, extra={:?}",
            discovered.difference(&audited).collect::<Vec<_>>(),
            audited.difference(&discovered).collect::<Vec<_>>()
        );
    }
    Ok(())
}

fn collect_nullable_property_paths(
    value: &serde_json::Value,
    pointer: &str,
    mut found: impl FnMut(String),
) {
    fn visit(value: &serde_json::Value, pointer: &str, found: &mut dyn FnMut(String)) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    let child_pointer = format!("{pointer}/{escaped}");
                    if pointer.ends_with("/properties") && schema_is_nullable(child) {
                        found(child_pointer.clone());
                    }
                    visit(child, &child_pointer, found);
                }
            }
            serde_json::Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    visit(child, &format!("{pointer}/{index}"), found);
                }
            }
            _ => {}
        }
    }
    visit(value, pointer, &mut found);
}

fn audit_owned_roundtrips() -> Result<()> {
    use crate::owned_codex_v2 as owned;

    let omitted_text_elements = serde_json::json!({
        "type": "text",
        "text": "hello"
    });
    let canonical_empty_text_elements = serde_json::json!({
        "type": "text",
        "text": "hello",
        "text_elements": []
    });
    assert_deserializes_to_canonical::<codex_app_server_protocol::UserInput>(
        "pinned vendor UserInput.Text omitted text_elements",
        &omitted_text_elements,
        &canonical_empty_text_elements,
    )?;
    assert_deserializes_to_canonical::<owned::thread_item::UserInput>(
        "owned UserInput.Text omitted text_elements",
        &omitted_text_elements,
        &canonical_empty_text_elements,
    )?;
    strict_roundtrip::<codex_app_server_protocol::UserInput, owned::thread_item::UserInput>(
        "UserInput.Text omitted text_elements",
        omitted_text_elements,
    )?;

    let non_empty_text_elements = serde_json::json!({
        "type": "text",
        "text": "$skill",
        "text_elements": [{
            "byteRange": {"start": 0, "end": 6},
            "placeholder": "$skill"
        }]
    });
    strict_roundtrip::<codex_app_server_protocol::UserInput, owned::thread_item::UserInput>(
        "UserInput.Text non-empty text_elements",
        non_empty_text_elements.clone(),
    )?;
    assert_vendor_preserves_fixture::<codex_app_server_protocol::UserInput>(
        "pinned vendor UserInput.Text non-empty text_elements",
        &non_empty_text_elements,
    )?;
    assert_owned_preserves_fixture::<owned::thread_item::UserInput>(
        "owned UserInput.Text non-empty text_elements",
        &non_empty_text_elements,
    )?;

    let omitted_reasoning_vectors = serde_json::json!({
        "type": "reasoning",
        "id": "main-reasoning-default-vectors"
    });
    let canonical_reasoning_vectors = serde_json::json!({
        "type": "reasoning",
        "id": "main-reasoning-default-vectors",
        "summary": [],
        "content": []
    });
    assert_deserializes_to_canonical::<codex_app_server_protocol::ThreadItem>(
        "pinned vendor ThreadItem.Reasoning omitted default vectors",
        &omitted_reasoning_vectors,
        &canonical_reasoning_vectors,
    )?;
    assert_deserializes_to_canonical::<owned::thread_item::ThreadItem>(
        "owned ThreadItem.Reasoning omitted default vectors",
        &omitted_reasoning_vectors,
        &canonical_reasoning_vectors,
    )?;
    strict_roundtrip::<codex_app_server_protocol::ThreadItem, owned::thread_item::ThreadItem>(
        "ThreadItem.Reasoning omitted default vectors",
        omitted_reasoning_vectors,
    )?;

    let omitted_workspace_writable_roots = serde_json::json!({
        "type": "workspaceWrite",
        "networkAccess": false,
        "excludeTmpdirEnvVar": false,
        "excludeSlashTmp": false
    });
    let canonical_workspace_writable_roots = serde_json::json!({
        "type": "workspaceWrite",
        "writableRoots": [],
        "networkAccess": false,
        "excludeTmpdirEnvVar": false,
        "excludeSlashTmp": false
    });
    assert_deserializes_to_canonical::<codex_app_server_protocol::SandboxPolicy>(
        "pinned vendor SandboxPolicy.WorkspaceWrite omitted writableRoots",
        &omitted_workspace_writable_roots,
        &canonical_workspace_writable_roots,
    )?;
    assert_deserializes_to_canonical::<owned::server_notification::SandboxPolicy>(
        "owned SandboxPolicy.WorkspaceWrite omitted writableRoots",
        &omitted_workspace_writable_roots,
        &canonical_workspace_writable_roots,
    )?;
    strict_roundtrip::<
        codex_app_server_protocol::SandboxPolicy,
        owned::server_notification::SandboxPolicy,
    >(
        "SandboxPolicy.WorkspaceWrite omitted writableRoots",
        omitted_workspace_writable_roots,
    )?;

    let app_without_plugin_names = serde_json::json!({
        "id": "app-1",
        "name": "Fixture",
        "description": "fixture",
        "logoUrl": "https://example.test/light.png",
        "logoUrlDark": "https://example.test/dark.png",
        "iconAssets": {"small": "https://example.test/icon.png"},
        "iconDarkAssets": {"small": "https://example.test/icon-dark.png"},
        "distributionChannel": "plugin",
        "branding": {
            "category": "productivity",
            "developer": "AgentDash",
            "website": "https://example.test",
            "privacyPolicy": "https://example.test/privacy",
            "termsOfService": "https://example.test/terms",
            "isDiscoverableApp": true
        },
        "appMetadata": {
            "review": {"status": "approved"},
            "categories": ["productivity"],
            "subCategories": ["agents"],
            "seoDescription": "fixture",
            "screenshots": [{
                "url": "https://example.test/screenshot.png",
                "fileId": "file-1",
                "userPrompt": "fixture"
            }],
            "developer": "AgentDash",
            "version": "1.0.0",
            "versionId": "version-1",
            "versionNotes": "fixture",
            "firstPartyType": "fixture",
            "firstPartyRequiresInstall": true,
            "showInComposerWhenUnlinked": true
        },
        "labels": {"tier": "fixture"},
        "installUrl": "https://example.test/install",
        "isAccessible": true,
        "isEnabled": true
    });
    let mut canonical_app = app_without_plugin_names.clone();
    canonical_app["pluginDisplayNames"] = serde_json::json!([]);
    assert_deserializes_to_canonical::<codex_app_server_protocol::AppInfo>(
        "pinned vendor AppInfo omitted pluginDisplayNames",
        &app_without_plugin_names,
        &canonical_app,
    )?;
    assert_deserializes_to_canonical::<owned::server_notification::AppInfo>(
        "owned AppInfo omitted pluginDisplayNames",
        &app_without_plugin_names,
        &canonical_app,
    )?;
    strict_roundtrip::<codex_app_server_protocol::AppInfo, owned::server_notification::AppInfo>(
        "AppInfo omitted pluginDisplayNames",
        app_without_plugin_names,
    )?;

    let main_thread_items = [
        serde_json::json!({
            "type": "agentMessage",
            "id": "main-agent-message",
            "text": "hello",
            "phase": null,
            "memoryCitation": null
        }),
        serde_json::json!({
            "type": "agentMessage",
            "id": "main-agent-message-omitted-nullables",
            "text": "hello without optional fields"
        }),
        serde_json::json!({
            "type": "reasoning",
            "id": "main-reasoning",
            "summary": ["summary"],
            "content": ["detail"]
        }),
        serde_json::json!({
            "type": "reasoning",
            "id": "main-reasoning-default-vectors",
            "summary": [],
            "content": []
        }),
        serde_json::json!({
            "type": "commandExecution",
            "id": "main-command",
            "command": "echo hello",
            "cwd": "/workspace",
            "processId": null,
            "source": "agent",
            "status": "completed",
            "commandActions": [],
            "aggregatedOutput": "hello",
            "exitCode": 0,
            "durationMs": 7
        }),
        serde_json::json!({
            "type": "fileChange",
            "id": "main-file-change",
            "changes": [],
            "status": "completed"
        }),
        serde_json::json!({
            "type": "dynamicToolCall",
            "id": "main-dynamic-tool",
            "namespace": null,
            "tool": "workspace_open",
            "arguments": {"path": "README.md"},
            "status": "completed",
            "contentItems": [{"type": "inputText", "text": "ok"}],
            "success": true,
            "durationMs": 9
        }),
        serde_json::json!({
            "type": "contextCompaction",
            "id": "main-compaction"
        }),
    ];
    strict_roundtrip_many::<codex_app_server_protocol::ThreadItem, owned::thread_item::ThreadItem>(
        "ThreadItem",
        &main_thread_items,
    )?;
    for (index, fixture) in main_thread_items.iter().enumerate() {
        assert_owned_preserves_fixture::<owned::thread_item::ThreadItem>(
            &format!("owned protected ThreadItem[{index}]"),
            fixture,
        )?;
        if index != 1 {
            assert_vendor_preserves_fixture::<codex_app_server_protocol::ThreadItem>(
                &format!("main protected ThreadItem[{index}]"),
                fixture,
            )?;
        }
    }
    let item_started = serde_json::json!({
        "method": "item/started",
        "params": {
            "threadId": "main-thread",
            "turnId": "main-turn",
            "item": main_thread_items[0].clone(),
            "startedAtMs": 1700000000123_i64
        }
    });
    strict_roundtrip::<
        codex_app_server_protocol::ServerNotification,
        owned::server_notification::ServerNotification,
    >("ServerNotification", item_started.clone())?;
    assert_vendor_preserves_fixture::<codex_app_server_protocol::ServerNotification>(
        "main protected ServerNotification",
        &item_started,
    )?;

    let failed_turn = serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": "main-thread",
            "turn": {
                "id": "main-turn",
                "items": [],
                "itemsView": "full",
                "status": "failed",
                "startedAt": null,
                "completedAt": null,
                "durationMs": null,
                "error": {
                    "message": "provider failed",
                    "additionalDetails": null,
                    "codexErrorInfo": null
                }
            }
        }
    });
    strict_roundtrip::<
        codex_app_server_protocol::ServerNotification,
        owned::server_notification::ServerNotification,
    >(
        "ServerNotification failed TurnError nulls",
        failed_turn.clone(),
    )?;
    assert_vendor_preserves_fixture::<codex_app_server_protocol::ServerNotification>(
        "main protected failed TurnError nulls",
        &failed_turn,
    )?;
    let failed_turn_omitted_timing = serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": "main-thread",
            "turn": {
                "id": "main-turn",
                "items": [],
                "itemsView": "full",
                "status": "failed",
                "error": {
                    "message": "provider failed",
                    "additionalDetails": null,
                    "codexErrorInfo": null
                }
            }
        }
    });
    assert_owned_preserves_fixture::<owned::server_notification::ServerNotification>(
        "owned turn/completed omitted timing",
        &failed_turn_omitted_timing,
    )?;
    let failed_turn_null_timing = serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": "main-thread",
            "turn": {
                "id": "main-turn",
                "items": [],
                "itemsView": "full",
                "status": "failed",
                "startedAt": null,
                "completedAt": null,
                "durationMs": null,
                "error": {
                    "message": "provider failed",
                    "additionalDetails": null,
                    "codexErrorInfo": null
                }
            }
        }
    });
    assert_owned_preserves_fixture::<owned::server_notification::ServerNotification>(
        "owned turn/completed explicit null timing",
        &failed_turn_null_timing,
    )?;

    strict_roundtrip::<
        codex_app_server_protocol::CommandExecutionRequestApprovalParams,
        owned::command_execution_request_approval_params::CommandExecutionRequestApprovalParams,
    >(
        "CommandExecutionRequestApprovalParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": "main-turn",
            "itemId": "main-command",
            "startedAtMs": 1700000000123_i64,
            "environmentId": null
        }),
    )?;
    strict_roundtrip::<
        codex_app_server_protocol::FileChangeRequestApprovalParams,
        owned::file_change_request_approval_params::FileChangeRequestApprovalParams,
    >(
        "FileChangeRequestApprovalParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": "main-turn",
            "itemId": "main-file-change",
            "startedAtMs": 1700000000123_i64,
            "reason": null,
            "grantRoot": null
        }),
    )?;
    let cwd = env::current_dir()?.to_string_lossy().into_owned();
    strict_roundtrip::<
        codex_app_server_protocol::PermissionsRequestApprovalParams,
        owned::permissions_request_approval_params::PermissionsRequestApprovalParams,
    >(
        "PermissionsRequestApprovalParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": "main-turn",
            "itemId": "main-command",
            "environmentId": null,
            "startedAtMs": 1700000000123_i64,
            "cwd": cwd,
            "reason": null,
            "permissions": {"network": null, "fileSystem": null}
        }),
    )?;
    strict_roundtrip::<
        codex_app_server_protocol::ToolRequestUserInputParams,
        owned::tool_request_user_input_params::ToolRequestUserInputParams,
    >(
        "ToolRequestUserInputParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": "main-turn",
            "itemId": "main-question",
            "questions": [{
                "id": "question-1",
                "header": "Choice",
                "question": "Continue?",
                "isOther": false,
                "isSecret": false,
                "options": null
            }],
            "autoResolutionMs": null
        }),
    )?;
    strict_roundtrip::<
        codex_app_server_protocol::DynamicToolCallParams,
        owned::dynamic_tool_call_params::DynamicToolCallParams,
    >(
        "DynamicToolCallParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": "main-turn",
            "callId": "main-call",
            "namespace": null,
            "tool": "workspace_open",
            "arguments": {"path": "README.md"}
        }),
    )?;
    strict_roundtrip::<
        codex_app_server_protocol::McpServerElicitationRequestParams,
        owned::mcp_server_elicitation_request_params::McpServerElicitationRequestParams,
    >(
        "McpServerElicitationRequestParams",
        serde_json::json!({
            "threadId": "main-thread",
            "turnId": null,
            "serverName": "main-mcp",
            "mode": "url",
            "_meta": null,
            "message": "Open form",
            "url": "https://example.test/form",
            "elicitationId": "elicitation-1"
        }),
    )?;
    Ok(())
}

fn strict_roundtrip_many<V, O>(label: &str, fixtures: &[serde_json::Value]) -> Result<()>
where
    V: DeserializeOwned + Serialize,
    O: DeserializeOwned + Serialize,
{
    for (index, fixture) in fixtures.iter().enumerate() {
        strict_roundtrip::<V, O>(&format!("{label}[{index}]"), fixture.clone())?;
    }
    Ok(())
}

fn strict_roundtrip<V, O>(label: &str, fixture: serde_json::Value) -> Result<()>
where
    V: DeserializeOwned + Serialize,
    O: DeserializeOwned + Serialize,
{
    let vendor: V = serde_json::from_value(fixture)
        .with_context(|| format!("deserialize pinned vendor {label}"))?;
    let vendor_json =
        serde_json::to_value(vendor).with_context(|| format!("serialize pinned vendor {label}"))?;
    let owned: O = serde_json::from_value(vendor_json.clone())
        .with_context(|| format!("strict transcode vendor to owned {label}"))?;
    let owned_json = serde_json::to_value(owned)
        .with_context(|| format!("serialize AgentDash-owned {label}"))?;
    if vendor_json != owned_json {
        bail!(
            "vendor↔owned JSON drift for {label}:\nvendor={}\nowned={}",
            serde_json::to_string_pretty(&vendor_json)?,
            serde_json::to_string_pretty(&owned_json)?
        );
    }
    let vendor_again: V = serde_json::from_value(owned_json.clone())
        .with_context(|| format!("strict transcode owned to vendor {label}"))?;
    let vendor_again_json = serde_json::to_value(vendor_again)
        .with_context(|| format!("reserialize pinned vendor {label}"))?;
    if vendor_again_json != owned_json {
        bail!("owned→vendor JSON drift for {label}");
    }
    Ok(())
}

fn assert_vendor_preserves_fixture<V>(label: &str, fixture: &serde_json::Value) -> Result<()>
where
    V: DeserializeOwned + Serialize,
{
    let vendor: V =
        serde_json::from_value(fixture.clone()).with_context(|| format!("deserialize {label}"))?;
    let serialized = serde_json::to_value(vendor).with_context(|| format!("serialize {label}"))?;
    if serialized != *fixture {
        bail!(
            "pinned 0.144.1 changed {label}:\nfixture={}\nserialized={}",
            serde_json::to_string_pretty(fixture)?,
            serde_json::to_string_pretty(&serialized)?
        );
    }
    Ok(())
}

fn assert_owned_preserves_fixture<O>(label: &str, fixture: &serde_json::Value) -> Result<()>
where
    O: DeserializeOwned + Serialize,
{
    let owned: O =
        serde_json::from_value(fixture.clone()).with_context(|| format!("deserialize {label}"))?;
    let serialized = serde_json::to_value(owned).with_context(|| format!("serialize {label}"))?;
    if serialized != *fixture {
        bail!(
            "AgentDash-owned type changed {label}:\nfixture={}\nserialized={}",
            serde_json::to_string_pretty(fixture)?,
            serde_json::to_string_pretty(&serialized)?
        );
    }
    Ok(())
}

fn assert_deserializes_to_canonical<T>(
    label: &str,
    fixture: &serde_json::Value,
    canonical: &serde_json::Value,
) -> Result<()>
where
    T: DeserializeOwned + Serialize,
{
    let value: T =
        serde_json::from_value(fixture.clone()).with_context(|| format!("deserialize {label}"))?;
    let serialized = serde_json::to_value(value).with_context(|| format!("serialize {label}"))?;
    if serialized != *canonical {
        bail!(
            "{label} canonical serialization drift:\nexpected={}\nserialized={}",
            serde_json::to_string_pretty(canonical)?,
            serde_json::to_string_pretty(&serialized)?
        );
    }
    Ok(())
}

fn generate_rust(mut value: serde_json::Value, source: &str) -> Result<String> {
    normalize_nullable_types(&mut value);
    let schema: RootSchema = serde_json::from_value(value)
        .with_context(|| format!("parse normalized {source} schema"))?;
    let mut settings = TypeSpaceSettings::default();
    settings.with_derive("::ts_rs::TS".to_string());
    settings.with_derive("::schemars::JsonSchema".to_string());
    settings.with_derive("PartialEq".to_string());
    let mut space = TypeSpace::new(&settings);
    space.add_root_schema(schema)?;
    let tokens = space.to_stream().to_string();
    if tokens.is_empty() {
        bail!("typify generated no Rust for {source}");
    }
    Ok(tokens)
}

fn render_rust(modules: Vec<(String, String)>) -> Result<String> {
    let mut source = String::from(
        "// @generated by agentdash-integration-codex private vendor codegen.\n\
         pub(crate) fn deserialize_optional_explicit_null<'de, D, T>(\n\
             deserializer: D,\n\
         ) -> Result<Option<Option<T>>, D::Error>\n\
         where\n\
             D: ::serde::Deserializer<'de>,\n\
             T: ::serde::Deserialize<'de>,\n\
         {\n\
             <Option<T> as ::serde::Deserialize>::deserialize(deserializer).map(Some)\n\
         }\n",
    );
    for (name, tokens) in modules {
        let mut module = rustfmt(&format!("pub mod {name} {{ {tokens} }}\n"))?;
        match name.as_str() {
            "thread_item" | "server_notification" => {
                preserve_optional_explicit_null_fields_in_enum_variants(
                    &mut module,
                    "pub enum ThreadItem",
                    THREAD_ITEM_NULLABLE_PATHS,
                )?;
                preserve_default_vec_serialization_in_enum_variant(
                    &mut module,
                    "pub enum ThreadItem",
                    "Reasoning.content",
                )?;
                preserve_default_vec_serialization_in_enum_variant(
                    &mut module,
                    "pub enum ThreadItem",
                    "Reasoning.summary",
                )?;
                preserve_optional_explicit_null_fields_in_enum_variants(
                    &mut module,
                    "pub enum CommandAction",
                    &["ListFiles.path", "Search.path", "Search.query"],
                )?;
                preserve_optional_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct McpToolCallAppContext",
                    &[
                        "action_name".to_string(),
                        "app_name".to_string(),
                        "link_id".to_string(),
                        "resource_uri".to_string(),
                        "template_id".to_string(),
                    ],
                )?;
                preserve_optional_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct TextElement",
                    &["placeholder".to_string()],
                )?;
                preserve_optional_explicit_null_fields_in_enum_variants(
                    &mut module,
                    "pub enum UserInput",
                    &["Image.detail", "LocalImage.detail"],
                )?;
                preserve_default_vec_serialization_in_enum_variant(
                    &mut module,
                    "pub enum UserInput",
                    "Text.text_elements",
                )?;
                preserve_optional_explicit_null_fields_in_enum_variants(
                    &mut module,
                    "pub enum WebSearchAction",
                    &[
                        "Search.queries",
                        "Search.query",
                        "OpenPage.url",
                        "FindInPage.pattern",
                        "FindInPage.url",
                    ],
                )?;
                if name == "server_notification" {
                    preserve_default_vec_serialization_in_type(
                        &mut module,
                        "pub struct AppInfo",
                        "plugin_display_names",
                    )?;
                    preserve_default_vec_serialization_in_enum_variant(
                        &mut module,
                        "pub enum SandboxPolicy",
                        "WorkspaceWrite.writable_roots",
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct TurnError",
                        &[
                            "additional_details".to_string(),
                            "codex_error_info".to_string(),
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_enum_variants(
                        &mut module,
                        "pub enum GuardianApprovalReviewAction",
                        &[
                            "McpToolCall.connectorId",
                            "McpToolCall.connectorName",
                            "McpToolCall.toolTitle",
                            "RequestPermissions.reason",
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct RequestPermissionProfile",
                        &["file_system".to_string(), "network".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct AdditionalFileSystemPermissions",
                        &[
                            "entries".to_string(),
                            "glob_scan_max_depth".to_string(),
                            "read".to_string(),
                            "write".to_string(),
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_enum_variants(
                        &mut module,
                        "pub enum FileSystemSpecialPath",
                        &["ProjectRoots.subpath", "Unknown.subpath"],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct AdditionalNetworkPermissions",
                        &["enabled".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct GuardianApprovalReview",
                        &[
                            "rationale".to_string(),
                            "risk_level".to_string(),
                            "user_authorization".to_string(),
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct ItemGuardianApprovalReviewStartedNotification",
                        &["target_item_id".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct ItemGuardianApprovalReviewCompletedNotification",
                        &["target_item_id".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_enum_variants(
                        &mut module,
                        "pub enum PatchChangeKind",
                        &["Update.move_path"],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct ModelSafetyBufferingUpdatedNotification",
                        &["faster_model".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct WarningNotification",
                        &["thread_id".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct DeprecationNoticeNotification",
                        &["details".to_string()],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct ConfigWarningNotification",
                        &[
                            "details".to_string(),
                            "path".to_string(),
                            "range".to_string(),
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct Turn",
                        &[
                            "completed_at".to_string(),
                            "duration_ms".to_string(),
                            "error".to_string(),
                            "started_at".to_string(),
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_enum_variants(
                        &mut module,
                        "pub enum CodexErrorInfo",
                        &[
                            "HttpConnectionFailed.httpStatusCode",
                            "ResponseStreamConnectionFailed.httpStatusCode",
                            "ResponseStreamDisconnected.httpStatusCode",
                            "ResponseTooManyFailedAttempts.httpStatusCode",
                        ],
                    )?;
                    preserve_optional_explicit_null_fields_in_type(
                        &mut module,
                        "pub struct TurnPlanUpdatedNotification",
                        &["explanation".to_string()],
                    )?;
                    rename_typescript_type(
                        &mut module,
                        "pub struct RequestPermissionProfile",
                        "ServerNotificationRequestPermissionProfile",
                    )?;
                }
            }
            "mcp_server_elicitation_request_params" => {
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub enum McpServerElicitationRequestParams",
                    &["meta".to_string(), "turn_id".to_string()],
                )?;
            }
            "command_execution_request_approval_params" => {
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct CommandExecutionRequestApprovalParams",
                    &["environment_id".to_string()],
                )?;
            }
            "file_change_request_approval_params" => {
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct FileChangeRequestApprovalParams",
                    &["grant_root".to_string(), "reason".to_string()],
                )?;
            }
            "permissions_request_approval_params" => {
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct PermissionsRequestApprovalParams",
                    &["environment_id".to_string(), "reason".to_string()],
                )?;
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct RequestPermissionProfile",
                    &["file_system".to_string(), "network".to_string()],
                )?;
            }
            "tool_request_user_input_params" => {
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct ToolRequestUserInputParams",
                    &["auto_resolution_ms".to_string()],
                )?;
                preserve_explicit_null_fields_in_type(
                    &mut module,
                    "pub struct ToolRequestUserInputQuestion",
                    &["options".to_string()],
                )?;
            }
            "dynamic_tool_call_params" => preserve_explicit_null_fields_in_type(
                &mut module,
                "pub struct DynamicToolCallParams",
                &["namespace".to_string()],
            )?,
            _ => {}
        }
        annotate_ts_numbers(&mut module);
        module = module.replace("Int64(i64),", "Int64(#[ts(type = \"number\")] i64),");
        source.push_str(&module);
    }
    rustfmt(&source)
}

fn annotate_ts_numbers(source: &mut String) {
    let mut output = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.ends_with(',')
            && (trimmed.starts_with("pub ")
                || (!trimmed.starts_with("pub mod") && trimmed.contains(':')))
            && (line.contains(": i64")
                || line.contains(": u64")
                || line.contains("Option<i64>")
                || line.contains("Option<u64>"))
        {
            let indent = &line[..line.len() - trimmed.len()];
            let ts_type = if line.contains("Option<") {
                "number | null"
            } else {
                "number"
            };
            output.push(format!("{indent}#[ts(type = \"{ts_type}\")]"));
        }
        output.push(line.to_string());
    }
    *source = format!("{}\n", output.join("\n"));
}

fn preserve_optional_explicit_null_fields_in_type(
    source: &mut String,
    type_marker: &str,
    fields: &[String],
) -> Result<()> {
    let type_start = source
        .find(type_marker)
        .with_context(|| format!("generated Rust lost {type_marker}"))?;
    let brace_start = source[type_start..]
        .find('{')
        .map(|offset| type_start + offset)
        .with_context(|| format!("generated Rust {type_marker} has no body"))?;
    let type_end = matching_brace(source, brace_start)
        .with_context(|| format!("generated Rust {type_marker} has unbalanced braces"))?;
    let mut type_source = source[type_start..=type_end].to_string();
    preserve_optional_explicit_null_fields(&mut type_source, fields)?;
    type_source.truncate(type_source.trim_end_matches(['\r', '\n']).len());
    source.replace_range(type_start..=type_end, &type_source);
    Ok(())
}

fn preserve_optional_explicit_null_fields_in_enum_variants(
    source: &mut String,
    enum_marker: &str,
    qualified_fields: &[&str],
) -> Result<()> {
    let enum_start = source
        .find(enum_marker)
        .with_context(|| format!("generated Rust lost {enum_marker}"))?;
    let brace_start = source[enum_start..]
        .find('{')
        .map(|offset| enum_start + offset)
        .with_context(|| format!("generated Rust {enum_marker} has no body"))?;
    let enum_end = matching_brace(source, brace_start)
        .with_context(|| format!("generated Rust {enum_marker} has unbalanced braces"))?;
    let mut enum_source = source[enum_start..=enum_end].to_string();
    for qualified in qualified_fields {
        let (variant, field) = qualified
            .split_once('.')
            .context("nullable path must be variant-qualified")?;
        let variant_marker = format!("{variant} {{");
        let variant_start = enum_source
            .find(&variant_marker)
            .with_context(|| format!("generated Rust {enum_marker} lost variant {variant}"))?;
        let variant_brace = variant_start + variant_marker.len() - 1;
        let variant_end = matching_brace(&enum_source, variant_brace).with_context(|| {
            format!("generated Rust {enum_marker}.{variant} has unbalanced braces")
        })?;
        let mut variant_source = enum_source[variant_start..=variant_end].to_string();
        preserve_optional_explicit_null_fields(&mut variant_source, &[snake(field)])?;
        variant_source.truncate(variant_source.trim_end_matches(['\r', '\n']).len());
        enum_source.replace_range(variant_start..=variant_end, &variant_source);
    }
    enum_source.truncate(enum_source.trim_end_matches(['\r', '\n']).len());
    source.replace_range(enum_start..=enum_end, &enum_source);
    Ok(())
}

fn preserve_default_vec_serialization_in_enum_variant(
    source: &mut String,
    enum_marker: &str,
    qualified_field: &str,
) -> Result<()> {
    let (variant, field) = qualified_field
        .split_once('.')
        .context("serialization overlay path must be variant-qualified")?;
    let enum_start = source
        .find(enum_marker)
        .with_context(|| format!("generated Rust lost {enum_marker}"))?;
    let enum_brace = source[enum_start..]
        .find('{')
        .map(|offset| enum_start + offset)
        .with_context(|| format!("generated Rust {enum_marker} has no body"))?;
    let enum_end = matching_brace(source, enum_brace)
        .with_context(|| format!("generated Rust {enum_marker} has unbalanced braces"))?;
    let mut enum_source = source[enum_start..=enum_end].to_string();
    let variant_marker = format!("{variant} {{");
    let variant_start = enum_source
        .find(&variant_marker)
        .with_context(|| format!("generated Rust {enum_marker} lost variant {variant}"))?;
    let variant_brace = variant_start + variant_marker.len() - 1;
    let variant_end = matching_brace(&enum_source, variant_brace)
        .with_context(|| format!("generated Rust {enum_marker}.{variant} has unbalanced braces"))?;
    let mut variant_source = enum_source[variant_start..=variant_end].to_string();
    preserve_default_vec_field_serialization(&mut variant_source, field)?;
    enum_source.replace_range(variant_start..=variant_end, &variant_source);
    source.replace_range(enum_start..=enum_end, &enum_source);
    Ok(())
}

fn preserve_default_vec_serialization_in_type(
    source: &mut String,
    type_marker: &str,
    field: &str,
) -> Result<()> {
    let type_start = source
        .find(type_marker)
        .with_context(|| format!("generated Rust lost {type_marker}"))?;
    let type_brace = source[type_start..]
        .find('{')
        .map(|offset| type_start + offset)
        .with_context(|| format!("generated Rust {type_marker} has no body"))?;
    let type_end = matching_brace(source, type_brace)
        .with_context(|| format!("generated Rust {type_marker} has unbalanced braces"))?;
    let mut type_source = source[type_start..=type_end].to_string();
    preserve_default_vec_field_serialization(&mut type_source, field)?;
    source.replace_range(type_start..=type_end, &type_source);
    Ok(())
}

fn preserve_default_vec_field_serialization(source: &mut String, field: &str) -> Result<()> {
    let mut lines = source.lines().map(ToString::to_string).collect::<Vec<_>>();
    let field_index = lines
        .iter()
        .position(|line| {
            let line = line.trim_start();
            line.starts_with(&format!("pub {field}:")) || line.starts_with(&format!("{field}:"))
        })
        .with_context(|| format!("generated Rust lost default Vec field {field}"))?;
    if !lines[field_index].contains("::std::vec::Vec<") {
        bail!("generated Rust {field} is no longer a Vec");
    }
    let attribute_index = (0..field_index)
        .rev()
        .find(|index| lines[*index].trim_start().starts_with("#[serde("))
        .with_context(|| format!("generated Rust {field} lost serde attributes"))?;
    let attribute_end = (attribute_index..field_index)
        .find(|index| lines[*index].contains(")]"))
        .with_context(|| format!("generated Rust {field} has an unterminated serde attribute"))?;
    let attribute = lines[attribute_index..=attribute_end].join("\n");
    let skip = "skip_serializing_if = \"::std::vec::Vec::is_empty\"";
    if !attribute.contains("default") || attribute.matches(skip).count() != 1 {
        bail!("generated Rust {field} serialization attribute drifted: {attribute}");
    }
    if attribute_index == attribute_end {
        lines[attribute_index] = lines[attribute_index]
            .replace(&format!(", {skip}"), "")
            .replace(&format!("{skip}, "), "");
    } else {
        lines.remove(
            (attribute_index..=attribute_end)
                .find(|index| lines[*index].contains(skip))
                .context("default Vec skip line disappeared during overlay")?,
        );
    }
    *source = lines.join("\n");
    Ok(())
}

fn preserve_optional_explicit_null_fields(source: &mut String, fields: &[String]) -> Result<()> {
    let mut lines = source.lines().map(ToString::to_string).collect::<Vec<_>>();
    for field in fields {
        let index = lines
            .iter()
            .position(|line| {
                let line = line.trim_start();
                line.starts_with(&format!("pub {field}:")) || line.starts_with(&format!("{field}:"))
            })
            .with_context(|| format!("generated Rust lost nullable field {field}"))?;
        let line = &mut lines[index];
        let option = "::std::option::Option<";
        let option_start = line
            .find(option)
            .with_context(|| format!("generated nullable field {field} is not Option"))?;
        let comma = line
            .rfind(',')
            .with_context(|| format!("generated nullable field {field} has no trailing comma"))?;
        let closing = line[..comma]
            .rfind('>')
            .filter(|position| *position > option_start)
            .with_context(|| {
                format!("generated nullable field {field} has no Option terminator")
            })?;
        line.insert_str(option_start, option);
        line.insert(closing + option.len(), '>');
        let indent = line[..line.len() - line.trim_start().len()].to_string();
        lines.insert(
            index,
            format!(
                "{indent}#[serde(deserialize_with = \"super::deserialize_optional_explicit_null\")]"
            ),
        );
    }
    *source = format!("{}\n", lines.join("\n"));
    Ok(())
}

fn preserve_explicit_null_fields_in_type(
    source: &mut String,
    type_marker: &str,
    fields: &[String],
) -> Result<()> {
    let type_start = source
        .find(type_marker)
        .with_context(|| format!("generated Rust lost {type_marker}"))?;
    let brace_start = source[type_start..]
        .find('{')
        .map(|offset| type_start + offset)
        .with_context(|| format!("generated Rust {type_marker} has no body"))?;
    let type_end = matching_brace(source, brace_start)
        .with_context(|| format!("generated Rust {type_marker} has unbalanced braces"))?;
    let mut type_source = source[type_start..=type_end].to_string();
    preserve_explicit_null_fields(&mut type_source, fields);
    type_source.truncate(type_source.trim_end_matches(['\r', '\n']).len());
    source.replace_range(type_start..=type_end, &type_source);
    Ok(())
}

fn rename_typescript_type(source: &mut String, type_marker: &str, ts_name: &str) -> Result<()> {
    let type_start = source
        .find(type_marker)
        .with_context(|| format!("generated Rust lost {type_marker}"))?;
    let line_start = source[..type_start]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let indent = &source[line_start..type_start];
    source.insert_str(
        type_start,
        &format!("#[ts(rename = \"{ts_name}\")]\n{indent}"),
    );
    Ok(())
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn preserve_explicit_null_fields(source: &mut String, fields: &[String]) {
    let mut lines = source.lines().map(ToString::to_string).collect::<Vec<_>>();
    for index in 0..lines.len() {
        let Some(field) = fields.iter().find(|field| {
            let line = lines[index].trim_start();
            line.starts_with(&format!("pub {field}:")) || line.starts_with(&format!("{field}:"))
        }) else {
            continue;
        };
        let _ = field;
        let start = (0..index)
            .rev()
            .take_while(|candidate| !lines[*candidate].trim_start().starts_with("pub "))
            .find(|candidate| lines[*candidate].contains("#[serde("));
        let Some(start) = start else { continue };
        for line in &mut lines[start..index] {
            if line.contains("skip_serializing_if") {
                if line.trim_start().starts_with("skip_serializing_if") {
                    line.clear();
                } else {
                    *line = line
                        .replace(
                            ", skip_serializing_if = \"::std::option::Option::is_none\"",
                            "",
                        )
                        .replace(
                            "skip_serializing_if = \"::std::option::Option::is_none\", ",
                            "",
                        );
                }
            }
        }
    }
    *source = format!(
        "{}\n",
        lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn apply_owned_ts_overlays(relative: &Path, bytes: &mut Vec<u8>) -> Result<()> {
    let file = relative
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let mut text = String::from_utf8(std::mem::take(bytes))?;
    match file {
        "ThreadItem.ts" => {
            for path in THREAD_ITEM_NULLABLE_PATHS {
                let (variant, field) = path.split_once('.').expect("qualified nullable path");
                if !["WebSearch", "ImageGeneration"].contains(&variant) {
                    make_union_field_optional_nullable(&mut text, variant, field)?;
                }
            }
            assert_sleep_duration_contract(&text)?;
        }
        "WebSearchItem.ts" => make_optional_nullable(&mut text, "action")?,
        "ImageGenerationItem.ts" => {
            make_optional_nullable(&mut text, "revisedPrompt")?;
            make_optional_nullable(&mut text, "savedPath")?;
        }
        "McpServerElicitationRequestParams.ts" => {
            make_optional_nullable(&mut text, "turnId")?;
            make_optional_nullable(&mut text, "_meta")?;
        }
        _ => {}
    }
    text = text.replace("bigint", "number");
    *bytes = text.into_bytes();
    Ok(())
}

fn make_union_field_optional_nullable(text: &mut String, variant: &str, field: &str) -> Result<()> {
    let mut chars = variant.chars();
    let discriminator = chars
        .next()
        .map(|first| first.to_lowercase().collect::<String>() + chars.as_str())
        .context("empty ThreadItem variant")?;
    let marker = format!("\"type\": \"{discriminator}\"");
    let start = text
        .find(&marker)
        .with_context(|| format!("generated ThreadItem TS lost variant {variant}"))?;
    let end = text[start..]
        .find(" } | { ")
        .map(|offset| start + offset)
        .unwrap_or(text.len());
    let mut branch = text[start..end].to_string();
    make_optional_nullable(&mut branch, field)?;
    text.replace_range(start..end, &branch);
    Ok(())
}

fn assert_sleep_duration_contract(text: &str) -> Result<()> {
    let start = text
        .find("\"type\": \"sleep\"")
        .context("ThreadItem TS lost Sleep")?;
    let end = text[start..]
        .find(" } | { ")
        .map(|offset| start + offset)
        .unwrap_or(text.len());
    let branch = &text[start..end];
    if !branch.contains("durationMs: number")
        || branch.contains("durationMs?:")
        || branch.contains("durationMs: number | null")
    {
        bail!("Sleep.durationMs must remain required non-null number");
    }
    Ok(())
}

fn make_optional_nullable(text: &mut String, field: &str) -> Result<()> {
    let required = format!("{field}: ");
    let optional = format!("{field}?: ");
    *text = text.replace(&required, &optional);
    let start = text
        .find(&optional)
        .with_context(|| format!("generated TypeScript lost nullable field {field}"))?;
    let value_start = start + optional.len();
    let end = text[value_start..]
        .find(',')
        .map(|offset| value_start + offset)
        .with_context(|| format!("generated TypeScript field {field} has no terminator"))?;
    if !text[value_start..end].contains("null") {
        text.insert_str(end, " | null");
    }
    Ok(())
}

fn thread_item_nullable_fields(bundle: &serde_json::Value) -> Result<BTreeSet<String>> {
    let variants = bundle["definitions"]["ThreadItem"]["oneOf"]
        .as_array()
        .context("ThreadItem schema lost oneOf variants")?;
    let mut output = BTreeSet::new();
    for variant in variants {
        let variant_name = variant["title"]
            .as_str()
            .and_then(|title| title.strip_suffix("ThreadItem"))
            .context("ThreadItem variant lost qualified title")?;
        let Some(properties) = variant
            .get("properties")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (field, schema) in properties {
            if schema_is_nullable(schema) {
                output.insert(format!("{variant_name}.{field}"));
            }
        }
        if variant_name == "Sleep" {
            let required = variant["required"]
                .as_array()
                .context("SleepThreadItem lost required list")?;
            let duration = properties
                .get("durationMs")
                .context("SleepThreadItem lost durationMs")?;
            if !required.iter().any(|field| field == "durationMs") || schema_is_nullable(duration) {
                bail!("Sleep.durationMs must remain required and non-null in pinned schema");
            }
        }
    }
    Ok(output)
}

fn session_notification_nullable_paths(bundle: &serde_json::Value) -> Result<BTreeSet<String>> {
    fn visit(
        bundle: &serde_json::Value,
        schema: &serde_json::Value,
        pointer: &str,
        visited_definitions: &mut BTreeSet<String>,
        output: &mut BTreeSet<String>,
    ) -> Result<()> {
        if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
            let name = reference
                .strip_prefix("#/definitions/")
                .with_context(|| format!("unsupported session schema reference {reference}"))?;
            if visited_definitions.insert(name.to_string()) {
                let definition = bundle["definitions"]
                    .get(name)
                    .with_context(|| format!("session schema lost definition {name}"))?;
                visit(
                    bundle,
                    definition,
                    &format!("/definitions/{name}"),
                    visited_definitions,
                    output,
                )?;
            }
            return Ok(());
        }
        if let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            for (field, child) in properties {
                let child_pointer = format!("{pointer}/properties/{field}");
                if schema_is_nullable(child) {
                    output.insert(child_pointer.clone());
                }
                visit(bundle, child, &child_pointer, visited_definitions, output)?;
            }
        }
        for keyword in ["oneOf", "anyOf", "allOf"] {
            if let Some(branches) = schema.get(keyword).and_then(serde_json::Value::as_array) {
                for (index, child) in branches.iter().enumerate() {
                    visit(
                        bundle,
                        child,
                        &format!("{pointer}/{keyword}/{index}"),
                        visited_definitions,
                        output,
                    )?;
                }
            }
        }
        if let Some(items) = schema.get("items") {
            visit(
                bundle,
                items,
                &format!("{pointer}/items"),
                visited_definitions,
                output,
            )?;
        }
        Ok(())
    }

    let mut visited_definitions = BTreeSet::new();
    let mut output = BTreeSet::new();
    for root in SESSION_NOTIFICATION_ROOTS {
        if visited_definitions.insert((*root).to_string()) {
            let definition = bundle["definitions"]
                .get(*root)
                .with_context(|| format!("session notification schema lost root {root}"))?;
            visit(
                bundle,
                definition,
                &format!("/definitions/{root}"),
                &mut visited_definitions,
                &mut output,
            )?;
        }
    }
    Ok(output)
}

fn schema_is_nullable(schema: &serde_json::Value) -> bool {
    schema
        .get("type")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|types| types.iter().any(|kind| kind == "null"))
        || ["oneOf", "anyOf"].iter().any(|key| {
            schema
                .get(*key)
                .and_then(serde_json::Value::as_array)
                .is_some_and(|variants| {
                    variants.iter().any(|variant| {
                        variant.get("type") == Some(&serde_json::Value::String("null".to_string()))
                    })
                })
        })
}

fn rustfmt(source: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("spawn workspace rustfmt")?;
    child
        .stdin
        .take()
        .context("rustfmt stdin")?
        .write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "rustfmt failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn normalize_nullable_types(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => values.iter_mut().for_each(normalize_nullable_types),
        serde_json::Value::Object(object) => {
            if let Some(serde_json::Value::Array(types)) = object.get("type")
                && types.len() == 2
                && types.iter().any(|kind| kind == "null")
            {
                let non_null = types.iter().find(|kind| *kind != "null").cloned().unwrap();
                let mut concrete = object.clone();
                concrete.insert("type".to_string(), non_null);
                let metadata: BTreeMap<_, _> = ["description", "title", "default", "examples"]
                    .into_iter()
                    .filter_map(|key| {
                        object
                            .get(key)
                            .cloned()
                            .map(|value| (key.to_string(), value))
                    })
                    .collect();
                object.clear();
                object.extend(metadata);
                object.insert(
                    "oneOf".to_string(),
                    serde_json::json!([concrete, { "type": "null" }]),
                );
            }
            object.values_mut().for_each(normalize_nullable_types);
        }
        _ => {}
    }
}

fn normalize_mcp_elicitation_root(value: &mut serde_json::Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .context("MCP elicitation schema root is not an object")?;
    let definitions = object
        .get_mut("definitions")
        .and_then(serde_json::Value::as_object_mut)
        .context("MCP elicitation schema lost definitions")?;
    for name in [
        "McpElicitationPrimitiveSchema",
        "McpElicitationEnumSchema",
        "McpElicitationMultiSelectEnumSchema",
        "McpElicitationSingleSelectEnumSchema",
    ] {
        let schema = definitions
            .get_mut(name)
            .and_then(serde_json::Value::as_object_mut)
            .with_context(|| format!("MCP elicitation schema lost {name}"))?;
        let variants = schema
            .remove("anyOf")
            .with_context(|| format!("MCP elicitation {name} no longer has anyOf"))?;
        schema.insert("oneOf".to_string(), variants);
    }
    let variants = object
        .remove("oneOf")
        .context("MCP elicitation schema lost root oneOf")?;
    let variants = variants
        .as_array()
        .context("MCP root oneOf is not an array")?;
    let base_required = object
        .remove("required")
        .unwrap_or_else(|| serde_json::json!([]));
    let base_properties = object
        .remove("properties")
        .unwrap_or_else(|| serde_json::json!({}));
    let mut distributed = Vec::new();
    for variant in variants {
        let mut branch = variant.clone();
        let branch_object = branch
            .as_object_mut()
            .context("MCP elicitation branch is not an object")?;
        let mut required = base_required.as_array().cloned().unwrap_or_default();
        required.extend(
            branch_object
                .remove("required")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
        );
        required.sort_by_key(serde_json::Value::to_string);
        required.dedup();
        let mut properties = base_properties.as_object().cloned().unwrap_or_default();
        properties.extend(
            branch_object
                .remove("properties")
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default(),
        );
        branch_object.insert("required".to_string(), serde_json::Value::Array(required));
        branch_object.insert(
            "properties".to_string(),
            serde_json::Value::Object(properties),
        );
        distributed.push(branch);
    }
    object.insert("oneOf".to_string(), serde_json::Value::Array(distributed));
    Ok(())
}

fn select_roots(bundle: &serde_json::Value, roots: &[&str]) -> Result<serde_json::Value> {
    let definitions = bundle["definitions"]
        .as_object()
        .context("v2 bundle has no definitions")?;
    let mut pending = roots.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut selected = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !selected.insert(name.clone()) {
            continue;
        }
        collect_refs(
            definitions
                .get(&name)
                .with_context(|| format!("missing definition {name}"))?,
            &mut pending,
        );
    }
    let defs = selected
        .into_iter()
        .map(|name| (name.clone(), definitions[&name].clone()))
        .collect::<serde_json::Map<_, _>>();
    Ok(serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "CodexConversationRoot",
        "$ref": format!("#/definitions/{}", roots[0]),
        "definitions": defs,
    }))
}

fn collect_refs(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Array(values) => {
            values.iter().for_each(|value| collect_refs(value, output))
        }
        serde_json::Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(serde_json::Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/definitions/"))
            {
                output.push(name.to_string());
            }
            object
                .values()
                .for_each(|value| collect_refs(value, output));
        }
        _ => {}
    }
}

fn collect_ts_closure(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let root = fs::canonicalize(root)?;
    let roots = FLAT_ROOTS
        .iter()
        .chain(REQUEST_ROOTS)
        .map(|name| format!("{name}.ts"))
        .collect::<BTreeSet<_>>();
    let all = walk_files(&root, "ts")?;
    let mut pending = all
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|name| roots.contains(name))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut selected = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !selected.insert(path.clone()) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        for line in text.lines() {
            if let Some(specifier) = line
                .split(" from \"")
                .nth(1)
                .and_then(|tail| tail.split('"').next())
            {
                let candidate = path.parent().unwrap().join(format!("{specifier}.ts"));
                if candidate.exists() {
                    pending.push(fs::canonicalize(candidate)?);
                }
            }
        }
    }
    Ok(selected)
}

fn walk_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|v| v.to_str()) == Some(extension) {
                output.push(path);
            }
        }
    }
    Ok(output)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn canonicalize_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                canonicalize_json(value);
            }
            let sorted = std::mem::take(object)
                .into_iter()
                .collect::<BTreeMap<_, _>>();
            object.extend(sorted);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(canonicalize_json),
        _ => {}
    }
}

fn snake(value: &str) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            output.push('_');
        }
        output.extend(ch.to_lowercase());
    }
    output
}
