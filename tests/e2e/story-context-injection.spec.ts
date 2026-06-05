import { expect, test, type APIRequestContext } from "@playwright/test";

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const REPO_ROOT = (process.env.PLAYWRIGHT_E2E_ROOT ?? process.cwd()).replace(/\\/g, "/");
const PLAYWRIGHT_BACKEND_ID = process.env.PLAYWRIGHT_BACKEND_ID ?? "e2e-local";

interface BackendConfig {
  id: string;
  name: string;
  endpoint?: string;
  online?: boolean;
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
  name: string;
  identity_kind: "git_repo" | "p4_workspace" | "local_dir";
  default_binding_id?: string | null;
  bindings: Array<{
    id: string;
    backend_id: string;
    root_ref: string;
  }>;
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
  dispatch_preference: {
    context_sources: ContextSourceRef[];
  };
}

interface RuntimeTraceRef {
  runtime_session_id: string;
}

interface LifecycleRunView {
  runtime_trace_refs: RuntimeTraceRef[];
}

interface SubjectExecutionView {
  runs: LifecycleRunView[];
}

interface StartTaskResponse {
  trace_ref?: string;
}

async function ensureBackend(request: APIRequestContext, suffix: string): Promise<BackendConfig> {
  void suffix;
  const onlineResp = await request.get(`${API_ORIGIN}/backends/online`);
  expect(onlineResp.ok()).toBeTruthy();
  const onlineBackends = (await onlineResp.json()) as BackendConfig[];
  const backend = onlineBackends.find((item) => item.backend_id === PLAYWRIGHT_BACKEND_ID);
  expect(backend, `未找到在线 E2E backend: ${PLAYWRIGHT_BACKEND_ID}`).toBeTruthy();

  const workspaceRoots = backend?.workspace_roots ?? [];
  expect(
    workspaceRoots.some((root) => REPO_ROOT.startsWith(root.replace(/\\/g, "/"))),
    `E2E backend 未暴露当前仓库根目录: ${REPO_ROOT}`,
  ).toBeTruthy();

  return {
    id: PLAYWRIGHT_BACKEND_ID,
    name: backend?.name ?? PLAYWRIGHT_BACKEND_ID,
    online: true,
    workspace_roots: workspaceRoots,
  };
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
  await grantProjectBackendAccess(request, projectId, backendId);
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/workspaces`, {
    data: {
      name: `E2E Workspace ${suffix}`,
      shortcut_binding: {
        backend_id: backendId,
        root_ref: REPO_ROOT,
      },
    },
  });
  expect(resp.ok()).toBeTruthy();
  const workspace = (await resp.json()) as WorkspaceEntity;
  expect(workspace.bindings.length).toBeGreaterThan(0);
  expect(workspace.bindings[0]?.backend_id).toBe(backendId);
  expect(workspace.bindings[0]?.root_ref).toBe(REPO_ROOT);
  return workspace;
}

async function grantProjectBackendAccess(
  request: APIRequestContext,
  projectId: string,
  backendId: string,
): Promise<void> {
  const resp = await request.post(`${API_ORIGIN}/projects/${projectId}/backend-access`, {
    data: {
      backend_id: backendId,
      priority: 0,
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
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

async function createTask(
  request: APIRequestContext,
  storyId: string,
  suffix: string,
  sourceRefs: ContextSourceRef[] = [],
): Promise<TaskEntity> {
  const resp = await request.post(`${API_ORIGIN}/stories/${storyId}/tasks`, {
    data: {
      title: `E2E Context Runtime Task ${suffix}`,
      description: "用于验证 Story 上下文进入 Task runtime trace",
      dispatch_preference: {
        agent_type: "codex",
        context_sources: sourceRefs,
      },
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return (await resp.json()) as TaskEntity;
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

async function startTaskTrace(
  request: APIRequestContext,
  taskId: string,
  prompt: string,
): Promise<string> {
  const resp = await request.post(`${API_ORIGIN}/tasks/${taskId}/start`, {
    data: {
      override_prompt: prompt,
    },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
  const result = (await resp.json()) as StartTaskResponse;
  if (result.trace_ref) return result.trace_ref;
  return pollSubjectTrace(request, "task", taskId);
}

async function pollSubjectTrace(
  request: APIRequestContext,
  subjectKind: "story" | "task",
  subjectId: string,
): Promise<string> {
  let traceId = "";
  await expect
    .poll(
      async () => {
        const resp = await request.get(`${API_ORIGIN}/subjects/${subjectKind}/${subjectId}/execution`);
        if (!resp.ok()) return "";
        const view = (await resp.json()) as SubjectExecutionView;
        traceId = view.runs.flatMap((run) => run.runtime_trace_refs).at(0)?.runtime_session_id ?? "";
        return traceId;
      },
      { timeout: 20_000 },
    )
    .not.toBe("");
  return traceId;
}

function unwrapNotification(record: Record<string, unknown>): Record<string, unknown> | null {
  const candidate = (record.notification ?? record) as Record<string, unknown>;
  if (!candidate || typeof candidate !== "object") return null;
  if (typeof candidate.session_id !== "string") return null;
  if (!candidate.event || typeof candidate.event !== "object") return null;
  return candidate;
}

function userInputTexts(notification: Record<string, unknown>): string[] {
  const event = notification.event as Record<string, unknown> | undefined;
  if (!event || event.type !== "user_input_submitted") return [];
  const payload = event.payload as Record<string, unknown> | undefined;
  const content = payload?.content;
  if (!Array.isArray(content)) return [];
  return content
    .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
    .filter((item) => item.type === "text" && typeof item.text === "string")
    .map((item) => item.text as string);
}

async function collectSessionNotifications(sessionId: string, limit = 24): Promise<Record<string, unknown>[]> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 5000);
  const notifications: Record<string, unknown>[] = [];

  try {
    const response = await fetch(`${API_ORIGIN}/sessions/${sessionId}/stream/ndjson?since_id=0`, {
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
  expect(createdTask!.dispatch_preference.context_sources.map((item) => item.locator)).toEqual([
    "frontend/src/pages/StoryPage.tsx",
    "crates/agentdash-api/src/routes/stories.rs",
  ]);
});

test("Task dispatch runtime trace 会注入 Story 上下文资源", async ({ request }) => {
  const suffix = `${Date.now()}-trace`;
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);

  const sourceRefs: ContextSourceRef[] = [
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
  ];

  await updateStorySourceRefs(request, story.id, sourceRefs);
  const task = await createTask(request, story.id, suffix, sourceRefs);
  const sessionId = await startTaskTrace(
    request,
    task.id,
    "请先阅读当前 Story 上下文，然后简短确认你已拿到上下文。",
  );

  await expect
    .poll(async () => {
      const notifications = await collectSessionNotifications(sessionId, 12);
      return notifications.length;
    }, { timeout: 10_000 })
    .toBeGreaterThan(0);

  const notifications = await collectSessionNotifications(sessionId, 20);
  const texts = notifications.flatMap(userInputTexts);
  const joinedTexts = texts.join("\n");
  expect(joinedTexts).toContain(`agentdash://story-context/${story.id}`);
  expect(joinedTexts).toContain("Story");
  expect(joinedTexts).toContain(`title: ${story.title}`);
  expect(joinedTexts).toContain("文件: frontend/src/pages/StoryPage.tsx");
  expect(joinedTexts).toContain("1 |");
});

test("Task runtime trace 可识别 owner 并返回最新 Task 抽屉", async ({ page, request }) => {
  const suffix = `${Date.now()}-owner`;
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);
  const task = await createTask(request, story.id, suffix);
  const sessionId = await startTaskTrace(request, task.id, "请确认 Task owner trace 已启动。");
  const updatedTitle = `${task.title}（已更新）`;

  await page.goto(`/`);
  await page.evaluate((sid) => {
    window.history.pushState({}, "", `/session/${sid}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, sessionId);
  await expect(page.getByText(`已绑定：${task.title}`)).toBeVisible();
  await expect(page.getByRole("button", { name: "返回任务" })).toBeVisible();

  const updateResp = await request.put(`${API_ORIGIN}/tasks/${task.id}`, {
    data: {
      title: updatedTitle,
      description: "通过外部修改模拟 Task trace 内的更新行为",
    },
  });
  expect(updateResp.ok()).toBeTruthy();

  await page.getByRole("button", { name: "返回任务" }).click();
  await expect(page).toHaveURL(new RegExp(`/story/${story.id}$`));
  await expect(page.locator("aside.fixed").last().getByText(updatedTitle)).toBeVisible();
});
