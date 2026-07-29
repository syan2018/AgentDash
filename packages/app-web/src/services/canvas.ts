import { api } from "../api/client";
import type { JsonValue } from "../generated/common-contracts";
import type {
  ArchiveInteractionDefinitionResponse,
  CanvasAgentSubmitResponseDto,
  CanvasDefinitionDto,
  CanvasRuntimeSnapshotDto,
  CommitCanvasDefinitionRequest,
  CreateCanvasDefinitionRequest,
  DistributeCanvasDefinitionRequest,
  InteractionActionResponseDto,
  InteractionOperationRefDto,
  InteractionInstanceViewDto,
  OperationWorkshopSurfaceDto,
  InteractionRuntimeBindingDto,
  InteractionRuntimeBindingTargetDto,
  InteractionPresentationStateDto,
  InteractionSourceFileChangeDto,
  OperationWorkshopInvokeResponseDto,
} from "../generated/interaction-contracts";
import type { ExtensionPackageInstallationResponse } from "../generated/extension-package-contracts";
import type {
  Canvas,
  CanvasListScope,
  CanvasRuntimeFile,
  CanvasRuntimeSnapshot,
  CopyCanvasToPersonalInput,
  CreateCanvasInput,
  DeleteCanvasResult,
  PublishCanvasToProjectInput,
  UnpublishCanvasResult,
  UpdateCanvasInput,
} from "../types";

export interface PromoteCanvasToExtensionInput {
  extension_key?: string;
  display_name?: string;
  package_version?: string;
  asset_version?: string;
  overwrite?: boolean;
}

export async function fetchProjectCanvases(
  projectId: string,
  scope?: CanvasListScope,
): Promise<Canvas[]> {
  const query = scope ? `?scope=${encodeURIComponent(scope)}` : "";
  const definitions = await api.get<CanvasDefinitionDto[]>(
    `/projects/${encodeURIComponent(projectId)}/interaction-definitions/canvas${query}`,
  );
  return definitions.map(canvasFromDefinition);
}

export async function createCanvas(
  projectId: string,
  input: CreateCanvasInput,
): Promise<Canvas> {
  const request: CreateCanvasDefinitionRequest = {
    canvas_mount_id: input.canvas_mount_id,
    title: input.title,
    description: input.description ?? "",
    initial_state: {},
    state_schema: { type: "object" },
    agent_projection: {
      version: 1,
      allowed_state_paths: [],
    },
    command_definitions: [],
    component_bindings: [],
    action_bindings: [],
    resource_slots: [],
  };
  return canvasFromDefinition(
    await api.post<CanvasDefinitionDto>(
      `/projects/${encodeURIComponent(projectId)}/interaction-definitions/canvas`,
      request,
    ),
  );
}

export async function fetchCanvas(canvasId: string): Promise<Canvas> {
  return canvasFromDefinition(
    await api.get<CanvasDefinitionDto>(
      `/interaction-definitions/${encodeURIComponent(canvasId)}`,
    ),
  );
}

export async function fetchCanvasByMountId(
  projectId: string,
  canvasMountId: string,
): Promise<Canvas> {
  const canvases = await fetchProjectCanvases(projectId, "all");
  const canvas = canvases.find((candidate) => candidate.canvas_mount_id === canvasMountId);
  if (!canvas) {
    throw new Error(`Canvas mount 不存在或不可见: ${canvasMountId}`);
  }
  return canvas;
}

