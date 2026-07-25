import { api } from "../api/client";
import type {
  ArchiveInteractionDefinitionResponse,
  CanvasDefinitionDto,
  CommitCanvasDefinitionRequest,
  CreateCanvasDefinitionRequest,
  CreateInteractionInstanceRequestDto,
  DistributeCanvasDefinitionRequest,
  InteractionCommandRequestDto,
  InteractionCommandResponseDto,
  InteractionInstanceDto,
  InteractionInstanceViewDto,
  InteractionSourceBundleDto,
} from "../generated/interaction-contracts";

export type CanvasListScope = "all" | "mine" | "shared";

export async function fetchProjectCanvases(projectId: string, scope: CanvasListScope = "all") {
  return api.get<CanvasDefinitionDto[]>(
    `/projects/${encodeURIComponent(projectId)}/interaction-definitions/canvas?scope=${scope}`,
  );
}

export async function createCanvas(projectId: string, input: CreateCanvasDefinitionRequest) {
  return api.post<CanvasDefinitionDto>(
    `/projects/${encodeURIComponent(projectId)}/interaction-definitions/canvas`, input,
  );
}

export async function fetchCanvas(definitionId: string) {
  return api.get<CanvasDefinitionDto>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}`,
  );
}

export async function commitCanvas(definitionId: string, input: CommitCanvasDefinitionRequest) {
  return api.post<CanvasDefinitionDto>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/revisions`, input,
  );
}

export async function archiveCanvas(definitionId: string) {
  return api.post<ArchiveInteractionDefinitionResponse>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/archive`, {},
  );
}

export async function publishCanvasToProject(
  definitionId: string,
  input: DistributeCanvasDefinitionRequest,
) {
  return api.post<CanvasDefinitionDto>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/publish`, input,
  );
}

export async function copyCanvasToPersonal(
  definitionId: string,
  input: DistributeCanvasDefinitionRequest,
) {
  return api.post<CanvasDefinitionDto>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/copy`, input,
  );
}

export async function unpublishCanvas(definitionId: string) {
  return api.post<ArchiveInteractionDefinitionResponse>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/unpublish`, {},
  );
}

export async function createInteractionInstance(
  definitionId: string,
  input: CreateInteractionInstanceRequestDto,
) {
  return api.post<InteractionInstanceViewDto>(
    `/interaction-definitions/${encodeURIComponent(definitionId)}/instances`, input,
  );
}

export async function fetchInteractionInstance(instanceId: string) {
  return api.get<InteractionInstanceViewDto>(
    `/interaction-instances/${encodeURIComponent(instanceId)}`,
  );
}

export async function fetchProjectInteractionInstances(projectId: string) {
  return api.get<InteractionInstanceViewDto[]>(
    `/projects/${encodeURIComponent(projectId)}/interaction-instances`,
  );
}

export async function executeInteractionCommand(
  instanceId: string,
  input: InteractionCommandRequestDto,
) {
  return api.post<InteractionCommandResponseDto>(
    `/interaction-instances/${encodeURIComponent(instanceId)}/commands`, input,
  );
}

export async function closeInteractionInstance(instanceId: string, expectedStateRevision: number) {
  return api.post<InteractionInstanceDto>(
    `/interaction-instances/${encodeURIComponent(instanceId)}/close`,
    { expected_state_revision: expectedStateRevision },
  );
}

export async function createDefaultCanvasSourceBundle(): Promise<InteractionSourceBundleDto> {
  return canonicalSourceBundle({
    format_version: 1,
    entry_file: "index.html",
    files: [{
      path: "index.html",
      content: "<!doctype html><html><body><main><h1>New Canvas</h1></main></body></html>",
      media_type: "text/html",
    }],
    sandbox: { libraries: [], import_map: {} },
    digest: "",
  });
}

export async function canonicalSourceBundle(
  bundle: Omit<InteractionSourceBundleDto, "digest"> & { digest?: string },
): Promise<InteractionSourceBundleDto> {
  const files = [...bundle.files].sort((left, right) => left.path.localeCompare(right.path));
  const libraries = [...new Set(bundle.sandbox.libraries.map((value) => value.trim()))].sort();
  const importMap = Object.fromEntries(
    Object.entries(bundle.sandbox.import_map).sort(([left], [right]) => left.localeCompare(right)),
  );
  const canonical = {
    format_version: 1,
    entry_file: bundle.entry_file,
    files,
    sandbox: { libraries, import_map: importMap },
  };
  const bytes = new TextEncoder().encode(JSON.stringify(canonical));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  return { ...canonical, digest: `sha256:${hex}` };
}
