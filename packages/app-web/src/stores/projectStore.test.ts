import { beforeEach, describe, expect, it, vi } from "vitest";

import { useProjectStore } from "./projectStore";
import * as projectService from "../services/project";
import type { SettingEntry } from "../api/settings";
import type { JsonValue } from "../generated/common-contracts";
import type { Project } from "../types";
import {
  USER_WORKSPACE_STATE_SETTING_KEY,
  createEmptyUserWorkspaceState,
} from "../services/userWorkspaceState";

const mocks = vi.hoisted(() => ({
  settingsList: vi.fn(),
  settingsUpdate: vi.fn(),
  fetchProjects: vi.fn(),
  createProject: vi.fn(),
  updateProject: vi.fn(),
  updateProjectConfig: vi.fn(),
  fetchProjectGrants: vi.fn(),
  grantProjectUser: vi.fn(),
  revokeProjectUser: vi.fn(),
  grantProjectGroup: vi.fn(),
  revokeProjectGroup: vi.fn(),
  cloneProject: vi.fn(),
  fetchProjectAgents: vi.fn(),
  fetchProjectAgentConfigs: vi.fn(),
  createProjectAgent: vi.fn(),
  updateProjectAgent: vi.fn(),
  deleteProjectAgent: vi.fn(),
  createProjectAgentRun: vi.fn(),
  deleteProject: vi.fn(),
}));

vi.mock("../services/project", () => ({
  fetchProjects: mocks.fetchProjects,
  createProject: mocks.createProject,
  updateProject: mocks.updateProject,
  updateProjectConfig: mocks.updateProjectConfig,
  fetchProjectGrants: mocks.fetchProjectGrants,
  grantProjectUser: mocks.grantProjectUser,
  revokeProjectUser: mocks.revokeProjectUser,
  grantProjectGroup: mocks.grantProjectGroup,
  revokeProjectGroup: mocks.revokeProjectGroup,
  cloneProject: mocks.cloneProject,
  fetchProjectAgents: mocks.fetchProjectAgents,
  fetchProjectAgentConfigs: mocks.fetchProjectAgentConfigs,
  createProjectAgent: mocks.createProjectAgent,
  updateProjectAgent: mocks.updateProjectAgent,
  deleteProjectAgent: mocks.deleteProjectAgent,
  createProjectAgentRun: mocks.createProjectAgentRun,
  deleteProject: mocks.deleteProject,
}));

vi.mock("../api/settings", () => ({
  settingsApi: {
    list: mocks.settingsList,
    update: mocks.settingsUpdate,
  },
}));

