import { expect, test, type APIRequestContext } from "@playwright/test";

const API_ORIGIN = "http://127.0.0.1:3011/api";
const REPO_ROOT = "F:/Projects/AgentDash";

interface BackendConfig {
  id: string;
  name: string;
  endpoint: string;
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
  backend_id: string;
  name: string;
  container_ref: string;
}

interface ContextSourceRef {
  kind: string;
  locator: string;
  label?: string | null;
  slot?: string;
  priority?: number;
  required?: boolean;
  max_chars?: number | null;
  delivery?: string;
}

interface StoryEntity {
  id: string;
  title: string;
  context: {
    source_refs: ContextSourceRef[];
  };
}

interface TaskEntity {
  id: string;
  title: string;
  agent_binding: {
    context_sources: ContextSourceRef[];
  };
}

interface SessionBindingEntity {
  id: string;
  sessionId?: string;
  session_id?: string;
  label: string;
}

interface PromptResult {
  status: number;
  body: string;
}

async function ensureBackend(request: APIRequestContext, suffix: string): Promise<BackendConfig> {
  const listResp = await request.get(`${API_ORIGIN}/backends`);
  expect(listResp.ok()).toBeTruthy();
  const backends = (await listResp.json()) as BackendConfig[];
  if (backends.length > 0) {
    return backends[0]!;
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

async function createProject(request: APIRequestContext, suffix: string): Promise<ProjectEntity> {
  const resp = await request.post(`${API_ORIGIN}/projects`, {
    data: {
      name: `E2E Story Context 项目 ${suffix}`,
      description: "用于验证 Story 文件引用与上下文注入",
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
      name: `E2E Workspace ${suffix}`,
      backend_id: backendId,
      container_ref: REPO_ROOT,
      workspace_type: "static",
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as WorkspaceEntity;
}

async function updateProjectDefaultWorkspace(
  request: APIRequestContext,
  project: ProjectEntity,
  workspaceId: string,
): Promise<ProjectEntity> {
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
  return (await resp.json()) as ProjectEntity;
}

async function createStory(
  request: APIRequestContext,
  projectId: string,
  suffix: string,
): Promise<StoryEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories`, {
    data: {
      project_id: projectId,
      title: `E2E Story Context ${suffix}`,
      description: "用于验证 Story 文件引用、Task 分配与会话上下文注入",
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as StoryEntity;
}

async function getStory(request: APIRequestContext, storyId: string): Promise<StoryEntity> {
  const resp = await request.get(`${API_ORIGIN}/stories/${storyId}`);
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as StoryEntity;
}

async function listTasks(request: APIRequestContext, storyId: string): Promise<TaskEntity[]> {
  const resp = await request.get(`${API_ORIGIN}/stories/${storyId}/tasks`);
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as TaskEntity[];
}

async function updateStorySourceRefs(
  request: APIRequestContext,
  storyId: string,
  sourceRefs: ContextSourceRef[],
): Promise<StoryEntity> {
  const resp = await request.put(`${API_ORIGIN}/stories/${storyId}`, {
    data: {
      context_source_refs: sourceRefs,
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as StoryEntity;
}

async function createStorySession(
  request: APIRequestContext,
  storyId: string,
  suffix: string,
): Promise<SessionBindingEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories/${storyId}/sessions`, {
    data: {
      title: `E2E Story Session ${suffix}`,
      label: "companion",
    },
  });
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as SessionBindingEntity;
}

async function promptSession(request: APIRequestContext, sessionId: string): Promise<PromptResult> {
  const resp = await request.post(`${API_ORIGIN}/sessions/${sessionId}/prompt`, {
    data: {
      prompt: "请先阅读当前 Story 上下文，然后简短确认你已拿到上下文。",
    },
  });
  return {
    status: resp.status(),
    body: await resp.text(),
  };
}

function getBindingSessionId(binding: SessionBindingEntity): string {
  const sessionId = binding.sessionId ?? binding.session_id ?? "";
  expect(sessionId).not.toBe("");
  return sessionId;
}

function unwrapNotification(record: Record<string, unknown>): Record<string, unknown> | null {
  const candidate = (record.notification ?? record) as Record<string, unknown>;
  if (!candidate || typeof candidate !== "object") return null;
  if (typeof candidate.sessionId !== "string") return null;
  if (!candidate.update || typeof candidate.update !== "object") return null;
  return candidate;
}

async function collectSessionNotifications(sessionId: string, limit = 24): Promise<Record<string, unknown>[]> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 5000);
  const notifications: Record<string, unknown>[] = [];

  try {
    const response = await fetch(`${API_ORIGIN}/acp/sessions/${sessionId}/stream/ndjson?since_id=0`, {
      headers: {
        Accept: "application/x-ndjson",
      },
      signal: controller.signal,
    });
    expect(response.ok).toBeTruthy();
    expect(response.body).toBeTruthy();

    const reader = response.body!.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (notifications.length < limit) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        const parsed = JSON.parse(trimmed) as Record<string, unknown>;
        const notification = unwrapNotification(parsed);
        if (!notification) continue;
        notifications.push(notification);
        if (notifications.length >= limit) {
          controller.abort();
          break;
        }
      }
    }
  } catch (error) {
    if (!(error instanceof Error) || error.name !== "AbortError") {
      throw error;
    }
  } finally {
    clearTimeout(timeout);
  }

  return notifications;
}

