import { describe, expect, it, vi } from "vitest";

import type { AgentRunWorkspaceView } from "../types";
import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "../stores/workspaceTabStore";
import {
  buildDraftSessionCommandState,
  buildRuntimeSessionCommandState,
  resolveExecutorConfigForConversationCommand,
} from "./AgentRunWorkspacePage.conversationCommandState";
import type { WorkspaceModuleDescriptor } from "../generated/workspace-module-contracts";
import type { ConversationCommandView, ConversationKeyboardMapView } from "../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../types";
import {
  openUserCanvasModule,
  selectCanvasModuleOpenOptions,
} from "../features/workspace-panel/model/canvasModuleOpen";
import {
  isConcreteCanvasPresentationUri,
  workspaceModulePresentedTabTarget,
} from "./AgentRunWorkspacePage.workspaceModulePresentation";

function workspaceView(
  controlStatus: AgentRunWorkspaceView["control_plane"]["status"],
  actions: AgentRunWorkspaceView["actions"],
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
      workspace_status: controlStatus,
      delivery_status: controlStatus,
      last_activity_at: "2026-06-12T00:00:00.000Z",
    },
    delivery_runtime_ref: { runtime_session_id: "session-1" },
    control_plane: { status: controlStatus },
    subject_associations: [],
    actions,
    pending_queue: {
      paused: false,
      can_resume: false,
    },
    pending_messages: [],
    conversation: {
      snapshot_id: "snapshot-1",
      identity: {
        run_ref: { run_id: "run-1" },
        agent_ref: { run_id: "run-1", agent_id: "agent-1" },
        project_id: "project-1",
      },
      lifecycle_context: {
        delivery_runtime_ref: { runtime_session_id: "session-1" },
        subject_associations: [],
      },
      execution: {
        status: controlStatus === "running" ? "running_active" : controlStatus === "ready" ? "ready" : "terminal",
      },
      model_config: {
        status: "resolved",
        missing_fields: [],
      },
      commands: {
        commands,
        keyboard,
      },
      pending: {
        visible_message_count: 0,
        paused: false,
        user_attention: false,
      },
      diagnostics: [],
    },
  };
}

const runningActions: AgentRunWorkspaceView["actions"] = {
  send_next: { enabled: false, unavailable_reason: "running" },
  enqueue: { enabled: true },
  steer: { enabled: true },
  cancel: { enabled: true },
};

const terminalActionsWithStaleRunningBits: AgentRunWorkspaceView["actions"] = {
  send_next: { enabled: false, unavailable_reason: "terminal" },
  enqueue: { enabled: true },
  steer: { enabled: true },
  cancel: { enabled: false, unavailable_reason: "terminal" },
};

