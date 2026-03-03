import { expect, test, type APIRequestContext } from "@playwright/test";

const API_ORIGIN = "http://127.0.0.1:3011/api";

interface BackendConfig {
  id: string;
  name: string;
  endpoint: string;
}

interface ProjectEntity {
  id: string;
  backend_id: string;
}

interface StoryEntity {
  id: string;
}

interface TaskEntity {
  id: string;
  session_id?: string | null;
}

async function ensureBackend(request: APIRequestContext, suffix: string): Promise<BackendConfig> {
  const listResp = await request.get(`${API_ORIGIN}/backends`);
  expect(listResp.ok()).toBeTruthy();
  const backends = (await listResp.json()) as BackendConfig[];
  if (backends.length > 0) {
    return backends[0];
  }

  const backend: BackendConfig = {
    id: `e2e-backend-${suffix}`,
    name: `E2E Backend ${suffix}`,
    endpoint: "http://127.0.0.1:3011",
  };
  const createResp = await request.post(`${API_ORIGIN}/backends`, {
    data: {
      ...backend,
      backend_type: "local",
    },
  });
  expect(createResp.ok()).toBeTruthy();
  return backend;
}

async function createProject(request: APIRequestContext, backendId: string, suffix: string): Promise<ProjectEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects`, {
    data: {
      name: `E2E 抽屉返回项目 ${suffix}`,
      description: "用于回归测试 Task 抽屉返回链路",
      backend_id: backendId,
      config: {
        default_agent_type: "codex",
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as ProjectEntity;
}

async function createStory(
  request: APIRequestContext,
  projectId: string,
  backendId: string,
  suffix: string,
): Promise<StoryEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories`, {
    data: {
      project_id: projectId,
      backend_id: backendId,
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
  const project = await createProject(request, backend.id, suffix);
  const story = await createStory(request, project.id, backend.id, suffix);
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