test("Story 文件引用可保存到 Story 并分配给 Task Agent", async ({ page, request }) => {
  const suffix = Date.now().toString();
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);
  const taskTitle = `E2E Context Task ${suffix}`;

  await page.goto(`/`);
  await page.getByRole("button", { name: new RegExp(project.name) }).click();
  await page.locator("button").filter({ hasText: story.title }).last().click();
  await expect(page.getByText("工作区文件引用")).toBeVisible();

  await page.getByRole("button", { name: "编辑" }).nth(1).click();
  await page.getByRole("button", { name: "新增文件引用" }).click();
  await page.getByRole("button", { name: "新增文件引用" }).click();

  await page.getByPlaceholder("引用标题（可选）").nth(0).fill("Story 页面");
  await page.getByPlaceholder("例如: crates/agentdash-api/src/routes/stories.rs").nth(0).fill("frontend/src/pages/StoryPage.tsx");
  await page.getByPlaceholder("引用标题（可选）").nth(1).fill("Story 路由");
  await page.getByPlaceholder("例如: crates/agentdash-api/src/routes/stories.rs").nth(1).fill("crates/agentdash-api/src/routes/stories.rs");

  await page.getByRole("button", { name: "保存上下文" }).click();
  await expect(page.getByText("文件引用已保存到 Story，可用于伴生会话与 Task 分配")).toBeVisible();
  await expect(page.getByText("frontend/src/pages/StoryPage.tsx")).toBeVisible();
  await expect(page.getByText("crates/agentdash-api/src/routes/stories.rs")).toBeVisible();

  const updatedStory = await getStory(request, story.id);
  expect(updatedStory.context.source_refs).toHaveLength(2);
  expect(updatedStory.context.source_refs.map((item) => item.locator)).toEqual([
    "frontend/src/pages/StoryPage.tsx",
    "crates/agentdash-api/src/routes/stories.rs",
  ]);

  await page.getByRole("button", { name: "任务列表" }).click();
  await page.getByRole("button", { name: "添加 Task" }).click();
  await page.getByPlaceholder("Task 标题").fill(taskTitle);

  await page.locator("label").filter({ hasText: "Story 页面" }).click();
  await page.locator("label").filter({ hasText: "Story 路由" }).click();
  await page.getByRole("button", { name: "创建" }).click();

  const taskCard = page.getByRole("button", { name: new RegExp(taskTitle) }).first();
  await expect(taskCard).toBeVisible();
  await taskCard.click();

  await expect(page.getByText("已分配 Story 上下文")).toBeVisible();
  await expect(page.getByText("frontend/src/pages/StoryPage.tsx")).toBeVisible();
  await expect(page.getByText("crates/agentdash-api/src/routes/stories.rs")).toBeVisible();

  const tasks = await listTasks(request, story.id);
  const createdTask = tasks.find((item) => item.title === taskTitle);
  expect(createdTask).toBeTruthy();
  expect(createdTask!.agent_binding.context_sources.map((item) => item.locator)).toEqual([
    "frontend/src/pages/StoryPage.tsx",
    "crates/agentdash-api/src/routes/stories.rs",
  ]);
});

