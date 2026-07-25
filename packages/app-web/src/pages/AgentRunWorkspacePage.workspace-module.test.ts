import { describe, expect, it, vi } from "vitest";

import type { AgentRunWorkspaceView } from "../types";
import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "../stores/workspaceTabStore";
import {
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
  resolveExecutorConfigForConversationCommand,
} from "./AgentRunWorkspacePage.conversationCommandState";
import type {
  WorkspaceModuleDescriptor,
} from "../generated/workspace-module-contracts";
import type { WorkspaceModulePresentation } from "../generated/backbone-protocol";
import type {
  AgentRunOwnershipView,
  ConversationCommandView,
  ConversationKeyboardMapView,
} from "../generated/workflow-contracts";
import type { AgentRunWorkspaceListEntry, ProjectAgentSummary } from "../types";
import { collectCompanionSubagentRefs } from "./AgentRunWorkspacePage.companionRefs";
import {
  openUserCanvasModule,
  selectCanvasModuleOpenOptions,
} from "../features/workspace-panel/model/canvasModuleOpen";
import {
  isConcreteCanvasPresentationUri,
  isWorkspaceModulePresentationCurrent,
  workspaceModulePresentationFromPlatformEventData,
  workspaceModulePresentationTabTarget,
} from "./AgentRunWorkspacePage.workspaceModulePresentation";

const ownership: AgentRunOwnershipView = {
  run_created_by_user_id: "owner-user",
  agent_created_by_user_id: "owner-user",
  current_user_controls_run: true,
};

function workspaceView(
  controlStatus: "running" | "ready" | "completed" | "terminal",
  commands: ConversationCommandView[] = [],
  keyboard: ConversationKeyboardMapView = {},
): AgentRunWorkspaceView {
  return {
    run_ref: { run_id: "run-1" },
    agent_ref: { run_id: "run-1", agent_id: "agent-1" },
    project_id: "project-1",
    shell: {
      display_title: "Workspace",
      title_source: "session_meta",
      delivery_status: controlStatus,
      last_activity_at: "2026-06-12T00:00:00.000Z",
    },
    control_plane: {
      status: controlStatus === "running" ? "running" : controlStatus === "ready" ? "ready" : "terminal",
      ownership,
    },
    workspace_modules: [],
    agent: {
      agent_ref: { run_id: "run-1", agent_id: "agent-1" },
      project_id: "project-1",
      source: "project_agent",
      status: controlStatus,
      created_at: "2026-06-12T00:00:00.000Z",
      updated_at: "2026-06-12T00:00:00.000Z",
    },
    subject_associations: [],
    children: [],
    conversation: {
      snapshot_id: "snapshot-1",
      identity: {
        run_ref: { run_id: "run-1" },
        agent_ref: { run_id: "run-1", agent_id: "agent-1" },
        project_id: "project-1",
      },
      lifecycle_context: { subject_associations: [] },
      execution: {
        status: controlStatus === "running" ? "running_active" : controlStatus === "ready" ? "ready" : "terminal",
      },
      model_config: {
        status: "resolved",
        missing_fields: [],
      },
      commands: {
        ownership,
        commands,
        keyboard,
      },
      waiting_items: [],
      diagnostics: [],
    },
  };
}

