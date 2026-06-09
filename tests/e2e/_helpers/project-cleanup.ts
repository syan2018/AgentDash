import { expect, type APIRequestContext } from "@playwright/test";

const SERVER_PORT = process.env.PLAYWRIGHT_SERVER_PORT ?? "3011";
const API_ORIGIN = `http://127.0.0.1:${SERVER_PORT}/api`;
const E2E_PROJECT_NAME_PATTERN = /^E2E(?: .*)?项目 \d+(?:-[A-Za-z0-9_-]+)?$/;

interface E2eProjectRef {
  id: string;
  name: string;
}

const trackedProjectIds: string[] = [];

export function trackE2eProject<T extends E2eProjectRef>(project: T): T {
  if (!trackedProjectIds.includes(project.id)) {
    trackedProjectIds.push(project.id);
  }
  return project;
}

export async function cleanupE2eProjects(request: APIRequestContext): Promise<void> {
  const projects = await listProjects(request);
  const ids = new Set(trackedProjectIds);
  for (const project of projects) {
    if (E2E_PROJECT_NAME_PATTERN.test(project.name)) {
      ids.add(project.id);
    }
  }

  for (const projectId of Array.from(ids).reverse()) {
    await deleteProject(request, projectId);
  }

  trackedProjectIds.length = 0;
}

async function listProjects(request: APIRequestContext): Promise<E2eProjectRef[]> {
  const resp = await request.get(`${API_ORIGIN}/projects`);
  expect(resp.ok(), await resp.text()).toBeTruthy();
  const projects = (await resp.json()) as E2eProjectRef[];
  return projects.filter((project) =>
    typeof project.id === "string" && typeof project.name === "string"
  );
}

async function deleteProject(request: APIRequestContext, projectId: string): Promise<void> {
  const resp = await request.delete(`${API_ORIGIN}/projects/${projectId}`);
  if (resp.status() === 404) {
    return;
  }
  expect(resp.ok(), await resp.text()).toBeTruthy();
}