export async function updateCanvas(
  canvasId: string,
  input: UpdateCanvasInput,
): Promise<Canvas> {
  const current = await fetchCanvas(canvasId);
  const nextFiles = input.files ?? current.files;
  const nextPaths = new Set(nextFiles.map((file) => file.path));
  const fileChanges: InteractionSourceFileChangeDto[] = [
    ...current.files
      .filter((file) => !nextPaths.has(file.path))
      .map((file): InteractionSourceFileChangeDto => ({
        kind: "delete",
        path: file.path,
      })),
    ...nextFiles.map((file): InteractionSourceFileChangeDto => ({
      kind: "upsert",
      file,
    })),
  ];
  const request: CommitCanvasDefinitionRequest = {
    base_revision_id: current.current_revision_id,
    title: input.title,
    description: input.description,
    changeset: {
      entry_file: input.entry_file,
      sandbox: input.sandbox_config
        ? {
            libraries: input.sandbox_config.libraries,
            import_map: input.sandbox_config.import_map.imports,
          }
        : undefined,
      file_changes: fileChanges,
    },
  };
  return canvasFromDefinition(
    await api.post<CanvasDefinitionDto>(
      `/interaction-definitions/${encodeURIComponent(canvasId)}/revisions`,
      request,
    ),
  );
}

export async function deleteCanvas(canvasId: string): Promise<DeleteCanvasResult> {
  return api.post<ArchiveInteractionDefinitionResponse>(
    `/interaction-definitions/${encodeURIComponent(canvasId)}/archive`,
    {},
  );
}

export async function publishCanvasToProject(
  canvasId: string,
  input: PublishCanvasToProjectInput = {},
): Promise<Canvas> {
  const current = await fetchCanvas(canvasId);
  const request: DistributeCanvasDefinitionRequest = {
    source_revision_id: current.current_revision_id,
    title: input.title,
    description: input.description,
  };
  return canvasFromDefinition(
    await api.post<CanvasDefinitionDto>(
      `/interaction-definitions/${encodeURIComponent(canvasId)}/publish`,
      request,
    ),
  );
}

export async function copyCanvasToPersonal(
  canvasId: string,
  input: CopyCanvasToPersonalInput = {},
): Promise<Canvas> {
  const current = await fetchCanvas(canvasId);
  const request: DistributeCanvasDefinitionRequest = {
    source_revision_id: current.current_revision_id,
    title: input.title,
    description: input.description,
  };
  return canvasFromDefinition(
    await api.post<CanvasDefinitionDto>(
      `/interaction-definitions/${encodeURIComponent(canvasId)}/copy`,
      request,
    ),
  );
}

export async function unpublishCanvas(canvasId: string): Promise<UnpublishCanvasResult> {
  return api.post<ArchiveInteractionDefinitionResponse>(
    `/interaction-definitions/${encodeURIComponent(canvasId)}/unpublish`,
    {},
  );
}

export async function fetchCanvasRuntimeSnapshot(
  canvasId: string,
): Promise<CanvasRuntimeSnapshot> {
  const [snapshot, canvas] = await Promise.all([
    api.get<CanvasRuntimeSnapshotDto>(
    `/interaction-definitions/${encodeURIComponent(canvasId)}/runtime-snapshot`,
    ),
    fetchCanvas(canvasId),
  ]);
  return runtimeSnapshotFromDto(snapshot, canvas.project_id);
}

export async function fetchInteractionCanvasRuntimeSnapshot(
  instanceId: string,
  agentRunTarget?: {
    runId: string;
    agentId: string;
  } | null,
): Promise<CanvasRuntimeSnapshot> {
  const targetQuery = agentRunTarget
    ? `?run_id=${encodeURIComponent(agentRunTarget.runId)}`
      + `&agent_id=${encodeURIComponent(agentRunTarget.agentId)}`
    : "";
  const view = await api.get<InteractionInstanceViewDto>(
    `/interaction-instances/${encodeURIComponent(instanceId)}${targetQuery}`,
  );
  const canvas = await fetchCanvas(view.instance.definition_id);
  const [snapshot, surface] = await Promise.all([
    api.get<CanvasRuntimeSnapshotDto>(
      `/interaction-definitions/${encodeURIComponent(view.instance.definition_id)}/runtime-snapshot`,
    ),
    api.post<OperationWorkshopSurfaceDto>(
      `/projects/${encodeURIComponent(canvas.project_id)}/operation-workshop/surface`,
      {
        context: {
          kind: "interaction",
          instance_id: instanceId,
        },
      },
    ),
  ]);
  return {
    ...runtimeSnapshotFromDto(snapshot, canvas.project_id),
    interaction_instance_id: instanceId,
    interaction_state: view.instance.state,
    interaction_state_revision: Number(view.instance.state_revision),
    bindings: view.runtime_bindings.map((binding) => {
      const sourceUri = runtimeBindingSource(binding.target);
      return {
        alias: binding.slot_key,
        source_uri: sourceUri,
        data_path: bindingFilePath(binding.slot_key, sourceUri),
        content_type: binding.target.kind,
        resolved: false,
      };
    }),
    operations: surface.operations,
    features: {
      ...snapshot.features,
      interaction: true,
    },
  };
}

