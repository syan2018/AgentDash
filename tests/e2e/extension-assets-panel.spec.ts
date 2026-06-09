/**
 * Extension Assets Panel E2E
 *
 * 通过 Web UI 走完 Extension 资产页的「上传归档 → 从归档安装 → 卸载」流程，
 * 并验证后端 ExtensionRuntimeProjection 与「已安装」段同步。
 *
 * 大部分 backend / project / pack helper 来源于 tests/e2e/local-hello-extension.spec.ts，
 * 这里复制是为了避免在 tests/e2e/_helpers/ 不存在的情况下扩大改动面（参见 dispatch 指引）。
 */

import { expect, test, type APIRequestContext } from "@playwright/test";
import { execFile } from "node:child_process";
import { mkdtemp } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { cleanupE2eProjects, trackE2eProject } from "./_helpers/project-cleanup";

const execFileAsync = promisify(execFile);

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const REPO_ROOT = (process.env.PLAYWRIGHT_E2E_ROOT ?? process.cwd()).replace(/\\/g, "/");
const NORMALIZED_REPO_ROOT = normalizeComparablePath(REPO_ROOT);
const PLAYWRIGHT_BACKEND_ID = process.env.PLAYWRIGHT_BACKEND_ID ?? "e2e-local";
const DEMO_DIR = path.join(REPO_ROOT, "examples", "extensions", "local-hello");

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

interface PackResult {
  archive_path: string;
  archive_digest: string;
}