describe("AgentRun list child presentation parity", () => {
  it("preserves Main display fields, depth-first nested order, and navigation coordinates", () => {
    const entries: AgentRunWorkspaceListEntry[] = [{
      run_ref: { run_id: "run-1" },
      agent_ref: { run_id: "run-1", agent_id: "agent-root" },
      title: "Root",
      lifecycle_status: "running",
      last_activity_at: "2026-07-10T00:00:00Z",
      source: "project_agent",
      subagent_count: 3,
      children: [{
        run_ref: { run_id: "run-1" },
        agent_ref: { run_id: "run-1", agent_id: "agent-child-a" },
        title: "Child A",
        lifecycle_status: "running",
        last_activity_at: "2026-07-10T00:01:00Z",
        source: "subagent",
        children: [{
          run_ref: { run_id: "run-1" },
          agent_ref: { run_id: "run-1", agent_id: "agent-grandchild" },
          title: "Grandchild",
          lifecycle_status: "completed",
          last_activity_at: "2026-07-10T00:02:00Z",
          source: "subagent",
          children: [],
        }],
      }, {
        run_ref: { run_id: "run-1" },
        agent_ref: { run_id: "run-1", agent_id: "agent-child-b" },
        title: "Child B",
        lifecycle_status: "failed",
        last_activity_at: "2026-07-10T00:03:00Z",
        source: "subagent",
        children: [],
      }],
    }];

    expect(collectCompanionSubagentRefs(entries, "run-1")).toEqual([
      {
        run_id: "run-1",
        agent_id: "agent-child-a",
        display_title: "Child A",
        delivery_status: "running",
        last_activity_at: "2026-07-10T00:01:00Z",
      },
      {
        run_id: "run-1",
        agent_id: "agent-grandchild",
        display_title: "Grandchild",
        delivery_status: "completed",
        last_activity_at: "2026-07-10T00:02:00Z",
      },
      {
        run_id: "run-1",
        agent_id: "agent-child-b",
        display_title: "Child B",
        delivery_status: "failed",
        last_activity_at: "2026-07-10T00:03:00Z",
      },
    ]);
  });
});

function commandState(
  workspaceStateStatus: "ready" | "refreshing" | "error" | "idle" | "loading",
  workspace: AgentRunWorkspaceView | null,
) {
  return buildAgentRunConversationCommandState({
    workspaceStateStatus,
    workspaceStateError: workspaceStateStatus === "error" ? "refresh failed" : null,
    conversation: workspace?.conversation,
  });
}

function command(kind: ConversationCommandView["kind"], commandId: string): ConversationCommandView {
  return {
    kind,
    command_id: commandId,
    enabled: true,
    requires_input: true,
    executor_config_policy: "required",
    placement: ["composer_primary"],
    stale_guard: {
      snapshot_id: `snapshot-${commandId}`,
      run_id: "run-1",
      agent_id: "agent-1",
      active_turn_id: undefined,
    },
  };
}

function presentation(params: {
  renderer_kind: string;
  presentation_uri: string;
  view_key?: string;
  module_id?: string;
  title?: string;
}): WorkspaceModulePresentation {
  return {
    module_id: params.module_id ?? "module-a",
    view_key: params.view_key ?? "preview",
    renderer_kind: params.renderer_kind,
    presentation_uri: params.presentation_uri,
    title: params.title ?? "Module A",
  };
}

describe("workspaceModulePresentationTabTarget", () => {
  it("opens Canvas tabs from presentation_uri", () => {
    expect(workspaceModulePresentationTabTarget(presentation({
      renderer_kind: "canvas",
      presentation_uri: "canvas://cvs-dashboard-a",
    }))).toEqual({
      typeId: "canvas",
      uri: "canvas://cvs-dashboard-a",
    });
  });

  it("does not treat empty canvas:// as a concrete Canvas tab target", () => {
    expect(isConcreteCanvasPresentationUri("canvas://")).toBe(false);
    expect(workspaceModulePresentationTabTarget(presentation({
      renderer_kind: "canvas",
      presentation_uri: "canvas://",
    }))).toBeNull();
  });

  it("does not infer Canvas URI from view_key or module_id", () => {
    expect(workspaceModulePresentationTabTarget(presentation({
      module_id: "canvas:cvs-dashboard-a",
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "",
    }))).toBeNull();
  });

  it("does not parse legacy uri fallback as presentation_uri", () => {
    expect(workspaceModulePresentationFromPlatformEventData({
      module_id: "canvas:cvs-dashboard-a",
      renderer_kind: "canvas",
      view_key: "preview",
      uri: "canvas://cvs-dashboard-a",
      title: "Dashboard",
    })).toBeNull();
  });

  it("parses stream payload with the generated presentation DTO shape", () => {
    expect(workspaceModulePresentationFromPlatformEventData({
      module_id: "canvas:cvs-dashboard-a",
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "canvas://cvs-dashboard-a",
      title: "Dashboard",
      payload: { source: "tool" },
      diagnostics: null,
    })).toEqual({
      module_id: "canvas:cvs-dashboard-a",
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "canvas://cvs-dashboard-a",
      title: "Dashboard",
      payload: { source: "tool" },
      diagnostics: null,
    });
  });

  it("opens non-Canvas module views by view_key", () => {
    expect(workspaceModulePresentationTabTarget(presentation({
      renderer_kind: "webview",
      view_key: "inspector",
      presentation_uri: "ext-demo://panel",
    }))).toEqual({
      typeId: "inspector",
      uri: "ext-demo://panel",
    });
  });

  it("只让仍存在于 current AgentRun projection 的精确 presentation 生效", () => {
    const module = canvasModule("canvas:cvs-dashboard-a", "canvas://cvs-dashboard-a");
    const current = presentation({
      module_id: "canvas:cvs-dashboard-a",
      renderer_kind: "canvas",
      presentation_uri: "canvas://cvs-dashboard-a",
    });

    expect(isWorkspaceModulePresentationCurrent(current, [module])).toBe(true);
    expect(isWorkspaceModulePresentationCurrent({
      ...current,
      presentation_uri: "canvas://deleted",
    }, [module])).toBe(false);
    expect(isWorkspaceModulePresentationCurrent(current, [])).toBe(false);
  });
});

