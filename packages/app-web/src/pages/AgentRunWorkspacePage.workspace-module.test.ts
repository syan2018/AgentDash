import { describe, expect, it, vi } from "vitest";

import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "../stores/workspaceTabStore";
import {
  buildAgentRunConversationCommandState,
  buildDraftConversationCommandState,
  resolveExecutorConfigForConversationCommand,
} from "./AgentRunWorkspacePage.conversationCommandState";
import type {
  WorkspaceModuleDescriptor,
  WorkspaceModulePresentation,
} from "../generated/workspace-module-contracts";
import type {
  ConversationModelConfigView,
} from "../generated/workflow-contracts";
import type { RuntimeSnapshot } from "../generated/agent-runtime-contracts";
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

const resolvedModelConfig: ConversationModelConfigView = {
  status: "resolved",
  missing_fields: [],
  effective_executor_config: {
    executor: "CODEX",
    source: "frame_execution_profile",
  },
};

function runtimeSnapshot(activeTurnId: string | null): RuntimeSnapshot {
  return {
    thread_id: "thread-1",
    revision: 1n,
    latest_event_sequence: 0n,
    captured_at_ms: 0n,
    status: "active",
    active_turn_id: activeTurnId,
    binding_id: "binding-1",
    binding_epoch: 0n,
    profile_digest: "sha256:profile",
    bound_profile: {
      reference_class: "managed_thread",
      input: { modalities: ["text"] },
      instruction: { channels: ["system"], configuration_boundary: "thread_start" },
      tools: { channels: [], configuration_boundary: "turn_start", cancellation: true },
      workspace: { capabilities: [], mechanism: "host_adapted_exact" },
      interactions: { kinds: [], durable_correlation: true },
      lifecycle: ["turn_start", "turn_steer"],
      hooks: { points: [], configuration_boundary: "thread_start" },
      context: { capabilities: ["read"], fidelity: "platform_exact", activation_idempotent: true },
      telemetry_config: [],
    },
    active_checkpoint_id: null,
    context_revision: 1n,
    settings_revision: 1n,
    tool_set_revision: 1n,
    pending_interactions: [],
    command_availability: {
      [activeTurnId ? "turn_steer" : "turn_start"]: { status: "available" },
    },
    transcript: [],
    transcript_fidelity: "platform_exact",
  };
}

function commandState(
  workspaceStateStatus: "ready" | "refreshing" | "error" | "idle" | "loading",
  snapshot: RuntimeSnapshot | null,
) {
  return buildAgentRunConversationCommandState({
    workspaceStateStatus,
    workspaceStateError: workspaceStateStatus === "error" ? "refresh failed" : null,
    modelConfig: resolvedModelConfig,
    runtimeSnapshot: snapshot,
  });
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

  it("splits local draft model selection from the ProjectAgent executor", () => {
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
      model_selection: {
        provider_id: executorConfig?.provider_id,
        model_id: executorConfig?.model_id,
        agent_id: executorConfig?.agent_id,
        thinking_level: executorConfig?.thinking_level,
      },
    }).toMatchObject({
      model_selection: {
        provider_id: "openai",
        model_id: "gpt-5.4-mini",
      },
    });
  });

  it("uses Runtime snapshot availability for ready submit", () => {
    const state = commandState("ready", runtimeSnapshot(null));

    expect(state.commands.keyboard.enter).toBe("runtime:turn_start");
    expect(state.commands.keyboard.ctrl_enter).toBeUndefined();
    expect(state.commands.commands.find((item) => item.command_id === "runtime:turn_start")?.kind).toBe("submit_message");
  });

  it("switches running submit to Runtime steer", () => {
    const state = commandState("ready", runtimeSnapshot("turn-1"));

    expect(state.commands.keyboard.enter).toBe("runtime:turn_steer");
    expect(state.activeTurnId).toBe("turn-1");
  });

  it("does not infer command enablement without a Runtime snapshot", () => {
    const state = commandState("ready", null);

    expect(state.executionStatus).toBe("ready");
    expect(state.commands.commands).toHaveLength(0);
    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.keyboard.ctrl_enter).toBeUndefined();
  });

  it("freezes stale backend commands while projection is refreshing", () => {
    const state = commandState("refreshing", runtimeSnapshot("turn-1"));

    expect(state.executionStatus).toBe("refreshing");
    expect(state.commands.keyboard.enter).toBeUndefined();
    expect(state.commands.commands).toHaveLength(0);
  });

  it("requires Runtime snapshot before exposing commands", () => {
    const state = commandState("ready", null);

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