export async function upsertInteractionRuntimeBinding(input: {
  instanceId: string;
  slotKey: string;
  target: InteractionRuntimeBindingTargetDto;
}): Promise<InteractionRuntimeBindingDto> {
  return api.put<InteractionRuntimeBindingDto>(
    `/interaction-instances/${encodeURIComponent(input.instanceId)}`
      + `/runtime-bindings/${encodeURIComponent(input.slotKey)}`,
    { target: input.target },
  );
}

export async function submitCanvasAgentInput(input: {
  instanceId: string;
  runId: string;
  agentId: string;
  clientCommandId: string;
  content: import("../generated/agent-runtime-contracts").AgentInputContent[];
  includeInteractionState: boolean;
  includeRenderObservation: boolean;
}): Promise<CanvasAgentSubmitResponseDto> {
  return api.post<CanvasAgentSubmitResponseDto>(
    `/interaction-instances/${encodeURIComponent(input.instanceId)}/agent-submit`,
    {
      run_id: input.runId,
      agent_id: input.agentId,
      client_command_id: input.clientCommandId,
      input: input.content,
      include_interaction_state: input.includeInteractionState,
      include_render_observation: input.includeRenderObservation,
    },
  );
}

export async function persistCanvasRendererObservation(input: {
  instanceId: string;
  frameId: string;
  generation: number;
  observation: JsonValue;
}): Promise<InteractionPresentationStateDto> {
  const presentationKey = "canvas.renderer-observation";
  const query = `?presentation_key=${encodeURIComponent(presentationKey)}`;
  const current = await api.get<InteractionPresentationStateDto | null>(
    `/interaction-instances/${encodeURIComponent(input.instanceId)}/presentation${query}`,
  );
  return api.put<InteractionPresentationStateDto>(
    `/interaction-instances/${encodeURIComponent(input.instanceId)}/presentation`,
    {
      presentation_key: presentationKey,
      value: {
        frame_id: input.frameId,
        generation: input.generation,
        observation: input.observation,
      },
      expected_revision: current?.revision,
    },
  );
}

export async function invokeCanvasOperation(input: {
  projectId: string;
  definitionId: string;
  instanceId?: string;
  operationRef: InteractionOperationRefDto;
  value?: JsonValue;
  idempotencyKey?: string;
}): Promise<unknown> {
  const response = await api.post<OperationWorkshopInvokeResponseDto>(
    `/projects/${encodeURIComponent(input.projectId)}/operation-workshop/invoke`,
    {
      context: {
        ...(input.instanceId
          ? { kind: "interaction" as const, instance_id: input.instanceId }
          : { kind: "canvas" as const, definition_id: input.definitionId }),
      },
      operation_ref: input.operationRef,
      input: input.value ?? {},
      idempotency_key: input.idempotencyKey,
    },
  );
  return unwrapOperationResult(response.result);
}

export async function executeCanvasAction(input: {
  instanceId: string;
  actionKey: string;
  payload?: JsonValue;
  expectedStateRevision: number;
  agentRunTarget?: {
    runId: string;
    agentId: string;
  } | null;
}): Promise<InteractionActionResponseDto> {
  return api.post<InteractionActionResponseDto>(
    `/interaction-instances/${encodeURIComponent(input.instanceId)}/actions`,
    {
      command_id: crypto.randomUUID(),
      action_key: input.actionKey,
      payload: input.payload ?? {},
      expected_state_revision: input.expectedStateRevision,
      run_id: input.agentRunTarget?.runId,
      agent_id: input.agentRunTarget?.agentId,
    },
  );
}