test("Story 伴随会话在 prompt 前会自动注入 Story 上下文资源", async ({ request }) => {
  const suffix = `${Date.now()}-session`;
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);

  await updateStorySourceRefs(request, story.id, [
    {
      kind: "file",
      locator: "frontend/src/pages/StoryPage.tsx",
      label: "Story 页面",
      slot: "references",
      priority: 1000,
      required: false,
      max_chars: null,
      delivery: "resource",
    },
  ]);

  const binding = await createStorySession(request, story.id, suffix);
  const sessionId = getBindingSessionId(binding);
  const prompt = await promptSession(request, sessionId);

  expect([200, 400, 500]).toContain(prompt.status);

  await expect
    .poll(async () => {
      const notifications = await collectSessionNotifications(sessionId, 12);
      return notifications.length;
    }, { timeout: 10_000 })
    .toBeGreaterThan(0);

  const notifications = await collectSessionNotifications(sessionId, 20);
  const resourceBlock = notifications.find((item) => {
    const update = item.update as Record<string, unknown>;
    if (update.sessionUpdate !== "user_message_chunk") return false;
    const content = update.content as Record<string, unknown> | undefined;
    if (!content || content.type !== "resource") return false;
    const resource = content.resource as Record<string, unknown> | undefined;
    return resource?.uri === `agentdash://story-context/${story.id}`;
  });
  expect(resourceBlock).toBeTruthy();

  const instructionBlock = notifications.find((item) => {
    const update = item.update as Record<string, unknown>;
    if (update.sessionUpdate !== "user_message_chunk") return false;
    const content = update.content as Record<string, unknown> | undefined;
    return content?.type === "text" && String(content.text ?? "").includes("你是该 Story 的主代理");
  });
  expect(instructionBlock).toBeTruthy();

  const resource = ((resourceBlock!.update as Record<string, unknown>).content as Record<string, unknown>).resource as Record<string, unknown>;
  expect(String(resource.text ?? "")).toContain(`title: ${story.title}`);
  expect(String(resource.text ?? "")).toContain("文件: frontend/src/pages/StoryPage.tsx");
  expect(String(resource.text ?? "")).toContain("1 |");
});

test("Story 会话可识别 owner 绑定并返回最新 Story 页面", async ({ page, request }) => {
  const suffix = `${Date.now()}-owner`;
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, backend.id, suffix);
  const workspace = await createWorkspace(request, project.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, backend.id, suffix);
  const binding = await createStorySession(request, story.id, suffix);
  const sessionId = getBindingSessionId(binding);
  const updatedTitle = `${story.title}（已更新）`;

  await page.goto(`/`);
  await page.evaluate((sid) => {
    window.history.pushState({}, "", `/session/${sid}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, sessionId);
  await expect(page.getByText(`已绑定：${story.title}`)).toBeVisible();
  await expect(page.getByRole("button", { name: "返回 Story" })).toBeVisible();

  const updateResp = await request.put(`${API_ORIGIN}/stories/${story.id}`, {
    data: {
      title: updatedTitle,
      description: "通过外部修改模拟 Story 会话内的更新行为",
    },
  });
  expect(updateResp.ok()).toBeTruthy();

  await page.getByRole("button", { name: "返回 Story" }).click();
  await expect(page).toHaveURL(new RegExp(`/story/${story.id}$`));
  await expect(page.getByRole("heading", { name: updatedTitle })).toBeVisible();
});
