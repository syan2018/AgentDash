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
use sha2::{Digest, Sha256};
use typify::{TypeSpace, TypeSpaceSettings};

const CODEX_VERSION: &str = "0.144.1";
const CODEX_TAG: &str = "rust-v0.144.1";
const CODEX_COMMIT: &str = "44918ea1";
const TYPIFY_VERSION: &str = "0.7.0";
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
        bail!("usage: agentdash-agent-protocol-codegen [write|check]");
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
            "packages/app-web/src/generated/codex-app-server-protocol",
            "ts",
            &[][..],
        ),
        ("schemas/upstream", "json", &[][..]),
        (
            "crates/agentdash-agent-protocol/src/generated",
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
    let mcp_path = json_dir.join("McpServerElicitationRequestParams.json");
    let mut mcp_schema: serde_json::Value = serde_json::from_slice(&fs::read(&mcp_path)?)?;
    canonicalize_json(&mut mcp_schema);
    let mcp_bytes = format!("{}\n", serde_json::to_string_pretty(&mcp_schema)?).into_bytes();
    let mut modules = Vec::new();
    for root_name in FLAT_ROOTS {
        modules.push((
            snake(root_name),
            generate_rust(select_roots(&flat, &[*root_name])?, root_name)?,
        ));
    }
    for root_name in REQUEST_ROOTS {
        let path = json_dir.join(format!("{root_name}.json"));
        let mut schema: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        if *root_name == "McpServerElicitationRequestParams" {
            normalize_mcp_elicitation_root(&mut schema)?;
        }
        modules.push((snake(root_name), generate_rust(schema, root_name)?));
    }
    let rust = render_rust(modules)?;

    let mut output = BTreeMap::new();
    output.insert(
        PathBuf::from("schemas/upstream/codex-app-server-v2.schemas.json"),
        flat_bytes.clone(),
    );
    output.insert(
        PathBuf::from("schemas/upstream/McpServerElicitationRequestParams.json"),
        mcp_bytes.clone(),
    );
    output.insert(
        PathBuf::from("crates/agentdash-agent-protocol/src/generated/codex_v2.rs"),
        rust.into_bytes(),
    );
    let ts_files = collect_ts_closure(&ts_dir)?;
    let canonical_ts_dir = fs::canonicalize(&ts_dir)?;
    for source in ts_files {
        let relative = source.strip_prefix(&canonical_ts_dir)?;
        let mut bytes = fs::read(&source)?;
        apply_owned_ts_overlays(relative, &mut bytes)?;
        output.insert(
            PathBuf::from("packages/app-web/src/generated/codex-app-server-protocol")
                .join(relative),
            bytes,
        );
    }
    let lock = serde_json::json!({
        "codex_crate_version": CODEX_VERSION,
        "codex_git_tag": CODEX_TAG,
        "codex_commit": CODEX_COMMIT,
        "experimental_api": false,
        "upstream_v2_schema_sha256": sha256(&flat_bytes),
        "mcp_elicitation_schema_sha256": sha256(&mcp_bytes),
        "root_types": FLAT_ROOTS.iter().chain(REQUEST_ROOTS).collect::<Vec<_>>(),
        "typify_version": TYPIFY_VERSION,
        "schema_overrides": [{ "id": MCP_OVERRIDE_ID, "schema": "McpServerElicitationRequestParams" }],
        "owned_nullable_overlays": [{
            "id": NULLABLE_OVERLAY_ID,
            "codex_source": "app-server-protocol/src/protocol/v2/item.rs + mcp.rs@rust-v0.144.1",
            "thread_item_paths": THREAD_ITEM_NULLABLE_PATHS,
            "mcp_elicitation_fields": ["turnId", "_meta"],
            "wire_policy": "accept omitted or null; serialize canonical explicit null"
        }],
    });
    output.insert(
        PathBuf::from("crates/agentdash-agent-protocol/protocol-codegen.lock.json"),
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
    let mut source = String::from("// @generated by agentdash-agent-protocol-codegen.\n");
    for (name, tokens) in modules {
        let mut module = rustfmt(&format!("pub mod {name} {{ {tokens} }}\n"))?;
        let null_fields = match name.as_str() {
            "thread_item" => THREAD_ITEM_NULLABLE_PATHS
                .iter()
                .map(|path| path.split_once('.').expect("qualified nullable path").1)
                .map(snake)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "mcp_server_elicitation_request_params" => {
                vec!["meta".to_string(), "turn_id".to_string()]
            }
            "command_execution_request_approval_params" => {
                vec!["environment_id".to_string()]
            }
            _ => Vec::new(),
        };
        preserve_explicit_null_fields(&mut module, &null_fields);
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
