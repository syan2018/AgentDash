import { expect, test, type APIRequestContext } from "@playwright/test";
import { createHash } from "node:crypto";

import { cleanupE2eProjects, trackE2eProject } from "./_helpers/project-cleanup";

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const REPO_ROOT = (process.env.PLAYWRIGHT_E2E_ROOT ?? process.cwd()).replace(/\\/g, "/");
const NORMALIZED_REPO_ROOT = normalizeComparablePath(REPO_ROOT);
const PLAYWRIGHT_BACKEND_ID = process.env.PLAYWRIGHT_BACKEND_ID ?? "e2e-local";

interface BackendConfig {
  backend_id?: string;
  workspace_roots?: string[];
}

interface ProjectEntity {
  id: string;
  name: string;
  description: string;
  config?: {
    default_agent_type?: string | null;
    default_workspace_id?: string | null;
    agent_presets?: Array<unknown>;
  };
}

interface WorkspaceEntity {
  id: string;
  bindings: Array<{
    backend_id: string;
    root_ref: string;
  }>;
}

interface ProjectAgentEntity {
  id: string;
}

interface ProjectAgentLaunchResult {
  runtime_session_ref?: {
    runtime_session_id?: string;
  };
}

interface CanvasDefinitionEntity {
  definition_id: string;
  current_revision_id: string;
  title: string;
  source_bundle: {
    digest: string;
    entry_file: string;
  };
}

interface PromoteInteractionDefinitionResult {
  definition_id: string;
  definition_revision_id: string;
  source_bundle_digest: string;
  extension_key: string;
  extension_id: string;
  archive_digest: string;
}

interface ExtensionRuntimeProjection {
  workspace_tabs?: Array<{
    type_id?: string;
    extension_key?: string;
    label?: string;
    renderer?: {
      kind?: string;
      entry?: string;
    };
  }>;
}

interface SourceBundle {
  format_version: number;
  entry_file: string;
  files: Array<{ path: string; content: string; media_type?: string }>;
  sandbox: { libraries: string[]; import_map: Record<string, string> };
  digest: string;
}

function canonicalSourceBundle(
  input: Omit<SourceBundle, "digest">,
): SourceBundle {
  const files = [...input.files].sort((left, right) => left.path.localeCompare(right.path));
  const libraries = [...new Set(input.sandbox.libraries.map((value) => value.trim()))].sort();
  const importMap = Object.fromEntries(
    Object.entries(input.sandbox.import_map).sort(([left], [right]) => left.localeCompare(right)),
  );
  const canonical = {
    format_version: 1,
    entry_file: input.entry_file,
    files,
    sandbox: { libraries, import_map: importMap },
  };
  const digest = createHash("sha256").update(JSON.stringify(canonical)).digest("hex");
  return { ...canonical, digest: `sha256:${digest}` };
}

async function ensureBackend(request: APIRequestContext): Promise<void> {
  const onlineResp = await request.get(`${API_ORIGIN}/backends/online`);
  expect(onlineResp.ok()).toBeTruthy();
  const onlineBackends = (await onlineResp.json()) as BackendConfig[];
  const backend = onlineBackends.find((item) => item.backend_id === PLAYWRIGHT_BACKEND_ID);
  expect(backend, `未找到在线 E2E backend: ${PLAYWRIGHT_BACKEND_ID}`).toBeTruthy();

  const workspaceRoots = backend?.workspace_roots ?? [];
  expect(
    workspaceRoots.some((root) => NORMALIZED_REPO_ROOT.startsWith(normalizeComparablePath(root))),
    `E2E backend 未暴露当前仓库根目录: ${REPO_ROOT}`,
  ).toBeTruthy();
}