describe("projectStore Project selection", () => {
  beforeEach(() => {
    resetProjectStore();
    vi.clearAllMocks();
    mocks.settingsList.mockResolvedValue([]);
    mocks.settingsUpdate.mockResolvedValue({ updated: [USER_WORKSPACE_STATE_SETTING_KEY] });
  });

  it("restores the persisted current Project after loading projects", async () => {
    mocks.settingsList.mockResolvedValue([
      setting({
        schema_version: 1,
        navigation: { current_project_id: "project-old" },
        recent: { project_ids: ["project-old"] },
      }),
    ]);
    mocks.fetchProjects.mockResolvedValue([
      project("project-new"),
      project("project-old"),
    ]);

    await useProjectStore.getState().fetchProjects();

    expect(useProjectStore.getState().currentProjectId).toBe("project-old");
    expect(mocks.settingsUpdate).not.toHaveBeenCalled();
  });

  it("repairs unavailable persisted Project selection to a visible Project", async () => {
    mocks.settingsList.mockResolvedValue([
      setting({
        schema_version: 1,
        navigation: { current_project_id: "missing-project" },
        recent: { project_ids: ["missing-project"] },
      }),
    ]);
    mocks.fetchProjects.mockResolvedValue([project("project-new")]);

    await useProjectStore.getState().fetchProjects();

    expect(useProjectStore.getState().currentProjectId).toBe("project-new");
    expect(mocks.settingsUpdate).toHaveBeenCalledWith(
      { scope: "user" },
      [{
        key: USER_WORKSPACE_STATE_SETTING_KEY,
        value: {
          schema_version: 1,
          navigation: { current_project_id: "project-new" },
          recent: { project_ids: ["project-new"] },
        },
      }],
    );
  });

  it("persists explicit Project selection", () => {
    useProjectStore.setState({
      projects: [project("project-1"), project("project-2")],
      userWorkspaceState: createEmptyUserWorkspaceState(),
    });

    useProjectStore.getState().selectProject("project-2");

    expect(useProjectStore.getState().currentProjectId).toBe("project-2");
    expect(mocks.settingsUpdate).toHaveBeenCalledWith(
      { scope: "user" },
      [{
        key: USER_WORKSPACE_STATE_SETTING_KEY,
        value: {
          schema_version: 1,
          navigation: { current_project_id: "project-2" },
          recent: { project_ids: ["project-2"] },
        },
      }],
    );
  });

  it("selects and persists newly created Project", async () => {
    mocks.createProject.mockResolvedValue(project("project-new"));
    useProjectStore.setState({
      projects: [project("project-old")],
      currentProjectId: "project-old",
      userWorkspaceState: createEmptyUserWorkspaceState(),
    });

    await useProjectStore.getState().createProject("New", "");

    expect(useProjectStore.getState().projects.map((item) => item.id)).toEqual([
      "project-new",
      "project-old",
    ]);
    expect(useProjectStore.getState().currentProjectId).toBe("project-new");
    expect(mocks.settingsUpdate).toHaveBeenCalledWith(
      { scope: "user" },
      [{
        key: USER_WORKSPACE_STATE_SETTING_KEY,
        value: {
          schema_version: 1,
          navigation: { current_project_id: "project-new" },
          recent: { project_ids: ["project-new"] },
        },
      }],
    );
  });

  it("falls back and persists when deleting the current Project", async () => {
    mocks.deleteProject.mockResolvedValue(undefined);
    useProjectStore.setState({
      projects: [project("project-current"), project("project-next")],
      currentProjectId: "project-current",
      userWorkspaceState: {
        schema_version: 1,
        navigation: { current_project_id: "project-current" },
        recent: { project_ids: ["project-current", "project-next"] },
      },
    });

    await useProjectStore.getState().deleteProject("project-current");

    expect(useProjectStore.getState().currentProjectId).toBe("project-next");
    expect(mocks.settingsUpdate).toHaveBeenCalledWith(
      { scope: "user" },
      [{
        key: USER_WORKSPACE_STATE_SETTING_KEY,
        value: {
          schema_version: 1,
          navigation: { current_project_id: "project-next" },
          recent: { project_ids: ["project-next"] },
        },
      }],
    );
  });
});

describe("projectStore AgentRun commands", () => {
  beforeEach(() => {
    resetProjectStore();
    vi.clearAllMocks();
  });

  it("propagates createProjectAgentRun API errors", async () => {
    const error = new Error("缺少模型选择");
    vi.mocked(projectService.createProjectAgentRun).mockRejectedValue(error);

    await expect(useProjectStore.getState().createProjectAgentRun("project-1", "agent-1", {
      input: [],
      client_command_id: "cmd-1",
    })).rejects.toThrow("缺少模型选择");
    expect(useProjectStore.getState().error).toBe("缺少模型选择");
  });
});

function resetProjectStore(): void {
  useProjectStore.setState({
    projects: [],
    agentsByProjectId: {},
    grantsByProjectId: {},
    currentProjectId: null,
    userWorkspaceState: createEmptyUserWorkspaceState(),
    isLoading: false,
    error: null,
    projectAgentConfigsByProjectId: {},
    vfsMountsRevision: {},
  });
}

function project(id: string): Project {
  return {
    id,
    name: id,
    description: "",
    config: {
      default_agent_type: null,
      agent_presets: [],
      context_containers: [],
      default_workspace_id: null,
      scheduling: {
        stall_timeout_ms: null,
      },
    },
    created_by_user_id: "user",
    updated_by_user_id: "user",
    visibility: "private",
    is_template: false,
    cloned_from_project_id: null,
    access: {
      role: "owner",
      can_use: true,
      can_configure: true,
      can_manage_sharing: true,
      via_admin_bypass: false,
      via_template_visibility: false,
    },
    created_at: new Date(0).toISOString(),
    updated_at: new Date(0).toISOString(),
  };
}

function setting(value: JsonValue): SettingEntry {
  return {
    scope_kind: "user",
    key: USER_WORKSPACE_STATE_SETTING_KEY,
    value,
    updated_at: new Date(0).toISOString(),
    masked: false,
  };
}