export async function promoteCanvasToExtension(
  canvasId: string,
  input: PromoteCanvasToExtensionInput = {},
): Promise<ExtensionPackageInstallationResponse> {
  const current = await fetchCanvas(canvasId);
  return api.post<ExtensionPackageInstallationResponse>(
    `/interaction-definitions/${encodeURIComponent(canvasId)}/promote-extension`,
    {
      source_revision_id: current.current_revision_id,
      ...input,
    },
  );
}

function canvasFromDefinition(definition: CanvasDefinitionDto): Canvas {
  const scope = definition.owner.kind === "user" ? "personal" : "project";
  return {
    canvas_id: definition.definition_id,
    canvas_mount_id: definition.canvas_mount_id,
    vfs_mount_id: definition.canvas_mount_id,
    project_id: definition.project_id,
    scope,
    access: definition.access,
    title: definition.title,
    description: definition.description,
    entry_file: definition.source_bundle.entry_file,
    files: definition.source_bundle.files,
    sandbox_config: {
      libraries: definition.source_bundle.sandbox.libraries,
      import_map: {
        imports: Object.fromEntries(
          Object.entries(definition.source_bundle.sandbox.import_map).filter(
            (entry): entry is [string, string] => typeof entry[1] === "string",
          ),
        ),
      },
    },
    current_revision_id: definition.current_revision_id,
    source_bundle_digest: definition.source_bundle.digest,
    created_at: definition.created_at,
    updated_at: definition.updated_at,
    definition,
  };
}

function runtimeSnapshotFromDto(
  snapshot: CanvasRuntimeSnapshotDto,
  projectId: string,
): CanvasRuntimeSnapshot {
  return {
    project_id: projectId,
    canvas_id: snapshot.definition_id,
    definition_revision_id: snapshot.definition_revision_id,
    canvas_mount_id: snapshot.canvas_mount_id,
    vfs_mount_id: snapshot.canvas_mount_id,
    entry: snapshot.source_bundle.entry_file,
    files: snapshot.source_bundle.files.map(runtimeFile),
    bindings: [],
    import_map: {
      imports: Object.fromEntries(
        Object.entries(snapshot.source_bundle.sandbox.import_map).filter(
          (entry): entry is [string, string] => typeof entry[1] === "string",
        ),
      ),
    },
    libraries: snapshot.source_bundle.sandbox.libraries,
    operations: snapshot.operations,
    features: snapshot.features,
  };
}

function runtimeBindingSource(
  target: InteractionInstanceViewDto["runtime_bindings"][number]["target"],
): string {
  switch (target.kind) {
    case "resource":
      return target.resource_ref;
    case "artifact":
      return target.artifact_ref;
    case "provider":
      return target.provider_ref;
  }
}

function bindingFilePath(slotKey: string, sourceUri: string): string {
  const path = sourceUri.split("://", 2)[1] ?? "";
  const extension = path.match(/\.(json|csv|md|html|css|js|svg|ya?ml|xml|txt)$/i)?.[0]
    ?.toLowerCase() ?? ".json";
  return `bindings/${slotKey}${extension}`;
}

function runtimeFile(file: CanvasDefinitionDto["source_bundle"]["files"][number]): CanvasRuntimeFile {
  const path = file.path.toLowerCase();
  const fileType = path.endsWith(".css")
    ? "style"
    : path.endsWith(".json")
      ? "data"
      : "code";
  return { ...file, file_type: fileType };
}

function unwrapOperationResult(result: JsonValue): unknown {
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    return result;
  }
  const record = result as Record<string, unknown>;
  const value = record.value;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value ?? result;
  }
  const valueRecord = value as Record<string, unknown>;
  if (valueRecord.kind === "inline") {
    return valueRecord.value;
  }
  return value;
}