async function createProject(request: APIRequestContext, suffix: string): Promise<ProjectEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects`, {
    data: {
      name: `E2E Canvas Promote 项目 ${suffix}`,
      description: "用于验证 Canvas promoted extension 金线",
      config: {
        default_agent_type: "codex",
      },
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return trackE2eProject((await resp.json()) as ProjectEntity);
}

async function createWorkspace(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<WorkspaceEntity> {
  await grantProjectBackendAccess(request, projectId);
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/workspaces`, {
    data: {
      name: `E2E Canvas Promote Workspace ${suffix}`,
      shortcut_binding: {
        backend_id: PLAYWRIGHT_BACKEND_ID,
        root_ref: REPO_ROOT,
      },
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  const workspace = (await resp.json()) as WorkspaceEntity;
  expect(workspace.bindings[0]?.backend_id).toBe(PLAYWRIGHT_BACKEND_ID);
  expect(workspace.bindings[0]?.root_ref).toBe(REPO_ROOT);
  return workspace;
}

async function grantProjectBackendAccess(
  request: APIRequestContext,
  projectId: string,
): Promise<void> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/backend-access`, {
    data: {
      backend_id: PLAYWRIGHT_BACKEND_ID,
      priority: 0,
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
}

async function updateProjectDefaultWorkspace(
  request: APIRequestContext,
  project: ProjectEntity,
  workspaceId: string,
): Promise<void> {
  const resp = await request.put(`${API_ORIGIN}/projects/${project.id}`, {
    data: {
      name: project.name,
      description: project.description,
      config: {
        default_agent_type: project.config?.default_agent_type ?? "codex",
        default_workspace_id: workspaceId,
        agent_presets: project.config?.agent_presets ?? [],
      },
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
}

async function createProjectAgent(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<ProjectAgentEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/agents`, {
    data: {
      name: `canvas-promote-${suffix}`,
      agent_type: "codex",
      config: {},
      is_default_for_story: false,
      is_default_for_task: false,
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return (await resp.json()) as ProjectAgentEntity;
}

async function launchProjectAgentRuntime(
  request: APIRequestContext,
  projectId: string,
  agentId: string,
): Promise<string> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/agents/${agentId}/launch`, {
    data: {},
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  const result = (await resp.json()) as ProjectAgentLaunchResult;
  const sessionId = result.runtime_session_ref?.runtime_session_id ?? "";
  expect(sessionId).not.toBe("");
  return sessionId;
}

async function createPromotableCanvas(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<CanvasDefinitionEntity> {
  const title = `Promoted Canvas ${suffix}`;
  const sourceBundle = canonicalSourceBundle({
    format_version: 1,
    entry_file: "index.html",
    files: [{
      path: "index.html",
      content: [
        "<!doctype html>",
        "<html><body>",
        `<main data-testid="promoted-canvas"><h1>Canvas Extension Ready ${suffix}</h1></main>`,
        "</body></html>",
      ].join("\n"),
      media_type: "text/html",
    }],
    sandbox: {
      libraries: [],
      import_map: {},
    },
  });
  const resp = await request.post(
    `${API_ORIGIN}/projects/${projectId}/interaction-definitions/canvas`,
    {
    data: {
      title,
      description: "E2E Canvas promoted extension",
      source_bundle: sourceBundle,
      initial_state: {},
      state_schema: { type: "object" },
      command_definitions: [],
      component_bindings: [],
      resource_slots: [],
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return (await resp.json()) as CanvasDefinitionEntity;
}

async function promoteCanvas(
  request: APIRequestContext,
  canvas: CanvasDefinitionEntity,
): Promise<PromoteInteractionDefinitionResult> {
  const resp = await request.post(
    `${API_ORIGIN}/interaction-definitions/${canvas.definition_id}/promote-extension`,
    {
    data: {
      source_revision_id: canvas.current_revision_id,
      package_version: "0.1.0",
      asset_version: "0.1.0",
      extension_key: null,
      display_name: canvas.title,
      overwrite: true,
    },
  });
  const text = await resp.text();
  expect(resp.ok(), `status=${resp.status()} ${text}`).toBeTruthy();
  const result = JSON.parse(text) as PromoteInteractionDefinitionResult;
  expect(result.archive_digest).toMatch(/^sha256:/);
  expect(result.definition_id).toBe(canvas.definition_id);
  expect(result.definition_revision_id).toBe(canvas.current_revision_id);
  expect(result.source_bundle_digest).toBe(canvas.source_bundle.digest);
  return result;
}

async function waitForCanvasExtensionProjection(
  request: APIRequestContext,
  projectId: string,
  extensionKey: string,
  title: string,
): Promise<void> {
  await expect
    .poll(async () => {
      const resp = await request.get(`${API_ORIGIN}/projects/${projectId}/extension-runtime`);
      if (!resp.ok()) return "";
      const projection = (await resp.json()) as ExtensionRuntimeProjection;
      const tab = projection.workspace_tabs?.find((item) =>
        item.extension_key === extensionKey && item.label === title
      );
      if (
        tab?.renderer?.kind === "webview"
        && tab.renderer.entry === "dist/canvas/index.html"
      ) {
        return "ready";
      }
      return "";
    }, { timeout: 20_000 })
    .toBe("ready");
}

function normalizeComparablePath(value: string): string {
  const normalized = value.replace(/\\/g, "/").replace(/^\/\/\?\//, "");
  return process.platform === "win32" ? normalized.toLowerCase() : normalized;
}

test("Canvas 可发布为 packaged extension 并作为 WorkspacePanel tab 运行", async ({ page, request }) => {
  const suffix = Date.now().toString();
  await ensureBackend(request);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const agent = await createProjectAgent(request, project.id, suffix);
  const sessionId = await launchProjectAgentRuntime(request, project.id, agent.id);
  const canvas = await createPromotableCanvas(request, project.id, suffix);
  const promoted = await promoteCanvas(request, canvas);
  expect(promoted.extension_id).toBe(`canvas-${canvas.definition_id}`);
  await waitForCanvasExtensionProjection(request, project.id, promoted.extension_key, canvas.title);

  await page.goto(`/session/${sessionId}`);
  await page.getByTitle("展开/收起工作空间面板").click();
  await page.getByTitle("新建 Tab").click();
  const addTabMenu = page.getByText("打开面板", { exact: true }).locator("..");
  await addTabMenu.getByRole("button", { name: canvas.title }).click();

  const frame = page.frameLocator(`iframe[title="${canvas.title}"]`);
  await expect(frame.getByTestId("promoted-canvas")).toContainText(
    `Canvas Extension Ready ${suffix}`,
    { timeout: 30_000 },
  );
});

test.afterEach(async ({ request }) => {
  await cleanupE2eProjects(request);
});
