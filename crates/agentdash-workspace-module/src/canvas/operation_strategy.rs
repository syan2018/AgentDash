use std::sync::Arc;

use agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::interaction::{
    DefinitionLineage, DefinitionLineageKind, InteractionDefinitionRepository,
    InteractionDefinitionRevision, InteractionOwner, SourceBundle, SourceFile, SourceSandboxConfig,
    canvas_authoring_mount_id, normalize_canvas_authoring_mount_id,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::mount_surface::{
    CanvasMountIntentKind, CanvasMountMaterializationPort, CanvasMountMaterializationRequest,
};

#[derive(Clone)]
pub struct CanvasOperationStrategyDeps {
    pub definitions: Arc<dyn InteractionDefinitionRepository>,
    pub mounts: Arc<dyn CanvasMountMaterializationPort>,
}

pub struct CanvasOperationContext<'a> {
    pub project_id: uuid::Uuid,
    pub user_id: &'a str,
    pub agent_target: Option<&'a AgentRunTarget>,
    pub idempotency_key: &'a str,
    pub visible_definitions: &'a [InteractionDefinitionRevision],
}

pub struct CanvasOperationResult {
    pub action: &'static str,
    pub revision: InteractionDefinitionRevision,
}

#[async_trait]
trait CanvasOperationStrategy: Send + Sync {
    fn operation(&self) -> &'static str;

    async fn execute(
        &self,
        context: CanvasOperationContext<'_>,
        input: Value,
    ) -> Result<CanvasOperationResult, ProductRuntimeToolOutcome>;
}

pub struct CanvasOperationStrategies {
    strategies: Vec<Box<dyn CanvasOperationStrategy>>,
}

impl CanvasOperationStrategies {
    pub fn new(deps: CanvasOperationStrategyDeps) -> Self {
        Self {
            strategies: vec![
                Box::new(CreateCanvasStrategy { deps: deps.clone() }),
                Box::new(AttachCanvasStrategy { deps: deps.clone() }),
                Box::new(CopyCanvasStrategy { deps }),
            ],
        }
    }

