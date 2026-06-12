import { describe, expect, it } from "vitest";

import type { AgentRunWorkspaceView } from "../types";
import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "../stores/workspaceTabStore";
import { deriveAgentRunWorkspaceChatControlState } from "./AgentRunWorkspacePage.chatControlState";
import {
  isConcreteCanvasPresentationUri,
  workspaceModulePresentedTabTarget,
} from "./AgentRunWorkspacePage.workspaceModulePresentation";

function workspaceView(
  controlStatus: AgentRunWorkspaceView["control_plane"]["status"],
  actions: AgentRunWorkspaceView["actions"],
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
    pending_messages: [],
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

function deriveControl(
  projectionStatus: "ready" | "refreshing" | "error" | "idle" | "loading",
  workspace: AgentRunWorkspaceView | null,
) {
  return deriveAgentRunWorkspaceChatControlState({
    isProjectAgentDraft: false,
    draftProjectIdValue: null,
    draftProjectAgentKey: null,
    draftProjectAgent: null,
    currentRunId: "run-1",
    currentAgentId: "agent-1",
    projectionStatus,
    projectionError: projectionStatus === "error" ? "refresh failed" : null,
    workspace,
  });
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

describe("AgentRun workspace chat control authority", () => {
  it("uses running ready projection for enqueue and Ctrl/Cmd+Enter steer", () => {
    const control = deriveControl("ready", workspaceView("running", runningActions));

    expect(control.primaryAction.kind).toBe("enqueue");
    expect(control.primaryAction.enabled).toBe(true);
    expect(control.secondaryAction?.kind).toBe("steer");
    expect(control.secondaryAction?.enabled).toBe(true);
  });

  it("makes a refreshing projection read-only even when the retained workspace has running actions", () => {
    const control = deriveControl("refreshing", workspaceView("running", runningActions));

    expect(control.primaryAction.kind).toBe("none");
    expect(control.primaryAction.enabled).toBe(false);
    expect(control.secondaryAction).toBeUndefined();
    expect(control.cancelAction.enabled).toBe(false);
  });

  it("does not expose steer or enqueue from a terminal projection with stale action bits", () => {
    const control = deriveControl(
      "ready",
      workspaceView("terminal", terminalActionsWithStaleRunningBits),
    );

    expect(control.controlPlaneStatus).toBe("terminal");
    expect(control.primaryAction.kind).toBe("none");
    expect(control.primaryAction.enabled).toBe(false);
    expect(control.secondaryAction).toBeUndefined();
  });

  it("keeps error and stale projection states read-only", () => {
    const errorControl = deriveControl("error", workspaceView("running", runningActions));
    const staleControl = deriveControl("idle", workspaceView("running", runningActions));

    expect(errorControl.primaryAction.kind).toBe("none");
    expect(errorControl.primaryAction.enabled).toBe(false);
    expect(staleControl.primaryAction.kind).toBe("none");
    expect(staleControl.primaryAction.enabled).toBe(false);
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
});
