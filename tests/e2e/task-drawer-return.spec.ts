import { expect, test, type APIRequestContext } from "@playwright/test";

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const REPO_ROOT = (process.env.PLAYWRIGHT_E2E_ROOT ?? process.cwd()).replace(/\\/g, "/");
const PLAYWRIGHT_BACKEND_ID = process.env.PLAYWRIGHT_BACKEND_ID ?? "e2e-local";

interface BackendConfig {
  id: string;
  name: string;
  endpoint?: string;
  backend_id?: string;
  accessible_roots?: string[];
}

interface ProjectEntity {
  id: string;
  name?: string;
  description?: string;
  config?: {
    default_agent_type?: string | null;
    default_workspace_id?: string | null;
    agent_presets?: Array<unknown>;
  };
}

interface StoryEntity {
  id: string;
}

interface WorkspaceEntity {
  id: string;
  bindings: Array<{
    id: string;
    backend_id: string;
    root_ref: string;
  }>;
}

interface TaskEntity {
  id: string;
  session_id?: string | null;
}

async function ensureBackend(request: APIRequestContext, suffix: string): Promise<BackendConfig> {
  void suffix;
  const onlineResp = await request.get(`${API_ORIGIN}/backends/online`);
  expect(onlineResp.ok()).toBeTruthy();
  const onlineBackends = (await onlineResp.json()) as BackendConfig[];
  const backend = onlineBackends.find((item) => item.backend_id === PLAYWRIGHT_BACKEND_ID);
  expect(backend, `未找到在线 E2E backend: ${PLAYWRIGHT_BACKEND_ID}`).toBeTruthy();
  return {
    id: PLAYWRIGHT_BACKEND_ID,
    name: backend?.name ?? PLAYWRIGHT_BACKEND_ID,
    accessible_roots: backend?.accessible_roots ?? [],
  };
}

async function createProject(request: APIRequestContext, suffix: string): Promise<ProjectEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects`, {
    data: {
      name: `E2E 抽屉返回项目 ${suffix}`,
      description: "用于回归测试 Task 抽屉返回链路",
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
  backendId: string,
  suffix: string,
): Promise<WorkspaceEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/workspaces`, {
    data: {
      name: `E2E Drawer Workspace ${suffix}`,
      shortcut_binding: {
        backend_id: backendId,
        root_ref: REPO_ROOT,
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
  const workspace = (await resp.json()) as WorkspaceEntity;
  expect(workspace.bindings[0]?.backend_id).toBe(backendId);
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
      name: project.name ?? `E2E Drawer Project ${project.id}`,
      description: project.description ?? "用于回归测试 Task 抽屉返回链路",
      config: {
        default_agent_type: project.config?.default_agent_type ?? "codex",
        default_workspace_id: workspaceId,
        agent_presets: project.config?.agent_presets ?? [],
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
}

async function createStory(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<StoryEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories`, {
    data: {
      project_id: projectId,
      title: `E2E 抽屉返回 Story ${suffix}`,
      description: "用于验证会话返回后抽屉关闭行为",
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as StoryEntity;
}

async function createTask(request: APIRequestContext, storyId: string, suffix: string): Promise<TaskEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories/${storyId}/tasks`, {
    data: {
      title: `E2E 抽屉返回 Task ${suffix}`,
      description: "用于验证抽屉与会话回跳",
      agent_binding: {
        agent_type: "codex",
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as TaskEntity;
}

async function bindTaskSession(request: APIRequestContext, taskId: string): Promise<string> {
  await request.post(`${API_ORIGIN}/tasks/${taskId}/start`, {
    data: {},
  });

  await expect
    .poll(
      async () => {
        const taskResp = await request.get(`${API_ORIGIN}/tasks/${taskId}`);
        if (!taskResp.ok()) return "";
        const task = (await taskResp.json()) as TaskEntity;
        return task.session_id ?? "";
      },
      {
        timeout: 20_000,
      },
    )
    .not.toBe("");

  const latestResp = await request.get(`${API_ORIGIN}/tasks/${taskId}`);
  expect(latestResp.ok()).toBeTruthy();
  const latest = (await latestResp.json()) as TaskEntity;
  return latest.session_id as string;
}

test("Task 抽屉返回链路稳定：关闭不反弹、会话返回后可正常关闭", async ({ page, request }) => {
  const suffix = Date.now().toString();
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);
  const task = await createTask(request, story.id, suffix);
  const sessionId = await bindTaskSession(request, task.id);

  await page.goto(`/story/${story.id}`);
  await page.getByRole("button", { name: "任务列表" }).click();
  await page.getByRole("button", { name: new RegExp(`E2E 抽屉返回 Task ${suffix}`) }).click();

  const drawer = page.locator("aside.fixed").last();
  await expect(drawer).toBeVisible();

  // 1) 右上角关闭：不应自动重开
  await drawer.getByRole("button", { name: "关闭" }).click();
  await expect(page.locator("aside.fixed")).toHaveCount(0);
  await page.waitForTimeout(1500);
  await expect(page.locator("aside.fixed")).toHaveCount(0);

  // 2) 左侧遮罩关闭：不应自动重开
  await page.getByRole("button", { name: new RegExp(`E2E 抽屉返回 Task ${suffix}`) }).click();
  await expect(page.locator("aside.fixed")).toHaveCount(1);
  await page.locator("div.fixed.inset-0.z-30").click({ position: { x: 2, y: 20 } });
  await expect(page.locator("aside.fixed")).toHaveCount(0);
  await page.waitForTimeout(1500);
  await expect(page.locator("aside.fixed")).toHaveCount(0);

  // 3) 会话页返回任务：回到 Story 后抽屉可关闭且不反弹
  await page.getByRole("button", { name: new RegExp(`E2E 抽屉返回 Task ${suffix}`) }).click();
  await expect(page.locator("aside.fixed")).toHaveCount(1);
  await page.getByRole("button", { name: "刷新状态" }).click();
  await page.locator("aside.fixed").last().getByRole("button", { name: "会话页" }).click();
  await expect(page).toHaveURL(new RegExp(`/session/${sessionId}$`));

  await page.getByRole("button", { name: "返回任务" }).click();
  await expect(page).toHaveURL(new RegExp(`/story/${story.id}$`));
  await expect(page.locator("aside.fixed")).toHaveCount(1);

  await page.locator("aside.fixed").last().getByRole("button", { name: "关闭" }).click();
  await expect(page.locator("aside.fixed")).toHaveCount(0);
  await page.waitForTimeout(1500);
  await expect(page.locator("aside.fixed")).toHaveCount(0);
  await expect(page.getByText("Story 不存在")).toHaveCount(0);
});