describe("AgentRun workspace conversation command authority", () => {
  it("disables draft submit when model is required", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-1",
      display_name: "Agent",
      description: "",
      executor: {
        executor: "PI_AGENT",
        provider_id: null,
        model_id: null,
      },
      source: "project_agent",
    };
    const state = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      workspaceStateReady: true,
    });

    expect(state.executionStatus).toBe("model_required");
    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.commands).toHaveLength(0);
    expect(state.localDraftAction?.kind).toBe("draft_start_local");
    expect(state.localDraftAction?.enabled).toBe(false);
    expect(state.localDraftAction?.disabled_code).toBe("model_required");
  });

  it("enables draft submit after an explicit complete model override", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-1",
      display_name: "Agent",
      description: "",
      executor: {
        executor: "PI_AGENT",
        provider_id: null,
        model_id: null,
      },
      source: "project_agent",
    };
    const state = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      workspaceStateReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });

    const command = state.localDraftAction;
    expect(state.executionStatus).toBe("draft");
    expect(state.modelConfig.status).toBe("resolved");
    expect(state.modelConfig.effective_executor_config).toMatchObject({
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "gpt-5.4-mini",
      source: "user_override",
    });
    expect(command?.enabled).toBe(true);
    expect(state.commands.commands).toHaveLength(0);
    expect(state.commands.keyboard.enter).toBeUndefined();
  });

  it("keeps reasoning-capable model selection valid even without thinking level", () => {
    expect(buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent: {
        key: "agent-1",
        display_name: "Agent",
        description: "",
        executor: {
          executor: "PI_AGENT",
          provider_id: null,
          model_id: null,
        },
        source: "project_agent",
      },
      workspaceStateReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "reasoning-model",
      },
    }).modelConfig.status).toBe("resolved");
  });

  it("resolves local draft start payload executor_config from the explicit override", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-1",
      display_name: "Agent",
      description: "",
      executor: {
        executor: "PI_AGENT",
        provider_id: null,
        model_id: null,
      },
      source: "project_agent",
    };
    const state = buildDraftConversationCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      workspaceStateReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });
    const command = state.localDraftAction;
    expect(command).toBeDefined();
    if (!command) return;

    const executorConfig = resolveExecutorConfigForConversationCommand({
      command,
      modelConfig: state.modelConfig,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });

    expect({
      input: [],
      client_command_id: "cmd-1",
      executor_config: executorConfig,
    }).toMatchObject({
      executor_config: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });
  });

  it("uses snapshot keyboard mapping for ready Ctrl/Cmd+Enter submit_message", () => {
    const submit = command("submit_message", "cmd-submit");
    const state = commandState("ready", workspaceView("ready", [submit], {
      enter: "cmd-submit",
      ctrl_enter: "cmd-submit",
    }));

    expect(state.commands.keyboard.ctrl_enter).toBe("cmd-submit");
    expect(state.commands.commands.find((item) => item.command_id === "cmd-submit")?.kind).toBe("submit_message");
  });

  it("exposes running submit only when snapshot maps it", () => {
    const submit = {
      ...command("submit_message", "cmd-submit"),
      stale_guard: {
        snapshot_id: "snapshot-cmd-submit",
        run_id: "run-1",
        agent_id: "agent-1",
        active_turn_id: "turn-1",
      },
    };
    const state = commandState("ready", workspaceView("running", [submit], {
      enter: "cmd-submit",
      ctrl_enter: "cmd-submit",
    }));

    expect(state.commands.keyboard.enter).toBe("cmd-submit");
    expect(state.commands.keyboard.ctrl_enter).toBe("cmd-submit");
    expect(state.commands.commands.find((item) => item.command_id === "cmd-submit")?.stale_guard.active_turn_id).toBe("turn-1");
  });

  it("does not infer command enablement from top-level control_plane status", () => {
    const state = commandState("ready", workspaceView("running"));

    expect(state.executionStatus).toBe("running_active");
    expect(state.commands.commands).toHaveLength(0);
    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.keyboard.ctrl_enter).toBeUndefined();
  });

  it("keeps the committed conversation commands while projection is refreshing", () => {
    const state = commandState("refreshing", workspaceView("running", [
      command("submit_message", "cmd-submit"),
    ], { enter: "cmd-submit" }));

    expect(state.executionStatus).toBe("running_active");
    expect(state.commands.keyboard.enter).toBe("cmd-submit");
    expect(state.commands.commands).toHaveLength(1);
  });

  it("requires conversation snapshot before exposing commands", () => {
    const workspace = workspaceView("terminal");
    workspace.conversation = undefined;
    const state = commandState("ready", workspace);

    expect(state.executionStatus).toBe("ready");
    expect(state.commands.commands).toHaveLength(0);
  });
});