interface ExtensionRuntimeProjection {
  installations?: Array<{
    extension_key?: string;
    installation_id?: string;
  }>;
  workspace_tabs?: Array<{
    type_id?: string;
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
      name: `E2E Extension Assets 项目 ${suffix}`,
      description: "用于验证 Extension Assets 面板上传/安装/卸载金线",
      config: {
        default_agent_type: "codex",
      },
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return trackE2eProject((await resp.json()) as ProjectEntity);
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

async function createWorkspace(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<WorkspaceEntity> {
  await grantProjectBackendAccess(request, projectId);
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/workspaces`, {
    data: {
      name: `E2E Extension Assets Workspace ${suffix}`,
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
      name: `extension-assets-${suffix}`,
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

async function packLocalHello(): Promise<PackResult> {
  const outDir = await mkdtemp(path.join(os.tmpdir(), "agentdash-extension-assets-"));
  const cliPath = path.join(REPO_ROOT, "packages", "extension-dev", "src", "cli.js");
  const { stdout } = await execFileAsync(
    process.execPath,
    [cliPath, "pack", "--cwd", DEMO_DIR, "--out-dir", outDir],
    {
      cwd: REPO_ROOT,
      windowsHide: true,
    },
  );
  const jsonStart = stdout.indexOf("{");
  if (jsonStart < 0) {
    throw new Error(`local-hello pack 未输出 JSON: ${stdout}`);
  }
  return parsePackResult(JSON.parse(stdout.slice(jsonStart)));
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

async function fetchExtensionProjection(
  request: APIRequestContext,
  projectId: string,
): Promise<ExtensionRuntimeProjection> {
  const resp = await request.get(`${API_ORIGIN}/projects/${projectId}/extension-runtime`);
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return (await resp.json()) as ExtensionRuntimeProjection;
}

test("Local Hello 归档可在 Assets 面板上传/安装/卸载", async ({ page, request }) => {
  const suffix = Date.now().toString();
  const packed = await packLocalHello();
  await ensureBackend(request);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  // 准备一个 session，用于 AddTabMenu 子断言（projection -> workspace tab catalog）。
  const agent = await createProjectAgent(request, project.id, suffix);
  const sessionId = await launchProjectAgentRuntime(request, project.id, agent.id);

  // Step 1：进入 Extension 类目。Assets 页 useProjectStore.fetchProjects() 会
  // 自动选中第一个 Project；本测试串行运行所以新建的 project 即第一个候选。
  await page.goto(`/dashboard/assets/extension`);
  await expect(page.getByRole("heading", { name: "Extension", exact: true })).toBeVisible();
  await expect(page.getByText(/已安装 \(/)).toBeVisible();
  await expect(page.getByText(/归档库 \(/)).toBeVisible();
  await expect(page.getByText("还没有上传过归档")).toBeVisible();

  // Step 2：点「上传归档」→ 选文件 → 提交。
  await page.getByRole("button", { name: "上传归档", exact: true }).click();
  const uploadDialog = page.getByRole("dialog").filter({ hasText: "上传扩展归档" });
  await expect(uploadDialog).toBeVisible();
  const fileInput = uploadDialog.locator('input[type="file"]');
  await fileInput.setInputFiles(packed.archive_path);
  await uploadDialog.getByRole("button", { name: "上传", exact: true }).click();

  // Step 3：上传成功后面板自动弹「从归档安装」对话框；勾 overwrite 后提交。
  const installDialog = page.getByRole("dialog").filter({ hasText: "从归档安装扩展" });
  await expect(installDialog).toBeVisible();
  await installDialog.getByLabel("覆盖已存在的同 key 安装").check();
  await installDialog.getByRole("button", { name: "安装", exact: true }).click();
  await expect(installDialog).toBeHidden();

  // Step 4：「已安装」段出现 local-hello + 来源 badge = 本地归档。
  const installedSection = page
    .locator("section")
    .filter({ has: page.getByRole("heading", { name: /已安装 \(/ }) });
  // 兼容当前面板用 <h3> 而非 role=heading 渲染段标题；fallback 到 ancestor 选择器。
  const installedRow = page
    .locator("article")
    .filter({ hasText: "local-hello" })
    .filter({ hasText: "本地归档" });
  await expect(installedRow).toHaveCount(1, { timeout: 15_000 });

  // Step 5：归档段也出现该归档（archive_digest 截断展示）。
  const digestPrefix = packed.archive_digest.slice(0, 16);
  const archiveRow = page.locator("article").filter({ hasText: digestPrefix });
  await expect(archiveRow).toHaveCount(1);

  // Step 6：projection 已含 local-hello.panel workspace tab。
  await expect
    .poll(async () => {
      const projection = await fetchExtensionProjection(request, project.id);
      const hasInstall = projection.installations?.some(
        (item) => item.extension_key === "local-hello",
      );
      const hasTab = projection.workspace_tabs?.some(
        (tab) => tab.extension_key === "local-hello" && tab.type_id === "local-hello.panel",
      );
      return hasInstall && hasTab ? "ready" : "";
    }, { timeout: 20_000 })
    .toBe("ready");

  // Step 7：在 session 工作空间面板里的「打开面板」菜单可看到 Local Hello。
  await page.goto(`/session/${sessionId}`);
  await page.getByTitle("展开/收起工作空间面板").click();
  await page.getByTitle("新建 Tab").click();
  const addTabMenu = page.getByText("打开面板", { exact: true }).locator("..");
  await expect(addTabMenu.getByRole("button", { name: "Local Hello" })).toBeVisible({
    timeout: 15_000,
  });

  // Step 8：回 Assets 页执行卸载。
  await page.goto(`/dashboard/assets/extension`);
  await expect(installedRow).toHaveCount(1);
  await installedRow.getByRole("button", { name: "卸载", exact: true }).click();
  const confirmDialog = page.getByRole("dialog").filter({ hasText: "卸载扩展" });
  await expect(confirmDialog).toBeVisible();
  await confirmDialog.getByRole("button", { name: "卸载", exact: true }).click();

  // Step 9：「已安装」段不再有 local-hello；归档段仍然保留。
  await expect(installedRow).toHaveCount(0, { timeout: 15_000 });
  await expect(archiveRow).toHaveCount(1);

  // Step 10：projection 不再含 local-hello installation。
  await expect
    .poll(async () => {
      const projection = await fetchExtensionProjection(request, project.id);
      const hasInstall = projection.installations?.some(
        (item) => item.extension_key === "local-hello",
      );
      return hasInstall ? "still-present" : "gone";
    }, { timeout: 20_000 })
    .toBe("gone");

  // 防止 lint 投诉 unused：installedSection 仅作为面板可达性的 sanity check 占位。
  await expect(installedSection.first()).toBeVisible();
});

test.afterEach(async ({ request }) => {
  await cleanupE2eProjects(request);
});
