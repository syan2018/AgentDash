import { describe, expect, it, vi } from "vitest";

import type { AgentRunWorkspaceView } from "../types";
import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "../stores/workspaceTabStore";
import {
  buildDraftSessionCommandState,
  buildRuntimeSessionCommandState,
  resolveExecutorConfigForConversationCommand,
} from "./AgentRunWorkspacePage.conversationCommandState";
import type {
  WorkspaceModuleDescriptor,
  WorkspaceModulePresentation,
} from "../generated/workspace-module-contracts";
import type {
  AgentRunOwnershipView,
  ConversationCommandView,
  ConversationKeyboardMapView,
} from "../generated/workflow-contracts";
import type { ProjectAgentSummary } from "../types";
import {
  activeCanvasMountIdsFromRuntimeSurface,
  openUserCanvasModule,
  selectCanvasModuleOpenOptions,
} from "../features/workspace-panel/model/canvasModuleOpen";
import {
  isConcreteCanvasPresentationUri,
  workspaceModulePresentationFromPlatformEventData,
  workspaceModulePresentedTabTarget,
} from "./AgentRunWorkspacePage.workspaceModulePresentation";

const ownership: AgentRunOwnershipView = {
  run_created_by_user_id: "owner-user",
  agent_created_by_user_id: "owner-user",
  current_user_controls_run: true,
};

function workspaceView(
  controlStatus: AgentRunWorkspaceView["control_plane"]["status"],
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
    control_plane: { status: controlStatus, ownership },
    subject_associations: [],
    children: [],
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
        ownership,
        commands,
        keyboard,
      },
      mailbox: {
        visible_message_count: 0,
        paused: false,
        user_attention: false,
        messages: [],
      },
      diagnostics: [],
    },
  };
}

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
    placement: ["composer_primary"],
    stale_guard: {
      snapshot_id: `snapshot-${commandId}`,
      run_id: "run-1",
      agent_id: "agent-1",
      runtime_session_id: "session-1",
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

describe("workspaceModulePresentedTabTarget", () => {
  it("opens Canvas tabs from presentation_uri", () => {
    expect(workspaceModulePresentedTabTarget(presentation({
      renderer_kind: "canvas",
      presentation_uri: "canvas://cvs-dashboard-a",
    }))).toEqual({
      typeId: "canvas",
      uri: "canvas://cvs-dashboard-a",
      refreshRuntime: true,
    });
  });

  it("does not treat empty canvas:// as a concrete Canvas tab target", () => {
    expect(isConcreteCanvasPresentationUri("canvas://")).toBe(false);
    expect(workspaceModulePresentedTabTarget(presentation({
      renderer_kind: "canvas",
      presentation_uri: "canvas://",
    }))).toBeNull();
  });

  it("does not infer Canvas URI from view_key or module_id", () => {
    expect(workspaceModulePresentedTabTarget(presentation({
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
    expect(workspaceModulePresentedTabTarget(presentation({
      renderer_kind: "webview",
      view_key: "inspector",
      presentation_uri: "ext-demo://panel",
    }))).toEqual({
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
        runtime_session_id: "session-1",
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

  it("keeps backend commands while projection is refreshing", () => {
    const state = commandState("refreshing", workspaceView("running", [
      command("submit_message", "cmd-submit"),
    ], { enter: "cmd-submit" }));

    expect(state.executionStatus).toBe("running_active");
    expect(state.commands.keyboard.enter).toBe("cmd-submit");
    expect(state.commands.commands.find((item) => item.command_id === "cmd-submit")?.enabled).toBe(true);
  });

  it("requires conversation snapshot before exposing commands", () => {
    const workspace = workspaceView("terminal");
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

  it("filters Canvas menu options to the current runtime surface", () => {
    const activeCanvasMountIds = activeCanvasMountIdsFromRuntimeSurface({
      surface_ref: "session_runtime:session-1",
      source: { source_type: "session_runtime", session_id: "session-1" },
      mounts: [{
        id: "cvs-mount-a",
        display_name: "Canvas A",
        provider: "canvas_fs",
        backend_id: "",
        capabilities: ["read"],
        default_write: false,
        purpose: "canvas",
        edit_capabilities: { create: true, delete: true, rename: true },
      }],
    });
    const options = selectCanvasModuleOpenOptions([
      canvasModule("canvas:cvs-mount-a", "canvas://cvs-mount-a"),
      canvasModule("canvas:cvs-mount-b", "canvas://cvs-mount-b"),
    ], activeCanvasMountIds);

    expect(options.map((option) => option.presentation_uri)).toEqual(["canvas://cvs-mount-a"]);
  });

  it("opens an already active Canvas from the canonical project presentation URI", async () => {
    const openOrActivate = vi.fn();

    await openUserCanvasModule({
      runtimeSessionId: "session-1",
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

  it("does not open a tab without a runtime session or concrete Canvas presentation", async () => {
    const openOrActivate = vi.fn();
    const option = {
      module_id: "canvas:cvs-mount-a",
      view_key: "preview",
      title: "Canvas A",
      presentation_uri: "canvas://cvs-candidate",
    };

    await expect(openUserCanvasModule({
      runtimeSessionId: null,
      option,
      openOrActivate,
    })).rejects.toThrow("当前 AgentRun 尚未就绪，无法打开 Canvas。");
    expect(openOrActivate).not.toHaveBeenCalled();

    await expect(openUserCanvasModule({
      runtimeSessionId: "session-1",
      option: {
        ...option,
        presentation_uri: "canvas://",
      },
      openOrActivate,
    })).rejects.toThrow("当前 Canvas 没有可打开的 presentation。");
    expect(openOrActivate).not.toHaveBeenCalled();
  });
});