describe("workspaceTabStore Canvas tab identity", () => {
  const canvasLayoutOptions: WorkspaceTabLayoutOptions = {
    tabTypes: [{
      typeId: "canvas",
      label: "Canvas",
      allowMultiple: true,
      pinned: false,
      defaultUri: "canvas://",
      canCreateUri: (uri) => isConcreteCanvasPresentationUri(uri),
    }],
    resolveTitle: (_typeId, uri) => uri,
  };

  it("deduplicates the same concrete Canvas URI and keeps different Canvas URIs side by side", () => {
    useWorkspaceTabStore.getState().reset();

    const firstId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://cvs-mount-a", canvasLayoutOptions);
    const duplicateId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://cvs-mount-a", canvasLayoutOptions);
    const secondId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://cvs-mount-b", canvasLayoutOptions);

    const tabs = useWorkspaceTabStore.getState().tabs;
    expect(duplicateId).toBe(firstId);
    expect(secondId).not.toBe(firstId);
    expect(tabs.map((tab) => tab.uri)).toEqual([
      "canvas://cvs-mount-a",
      "canvas://cvs-mount-b",
    ]);

    useWorkspaceTabStore.getState().reset();
  });

  it("bumps refresh revision without changing Canvas tab identity", () => {
    useWorkspaceTabStore.getState().reset();

    const tabId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://cvs-mount-a", canvasLayoutOptions);
    useWorkspaceTabStore.getState().refreshTab(tabId);

    const tabs = useWorkspaceTabStore.getState().tabs;
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      id: tabId,
      typeId: "canvas",
      uri: "canvas://cvs-mount-a",
      refreshRevision: 1,
    });

    useWorkspaceTabStore.getState().reset();
  });

  it("rejects default empty canvas:// creation through add/open flows", () => {
    useWorkspaceTabStore.getState().reset();

    const addId = useWorkspaceTabStore
      .getState()
      .addTab("canvas", undefined, true, canvasLayoutOptions);
    const openId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://", canvasLayoutOptions);

    expect(addId).toBe("");
    expect(openId).toBe("");
    expect(useWorkspaceTabStore.getState().tabs).toEqual([]);

    useWorkspaceTabStore.getState().reset();
  });

  it("prunes a persisted Canvas tab that is absent from the current workspace projection", () => {
    useWorkspaceTabStore.getState().reset();
    useWorkspaceTabStore.getState().initialize("agentrun:run-1:agent-1", {
      tabs: [{
        type_id: "canvas",
        uri: "canvas://cvs-deleted",
        title: "Deleted Canvas",
        pinned: false,
      }],
      active_tab_uri: "canvas://cvs-deleted",
    }, canvasLayoutOptions);

    useWorkspaceTabStore.getState().pruneInvalidTabs({
      ...canvasLayoutOptions,
      tabTypes: [{
        ...canvasLayoutOptions.tabTypes[0],
        canCreateUri: (uri) => uri === "canvas://cvs-current",
      }],
    });

    expect(useWorkspaceTabStore.getState().tabs).toEqual([]);
    useWorkspaceTabStore.getState().reset();
  });
});

