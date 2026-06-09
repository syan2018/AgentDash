import { expect, test, type APIRequestContext } from "@playwright/test";

import { cleanupE2eProjects, trackE2eProject } from "./_helpers/project-cleanup";

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
    online: true,
    workspace_roots: backend?.workspace_roots ?? [],
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
  return trackE2eProject((await resp.json()) as ProjectEntity);
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

test("Story 文件引用可保存到 Story 并分配给 Task Agent", async ({ page, request }) => {
  const suffix = Date.now().toString();
  const backend = await ensureBackend(request, suffix);
  const project = await createProject(request, suffix);
  const workspace = await createWorkspace(request, project.id, backend.id, suffix);
  await updateProjectDefaultWorkspace(request, project, workspace.id);
  const story = await createStory(request, project.id, suffix);
  const taskTitle = `E2E Context Task ${suffix}`;
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
    {
      kind: "file",
      locator: "crates/agentdash-api/src/routes/stories.rs",
      label: "Story 路由",
      slot: "references",
      priority: 1001,
      required: false,
      max_chars: null,
      delivery: "resource",
    },
  ];

  await updateStorySourceRefs(request, story.id, sourceRefs);
  await page.goto(`/story/${story.id}`);
  await expect(page.getByRole("heading", { level: 1, name: story.title })).toBeVisible();

  const updatedStory = await getStory(request, story.id);
  expect(updatedStory.context.source_refs).toHaveLength(2);
  expect(updatedStory.context.source_refs.map((item) => item.locator)).toEqual([
    "frontend/src/pages/StoryPage.tsx",
    "crates/agentdash-api/src/routes/stories.rs",
  ]);

  await page.getByRole("button", { name: "添加 Task" }).click();
  await page.getByRole("textbox", { name: "Task 标题" }).fill(taskTitle);
  const agentTypeSelect = page.getByRole("combobox").nth(1);
  const codexOptionValue = await agentTypeSelect
    .locator("option", { hasText: "Codex" })
    .first()
    .getAttribute("value");
  expect(codexOptionValue).toBeTruthy();
  await agentTypeSelect.selectOption(codexOptionValue!);

  await page.getByRole("checkbox", { name: /Story 页面/ }).check();
  await page.getByRole("checkbox", { name: /Story 路由/ }).check();
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

test.afterEach(async ({ request }) => {
  await cleanupE2eProjects(request);
});