    pub async fn execute(
        &self,
        operation: &str,
        context: CanvasOperationContext<'_>,
        input: Value,
    ) -> Result<CanvasOperationResult, ProductRuntimeToolOutcome> {
        let Some(strategy) = self
            .strategies
            .iter()
            .find(|strategy| strategy.operation() == operation)
        else {
            return Err(rejected(
                "unsupported_workspace_module_operation",
                format!(
                    "workspace_module_operate 不支持 operation `{operation}`；支持 canvas.create、canvas.attach、canvas.copy"
                ),
            ));
        };
        strategy.execute(context, input).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCanvasArguments {
    #[serde(default)]
    canvas_mount_id: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
}

struct CreateCanvasStrategy {
    deps: CanvasOperationStrategyDeps,
}

#[async_trait]
impl CanvasOperationStrategy for CreateCanvasStrategy {
    fn operation(&self) -> &'static str {
        "canvas.create"
    }

    async fn execute(
        &self,
        context: CanvasOperationContext<'_>,
        input: Value,
    ) -> Result<CanvasOperationResult, ProductRuntimeToolOutcome> {
        let input: CreateCanvasArguments = serde_json::from_value(input).map_err(|error| {
            rejected(
                "workspace_module_invalid_arguments",
                format!("invalid canvas.create input: {error}"),
            )
        })?;
        let title = input.title.trim();
        if title.is_empty() {
            return Err(rejected(
                "workspace_module_invalid_arguments",
                "title is required for canvas.create",
            ));
        }
        let definition_id = stable_canvas_definition_id(
            self.operation(),
            context.project_id,
            context.agent_target,
            context.idempotency_key,
        );
        let revision = if let Some(existing) = self
            .deps
            .definitions
            .get(definition_id)
            .await
            .map_err(definition_query_failed)?
        {
            self.deps
                .definitions
                .get_revision(existing.current_revision_id)
                .await
                .map_err(definition_query_failed)?
                .ok_or_else(|| {
                    failed(
                        "workspace_module_definition_revision_missing",
                        "idempotent Canvas definition revision is missing",
                    )
                })?
        } else {
            let mount_id = requested_or_default_mount_id(
                input.canvas_mount_id,
                definition_id,
                context.visible_definitions,
            )?;
            let mut revision = InteractionDefinitionRevision::new_canvas_v1(
                definition_id,
                1,
                context.project_id,
                InteractionOwner::User(context.user_id.to_owned()),
                title,
                input.description.unwrap_or_default(),
                default_canvas_source_bundle()?,
                json!({}),
                json!({"type": "object"}),
                context.user_id,
            )
            .map_err(definition_invalid)?;
            revision.authoring_mount_id = mount_id;
            revision.validate().map_err(definition_invalid)?;
            create_definition(self.deps.definitions.as_ref(), revision).await?
        };
        materialize(
            self.deps.mounts.as_ref(),
            context.agent_target,
            context.idempotency_key,
            &revision,
            CanvasMountIntentKind::Create,
            "create",
        )
        .await?;
        Ok(CanvasOperationResult {
            action: "created",
            revision,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachCanvasArguments {
    canvas_mount_id: String,
}

struct AttachCanvasStrategy {
    deps: CanvasOperationStrategyDeps,
}

#[async_trait]
impl CanvasOperationStrategy for AttachCanvasStrategy {
    fn operation(&self) -> &'static str {
        "canvas.attach"
    }

    async fn execute(
        &self,
        context: CanvasOperationContext<'_>,
        input: Value,
    ) -> Result<CanvasOperationResult, ProductRuntimeToolOutcome> {
        let input: AttachCanvasArguments = serde_json::from_value(input).map_err(|error| {
            rejected(
                "workspace_module_invalid_arguments",
                format!("invalid canvas.attach input: {error}"),
            )
        })?;
        let revision =
            visible_canvas_by_mount(context.visible_definitions, input.canvas_mount_id.trim())?
                .clone();
        materialize(
            self.deps.mounts.as_ref(),
            context.agent_target,
            context.idempotency_key,
            &revision,
            CanvasMountIntentKind::Attach,
            "attach",
        )
        .await?;
        Ok(CanvasOperationResult {
            action: "attached",
            revision,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyCanvasArguments {
    source_mount_id: String,
    #[serde(default)]
    canvas_mount_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

struct CopyCanvasStrategy {
    deps: CanvasOperationStrategyDeps,
}

#[async_trait]
impl CanvasOperationStrategy for CopyCanvasStrategy {
    fn operation(&self) -> &'static str {
        "canvas.copy"
    }

    async fn execute(
        &self,
        context: CanvasOperationContext<'_>,
        input: Value,
    ) -> Result<CanvasOperationResult, ProductRuntimeToolOutcome> {
        let input: CopyCanvasArguments = serde_json::from_value(input).map_err(|error| {
            rejected(
                "workspace_module_invalid_arguments",
                format!("invalid canvas.copy input: {error}"),
            )
        })?;
        let source =
            visible_canvas_by_mount(context.visible_definitions, input.source_mount_id.trim())?
                .clone();
        let definition_id = stable_canvas_definition_id(
            self.operation(),
            context.project_id,
            context.agent_target,
            context.idempotency_key,
        );
        let revision = if let Some(existing) = self
            .deps
            .definitions
            .get(definition_id)
            .await
            .map_err(definition_query_failed)?
        {
            self.deps
                .definitions
                .get_revision(existing.current_revision_id)
                .await
                .map_err(definition_query_failed)?
                .ok_or_else(|| {
                    failed(
                        "workspace_module_definition_revision_missing",
                        "idempotent Canvas copy revision is missing",
                    )
                })?
        } else {
            let mount_id = requested_or_default_mount_id(
                input.canvas_mount_id,
                definition_id,
                context.visible_definitions,
            )?;
            let mut revision = source.clone();
            revision.definition_id = definition_id;
            revision.revision_id = uuid::Uuid::new_v4();
            revision.revision_number = 1;
            revision.owner = InteractionOwner::User(context.user_id.to_owned());
            revision.authoring_mount_id = mount_id;
            revision.title = input.title.unwrap_or(revision.title);
            revision.description = input.description.unwrap_or(revision.description);
            revision.lineage = Some(DefinitionLineage {
                kind: DefinitionLineageKind::CopiedFrom,
                source_definition_id: source.definition_id,
                source_revision_id: source.revision_id,
                source_bundle_digest: source.source_bundle.digest.clone(),
            });
            revision.created_by = context.user_id.to_owned();
            revision.created_at = chrono::Utc::now();
            revision.validate().map_err(definition_invalid)?;
            create_definition(self.deps.definitions.as_ref(), revision).await?
        };
        materialize(
            self.deps.mounts.as_ref(),
            context.agent_target,
            context.idempotency_key,
            &revision,
            CanvasMountIntentKind::Create,
            "copy",
        )
        .await?;
        Ok(CanvasOperationResult {
            action: "copied",
            revision,
        })
    }
}

async fn create_definition(
    definitions: &dyn InteractionDefinitionRepository,
    revision: InteractionDefinitionRevision,
) -> Result<InteractionDefinitionRevision, ProductRuntimeToolOutcome> {
    let (definition, revision) = revision
        .into_initial_definition()
        .map_err(definition_invalid)?;
    definitions
        .create(&definition, &revision)
        .await
        .map_err(|error| {
            failed(
                "workspace_module_canvas_definition_create_failed",
                error.to_string(),
            )
        })?;
    Ok(revision)
}

async fn materialize(
    mounts: &dyn CanvasMountMaterializationPort,
    target: Option<&AgentRunTarget>,
    idempotency_key: &str,
    revision: &InteractionDefinitionRevision,
    intent: CanvasMountIntentKind,
    action: &str,
) -> Result<(), ProductRuntimeToolOutcome> {
    let Some(target) = target else {
        return Ok(());
    };
    mounts
        .materialize(CanvasMountMaterializationRequest {
            target: target.clone(),
            definition_id: revision.definition_id,
            definition_revision_id: revision.revision_id,
            intent,
            idempotency_key: format!("workspace-module:{action}:{}", idempotency_key),
        })
        .await
        .map(|_| ())
        .map_err(|error| failed("workspace_module_canvas_mount_failed", error.to_string()))
}

fn stable_canvas_definition_id(
    operation: &str,
    project_id: uuid::Uuid,
    target: Option<&AgentRunTarget>,
    idempotency_key: &str,
) -> uuid::Uuid {
    let actor = target
        .map(|target| format!("agent:{}:{}", target.run_id, target.agent_id))
        .unwrap_or_else(|| "user".to_owned());
    let digest = Sha256::digest(
        format!("agentdash.canvas-operation/v1:{operation}:{project_id}:{actor}:{idempotency_key}")
            .as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}

fn requested_or_default_mount_id(
    requested: Option<String>,
    definition_id: uuid::Uuid,
    definitions: &[InteractionDefinitionRevision],
) -> Result<String, ProductRuntimeToolOutcome> {
    let mount_id = requested
        .map(|value| normalize_canvas_authoring_mount_id(&value))
        .transpose()
        .map_err(|error| {
            rejected(
                "workspace_module_invalid_canvas_mount_id",
                error.to_string(),
            )
        })?
        .unwrap_or_else(|| canvas_authoring_mount_id(definition_id));
    if definitions
        .iter()
        .any(|revision| revision.authoring_mount_id == mount_id)
    {
        return Err(rejected(
            "workspace_module_canvas_mount_conflict",
            format!("Canvas authoring mount 已存在: {mount_id}"),
        ));
    }
    Ok(mount_id)
}

fn visible_canvas_by_mount<'a>(
    definitions: &'a [InteractionDefinitionRevision],
    mount_id: &str,
) -> Result<&'a InteractionDefinitionRevision, ProductRuntimeToolOutcome> {
    let mount_id = normalize_canvas_authoring_mount_id(mount_id).map_err(|error| {
        rejected(
            "workspace_module_invalid_canvas_mount_id",
            error.to_string(),
        )
    })?;
    definitions
        .iter()
        .find(|revision| revision.authoring_mount_id == mount_id)
        .ok_or_else(|| {
            rejected(
                "workspace_module_canvas_not_visible",
                format!("Canvas 不存在或当前 Agent 不可见: {mount_id}"),
            )
        })
}

fn default_canvas_source_bundle() -> Result<SourceBundle, ProductRuntimeToolOutcome> {
    SourceBundle::new(
        "src/main.tsx",
        vec![
            SourceFile::new(
                "src/main.tsx",
                r#"const root = document.getElementById("root");

if (!root) {
  throw new Error("Canvas root element not found");
}

root.innerHTML = `
  <section style="font-family: sans-serif; padding: 16px;">
    <h1 style="margin: 0 0 8px;">Live Canvas Ready</h1>
    <p style="margin: 0; color: #475569;">
      Start editing <code>src/main.tsx</code> to render your canvas.
    </p>
  </section>
`;
"#,
                Some("text/typescript".to_owned()),
            )
            .map_err(|error| {
                failed(
                    "workspace_module_default_canvas_source_invalid",
                    error.to_string(),
                )
            })?,
        ],
        SourceSandboxConfig {
            libraries: vec!["react".to_owned(), "react-dom/client".to_owned()],
            import_map: std::collections::BTreeMap::from([
                ("react".to_owned(), "https://esm.sh/react@18?dev".to_owned()),
                (
                    "react-dom/client".to_owned(),
                    "https://esm.sh/react-dom@18/client?dev".to_owned(),
                ),
            ]),
        },
    )
    .map_err(|error| {
        failed(
            "workspace_module_default_canvas_source_invalid",
            error.to_string(),
        )
    })
}

fn definition_query_failed(
    error: agentdash_domain::interaction::InteractionError,
) -> ProductRuntimeToolOutcome {
    failed(
        "workspace_module_definition_query_failed",
        error.to_string(),
    )
}

fn definition_invalid(
    error: agentdash_domain::interaction::InteractionError,
) -> ProductRuntimeToolOutcome {
    rejected(
        "workspace_module_canvas_definition_invalid",
        error.to_string(),
    )
}

fn rejected(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

fn failed(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Failed {
        code: code.into(),
        message: message.into(),
    }
}