function canvasModule(
  moduleId: string,
  presentationUri: string | null,
  status: "ready" | "unavailable" = "ready",
): WorkspaceModuleDescriptor {
  return {
    summary: {
      module_id: moduleId,
      kind: "canvas",
      title: `Canvas ${moduleId}`,
      description: "",
      source: moduleId.replace("canvas:", ""),
      ui_summary: "preview",
      operation_summary: [],
      permission_summary: [],
      status: status === "ready"
        ? { kind: "ready" }
        : { kind: "unavailable", reason: "disabled" },
    },
    ui_entries: [{
      view_key: "preview",
      renderer_kind: "canvas",
      presentation_uri: presentationUri,
      title: `Preview ${moduleId}`,
    }],
    operations: [],
    runtime_backing: null,
  };
}

describe("Canvas workspace module selector and user-open flow", () => {
  it("selects only ready Canvas modules with concrete canonical presentation URIs", () => {
    const options = selectCanvasModuleOpenOptions([
      canvasModule("canvas:cvs-mount-a", "canvas://cvs-mount-a"),
      canvasModule("canvas:cvs-empty", "canvas://"),
      canvasModule("canvas:cvs-missing", null),
      canvasModule("canvas:cvs-disabled", "canvas://cvs-disabled", "unavailable"),
    ]);

    expect(options).toEqual([{
      module_id: "canvas:cvs-mount-a",
      view_key: "preview",
      title: "Preview canvas:cvs-mount-a",
      presentation_uri: "canvas://cvs-mount-a",
    }]);
  });

  it("uses the backend workspace module projection without a second surface join", () => {
    const options = selectCanvasModuleOpenOptions([
      canvasModule("canvas:cvs-mount-a", "canvas://cvs-mount-a"),
      canvasModule("canvas:cvs-mount-b", "canvas://cvs-mount-b"),
    ]);

    expect(options.map((option) => option.presentation_uri)).toEqual([
      "canvas://cvs-mount-a",
      "canvas://cvs-mount-b",
    ]);
  });

  it("opens an already active Canvas from the canonical project presentation URI", async () => {
    const openOrActivate = vi.fn();

    await openUserCanvasModule({
      option: {
        module_id: "canvas:cvs-mount-a",
        view_key: "preview",
        title: "Canvas A",
        presentation_uri: "canvas://cvs-candidate",
      },
      openOrActivate,
    });

    expect(openOrActivate).toHaveBeenCalledWith(
      "canvas",
      "canvas://cvs-candidate",
      true,
    );
  });

  it("does not open a tab without a concrete Canvas presentation", async () => {
    const openOrActivate = vi.fn();
    const option = {
      module_id: "canvas:cvs-mount-a",
      view_key: "preview",
      title: "Canvas A",
      presentation_uri: "canvas://cvs-candidate",
    };

    await expect(openUserCanvasModule({
      option,
      openOrActivate,
    })).resolves.toBeUndefined();
    expect(openOrActivate).toHaveBeenCalledWith("canvas", "canvas://cvs-candidate", true);

    openOrActivate.mockClear();
    await expect(openUserCanvasModule({
      option: {
        ...option,
        presentation_uri: "canvas://",
      },
      openOrActivate,
    })).rejects.toThrow("当前 Canvas 没有可打开的 presentation。");
    expect(openOrActivate).not.toHaveBeenCalled();
  });
});
