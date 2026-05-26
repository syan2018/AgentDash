import { expect, test, type APIRequestContext } from "@playwright/test";
import { execFile } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const REPO_ROOT = (process.env.PLAYWRIGHT_E2E_ROOT ?? process.cwd()).replace(/\\/g, "/");
const NORMALIZED_REPO_ROOT = normalizeComparablePath(REPO_ROOT);
const PLAYWRIGHT_BACKEND_ID = process.env.PLAYWRIGHT_BACKEND_ID ?? "e2e-local";
const DEMO_DIR = path.join(REPO_ROOT, "examples", "extensions", "local-hello");

interface BackendConfig {
  name?: string;
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

interface OpenProjectAgentSessionResult {
  session_id: string;
}

interface PackResult {
  archive_path: string;
  archive_digest: string;
}

interface ArtifactUploadResult {
  id: string;
  extension_id: string;
  archive_digest: string;
}

interface ExtensionRuntimeProjection {
  workspace_tabs?: Array<{
    type_id?: string;
    extension_key?: string;
  }>;
  runtime_actions?: Array<{
    action_key?: string;
    extension_key?: string;
  }>;
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
      name: `E2E Local Hello 项目 ${suffix}`,
      description: "用于验证 packaged extension 金线",
      config: {
        default_agent_type: "codex",
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as ProjectEntity;
}

async function createWorkspace(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<WorkspaceEntity> {
  await grantProjectBackendAccess(request, projectId);
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/workspaces`, {
    data: {
      name: `E2E Local Hello Workspace ${suffix}`,
      shortcut_binding: {
        backend_id: PLAYWRIGHT_BACKEND_ID,
        root_ref: REPO_ROOT,
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
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
  expect(resp.ok()).toBeTruthy();
}

async function createProjectAgent(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<ProjectAgentEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/agents`, {
    data: {
      name: `local-hello-${suffix}`,
      agent_type: "codex",
      config: {},
      is_default_for_story: false,
      is_default_for_task: false,
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as ProjectAgentEntity;
}

async function openProjectAgentSession(
  request: APIRequestContext,
  projectId: string,
  agentId: string,
): Promise<string> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/agents/${agentId}/session?force_new=true`, {
    data: {},
  });
  expect(resp.ok()).toBeTruthy();
  const result = (await resp.json()) as OpenProjectAgentSessionResult;
  expect(result.session_id).not.toBe("");
  return result.session_id;
}

async function packLocalHello(): Promise<PackResult> {
  const outDir = await mkdtemp(path.join(os.tmpdir(), "agentdash-local-hello-"));
  const cliPath = path.join(REPO_ROOT, "packages", "extension-dev", "src", "cli.js");
  const { stdout } = await execFileAsync(process.execPath, [
    cliPath,
    "pack",
    "--cwd",
    DEMO_DIR,
    "--out-dir",
    outDir,
  ], {
    cwd: REPO_ROOT,
    windowsHide: true,
  });
  const jsonStart = stdout.indexOf("{");
  if (jsonStart < 0) {
    throw new Error(`local-hello pack 未输出 JSON: ${stdout}`);
  }
  return parsePackResult(JSON.parse(stdout.slice(jsonStart)));
}

async function uploadAndInstallArchive(
  request: APIRequestContext,
  projectId: string,
  packed: PackResult,
): Promise<void> {
  const archive = await readFile(packed.archive_path);
  const uploadResp = await request.post(`${API_ORIGIN}/projects/${projectId}/extension-artifacts`, {
    multipart: {
      archive_digest: packed.archive_digest,
      archive: {
        name: path.basename(packed.archive_path),
        mimeType: "application/vnd.agentdash.extension+gzip",
        buffer: archive,
      },
    },
  });
  const uploadText = await uploadResp.text();
  expect(uploadResp.ok(), `status=${uploadResp.status()} ${uploadText}`).toBeTruthy();
  const artifact = JSON.parse(uploadText) as ArtifactUploadResult;
  expect(artifact.extension_id).toBe("local-hello");
  expect(artifact.archive_digest).toBe(packed.archive_digest);

  const installResp = await request.post(`${API_ORIGIN}/projects/${projectId}/extension-artifacts/${artifact.id}/install`, {
    data: {
      extension_key: "local-hello",
      display_name: "Local Hello",
      overwrite: true,
    },
  });
  const installText = await installResp.text();
  expect(installResp.ok(), `status=${installResp.status()} ${installText}`).toBeTruthy();
}

async function waitForExtensionProjection(
  request: APIRequestContext,
  projectId: string,
): Promise<void> {
  await expect
    .poll(async () => {
      const resp = await request.get(`${API_ORIGIN}/projects/${projectId}/extension-runtime`);
      if (!resp.ok()) return "";
      const projection = (await resp.json()) as ExtensionRuntimeProjection;
      const hasPanel = projection.workspace_tabs?.some((tab) =>
        tab.extension_key === "local-hello" && tab.type_id === "local-hello.panel"
      ) ?? false;
      const hasAction = projection.runtime_actions?.some((action) =>
        action.extension_key === "local-hello" && action.action_key === "local-hello.profile"
      ) ?? false;
      return hasPanel && hasAction ? "ready" : "";
    }, { timeout: 20_000 })
    .toBe("ready");
}

function parsePackResult(value: unknown): PackResult {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("local-hello pack result 必须是对象");
  }
  const record = value as Record<string, unknown>;
  if (typeof record.archive_path !== "string" || typeof record.archive_digest !== "string") {
    throw new Error("local-hello pack result 缺少 archive_path/archive_digest");
  }
  return {
    archive_path: record.archive_path,
    archive_digest: record.archive_digest,
  };
}

function normalizeComparablePath(value: string): string {
  const normalized = value.replace(/\\/g, "/").replace(/^\/\/\?\//, "");
  return process.platform === "win32" ? normalized.toLowerCase() : normalized;
}

test("Local Hello packaged archive 可安装并通过 WorkspacePanel 调用本机 profile", async ({ page, request }) => {
  const suffix = Date.now().toString();
  const packed = await packLocalHello();
  await ensureBackend(request);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const agent = await createProjectAgent(request, project.id, suffix);
  const sessionId = await openProjectAgentSession(request, project.id, agent.id);

  await uploadAndInstallArchive(request, project.id, packed);
  await waitForExtensionProjection(request, project.id);

  await page.goto(`/session/${sessionId}`);
  await page.getByTitle("展开/收起工作空间面板").click();
  await page.getByTitle("新建 Tab").click();
  const addTabMenu = page.getByText("打开面板", { exact: true }).locator("..");
  await addTabMenu.getByRole("button", { name: "Local Hello" }).click();

  const frame = page.frameLocator('iframe[data-extension-key="local-hello"]');
  await expect(frame.getByText("Profile loaded from the local TypeScript extension host.")).toBeVisible({
    timeout: 30_000,
  });
  await expect(frame.getByTestId("local-hello-backend")).toHaveText(PLAYWRIGHT_BACKEND_ID);
  await expect(frame.getByTestId("local-hello-session")).toHaveText(sessionId);
  await expect(frame.getByTestId("local-hello-username")).not.toHaveText("unknown");
  await expect(frame.getByTestId("local-hello-platform")).not.toHaveText("unknown");
});