function commandState(
  projectionStatus: "ready" | "refreshing" | "error" | "idle" | "loading",
  workspace: AgentRunWorkspaceView | null,
) {
  return buildRuntimeSessionCommandState({
    projectionStatus,
    projectionError: projectionStatus === "error" ? "refresh failed" : null,
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
    placement: kind === "steer" ? ["composer_secondary"] : ["composer_primary"],
    stale_guard: {
      snapshot_id: `snapshot-${commandId}`,
      run_id: "run-1",
      agent_id: "agent-1",
      runtime_session_id: "session-1",
      active_turn_id: kind === "steer" ? "turn-1" : undefined,
    },
  };
}

describe("workspaceModulePresentedTabTarget", () => {
  it("opens Canvas tabs from presentation_uri", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "canvas://dashboard-a",
    })).toEqual({
      typeId: "canvas",
      uri: "canvas://dashboard-a",
      refreshRuntime: true,
    });
  });

  it("does not treat empty canvas:// as a concrete Canvas tab target", () => {
    expect(isConcreteCanvasPresentationUri("canvas://")).toBe(false);
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "canvas://",
    })).toBeNull();
  });

  it("does not infer Canvas URI from view_key", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
    })).toBeNull();
  });

  it("does not open Canvas tabs from legacy uri fallback", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
      uri: "canvas://dashboard-a",
    })).toBeNull();
  });

  it("opens non-Canvas module views by view_key", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "webview",
      view_key: "inspector",
      presentation_uri: "ext-demo://panel",
    })).toEqual({
      typeId: "inspector",
      uri: "ext-demo://panel",
      refreshRuntime: false,
    });
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
    const state = buildDraftSessionCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      projectionReady: true,
    });

    expect(state.executionStatus).toBe("model_required");
    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.commands[0]?.enabled).toBe(false);
    expect(state.commands.commands[0]?.disabled_code).toBe("model_required");
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
    const state = buildDraftSessionCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      projectionReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });

    const command = state.commands.commands[0];
    expect(state.executionStatus).toBe("draft");
    expect(state.modelConfig.status).toBe("resolved");
    expect(state.modelConfig.effective_executor_config).toMatchObject({
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "gpt-5.4-mini",
      source: "user_override",
    });
    expect(command?.enabled).toBe(true);
    expect(state.commands.keyboard.enter).toBe(command?.command_id);
  });

  it("keeps reasoning-capable model selection valid even without thinking level", () => {
    expect(buildDraftSessionCommandState({
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
      projectionReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "reasoning-model",
      },
    }).modelConfig.status).toBe("resolved");
  });

  it("resolves start_draft payload executor_config from the explicit override", () => {
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
    const state = buildDraftSessionCommandState({
      projectId: "project-1",
      agentKey: "agent-1",
      agent,
      projectionReady: true,
      explicitExecutorConfigOverride: {
        executor: "PI_AGENT",
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });
    const command = state.commands.commands[0];
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

  it("uses snapshot keyboard mapping for ready Ctrl/Cmd+Enter send_next", () => {
    const sendNext = command("send_next", "cmd-send-next");
    const state = commandState("ready", workspaceView("ready", runningActions, [sendNext], {
      enter: "cmd-send-next",
      ctrl_enter: "cmd-send-next",
    }));

    expect(state.commands.keyboard.ctrl_enter).toBe("cmd-send-next");
    expect(state.commands.commands.find((item) => item.command_id === "cmd-send-next")?.kind).toBe("send_next");
  });

  it("exposes running steer only when snapshot maps it", () => {
    const enqueue = command("enqueue", "cmd-enqueue");
    const steer = command("steer", "cmd-steer");
    const state = commandState("ready", workspaceView("running", runningActions, [enqueue, steer], {
      enter: "cmd-enqueue",
      ctrl_enter: "cmd-steer",
    }));

    expect(state.commands.keyboard.enter).toBe("cmd-enqueue");
    expect(state.commands.keyboard.ctrl_enter).toBe("cmd-steer");
    expect(state.commands.commands.find((item) => item.command_id === "cmd-steer")?.stale_guard.active_turn_id).toBe("turn-1");
  });

  it("does not fabricate commands while projection is refreshing", () => {
    const state = commandState("refreshing", workspaceView("running", runningActions, [
      command("enqueue", "cmd-enqueue"),
    ], { enter: "cmd-enqueue" }));

    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.commands).toHaveLength(0);
  });

  it("requires conversation snapshot instead of falling back to stale action bits", () => {
    const workspace = workspaceView("terminal", terminalActionsWithStaleRunningBits);
    workspace.conversation = undefined;
    const state = commandState("ready", workspace);

    expect(state.executionStatus).toBe("delivery_missing");
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
      .openOrActivate("canvas", "canvas://mount-a", canvasLayoutOptions);
    const duplicateId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://mount-a", canvasLayoutOptions);
    const secondId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://mount-b", canvasLayoutOptions);

    const tabs = useWorkspaceTabStore.getState().tabs;
    expect(duplicateId).toBe(firstId);
    expect(secondId).not.toBe(firstId);
    expect(tabs.map((tab) => tab.uri)).toEqual(["canvas://mount-a", "canvas://mount-b"]);

    useWorkspaceTabStore.getState().reset();
  });

  it("bumps refresh revision without changing Canvas tab identity", () => {
    useWorkspaceTabStore.getState().reset();

    const tabId = useWorkspaceTabStore
      .getState()
      .openOrActivate("canvas", "canvas://mount-a", canvasLayoutOptions);
    useWorkspaceTabStore.getState().refreshTab(tabId);

    const tabs = useWorkspaceTabStore.getState().tabs;
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      id: tabId,
      typeId: "canvas",
      uri: "canvas://mount-a",
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
      canvasModule("canvas:mount-a", "canvas://mount-a"),
      canvasModule("canvas:empty", "canvas://"),
      canvasModule("canvas:missing", null),
      canvasModule("canvas:disabled", "canvas://disabled", "unavailable"),
    ]);

    expect(options).toEqual([{
      module_id: "canvas:mount-a",
      view_key: "preview",
      title: "Preview canvas:mount-a",
      presentation_uri: "canvas://mount-a",
    }]);
  });

  it("opens Canvas from the backend user-open presentation, not the project candidate URI", async () => {
    const presentWorkspaceModule = vi.fn().mockResolvedValue({
      module_id: "canvas:mount-a",
      view_key: "preview",
      renderer_kind: "canvas",
      presentation_uri: "canvas://canonical-from-backend",
      title: "Canvas A",
    });
    const openOrActivate = vi.fn();

    await openUserCanvasModule({
      projectId: "project-1",
      runtimeSessionId: "session-1",
      option: {
        module_id: "canvas:mount-a",
        view_key: "preview",
        title: "Canvas A",
        presentation_uri: "canvas://candidate",
      },
      presentWorkspaceModule,
      openOrActivate,
    });

    expect(presentWorkspaceModule).toHaveBeenCalledWith("project-1", {
      module_id: "canvas:mount-a",
      view_key: "preview",
      runtime_session_id: "session-1",
    });
    expect(openOrActivate).toHaveBeenCalledWith(
      "canvas",
      "canvas://canonical-from-backend",
      true,
    );
  });

  it("does not open a tab when user-open fails or returns no concrete Canvas presentation", async () => {
    const openOrActivate = vi.fn();
    const option = {
      module_id: "canvas:mount-a",
      view_key: "preview",
      title: "Canvas A",
      presentation_uri: "canvas://candidate",
    };

    await expect(openUserCanvasModule({
      projectId: "project-1",
      runtimeSessionId: "session-1",
      option,
      presentWorkspaceModule: vi.fn().mockRejectedValue(new Error("backend failed")),
      openOrActivate,
    })).rejects.toThrow("backend failed");
    expect(openOrActivate).not.toHaveBeenCalled();

    await expect(openUserCanvasModule({
      projectId: "project-1",
      runtimeSessionId: "session-1",
      option,
      presentWorkspaceModule: vi.fn().mockResolvedValue({
        module_id: "canvas:mount-a",
        view_key: "preview",
        renderer_kind: "canvas",
        presentation_uri: "canvas://",
      }),
      openOrActivate,
    })).rejects.toThrow("后端未返回可打开的 Canvas presentation。");
    expect(openOrActivate).not.toHaveBeenCalled();
  });
});
