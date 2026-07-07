/// <reference types="node" />

import { readFileSync } from "node:fs";
import { describe, expect, it, beforeEach, vi } from "vitest";

import type { ProjectEventStreamEnvelope } from "../../generated/project-contracts";
import { fetchProjectAgentRuns } from "../../services/lifecycle";
import type { AgentRunWorkspaceListEntry, AgentRunWorkspaceListView } from "../../types";
import {
  invalidateAgentRunListStateForProjectEvent,
  useAgentRunListStateStore,
} from "./agent-run-list-state-store";

vi.mock("../../services/lifecycle", () => ({
  fetchProjectAgentRuns: vi.fn(),
}));

const mockedFetchProjectAgentRuns = vi.mocked(fetchProjectAgentRuns);

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve: (value: T) => void = () => {};
  let reject: (reason?: unknown) => void = () => {};
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function agentRunEntry(
  runId: string,
  agentId: string,
  title: string,
  lastActivityAt: string,
): AgentRunWorkspaceListEntry {
  return {
    run_ref: { run_id: runId },
    agent_ref: { run_id: runId, agent_id: agentId },
    project_id: "project-1",
    shell: {
      display_title: title,
      title_source: "runtime_session",
      delivery_status: "idle",
      last_activity_at: lastActivityAt,
    },
    run_status: "ready",
    project_agent_label: title,
    source: "project_agent",
    subagent_count: 0,
    children: [],
  };
}

function listView(entries: AgentRunWorkspaceListEntry[], nextCursor?: string): AgentRunWorkspaceListView {
  return {
    project_id: "project-1",
    agent_runs: entries,
    next_cursor: nextCursor,
  };
}

function projectStateChanged(projectId: string): ProjectEventStreamEnvelope {
  return {
    type: "StateChanged",
    data: {
      id: 1,
      project_id: projectId,
      entity_id: "story-1",
      kind: "story_updated",
      payload: {},
      backend_id: null,
      created_at: "2026-06-25T00:00:00Z",
    },
  };
}

function agentRunListInvalidated(projectId: string): ProjectEventStreamEnvelope {
  return {
    type: "ControlPlaneProjectionChanged",
    data: {
      project_id: projectId,
      change: {
        projection: "agent_run_list",
        reason: "agent_run_lineage_changed",
        run_id: "run-1",
        agent_id: "agent-1",
        frame_id: null,
        gate_id: null,
        mailbox_message_id: null,
        delivery_runtime_session_id: null,
        workspace_module_presentation: null,
      },
    },
  };
}

function mailboxInvalidated(projectId: string): ProjectEventStreamEnvelope {
  return {
    type: "ControlPlaneProjectionChanged",
    data: {
      project_id: projectId,
      change: {
        projection: "mailbox",
        reason: "mailbox_state_changed",
        run_id: "run-1",
        agent_id: "agent-1",
        frame_id: null,
        gate_id: null,
        mailbox_message_id: null,
        delivery_runtime_session_id: null,
        workspace_module_presentation: null,
      },
    },
  };
}

describe("agent-run list state store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAgentRunListStateStore.setState({ byProjectId: {} });
  });

  it("保持后端列表顺序，不按 shell.last_activity_at 二次重排", async () => {
    const older = agentRunEntry("run-old", "agent-old", "后端第一条", "2026-06-25T01:00:00Z");
    const newer = agentRunEntry("run-new", "agent-new", "后端第二条", "2026-06-25T02:00:00Z");
    mockedFetchProjectAgentRuns.mockResolvedValueOnce(listView([older, newer]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledWith("project-1", { limit: 30 });
    expect(
      useAgentRunListStateStore
        .getState()
        .byProjectId["project-1"]
        ?.entries
        .map((entry) => entry.run_ref.run_id),
    ).toEqual(["run-old", "run-new"]);
  });

  it("Project 事件触发同一 Project 的 list state refresh", async () => {
    const before = agentRunEntry("run-1", "agent-1", "刷新前", "2026-06-25T01:00:00Z");
    const after = agentRunEntry("run-2", "agent-2", "刷新后", "2026-06-25T02:00:00Z");
    mockedFetchProjectAgentRuns
      .mockResolvedValueOnce(listView([before]))
      .mockResolvedValueOnce(listView([after]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListStateForProjectEvent(projectStateChanged("project-1"), "project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(2);
    expect(
      useAgentRunListStateStore.getState().byProjectId["project-1"]?.entries[0]?.run_ref.run_id,
    ).toBe("run-2");
  });

  it("project-scoped AgentRunList projection invalidation 触发列表 refresh", async () => {
    const before = agentRunEntry("run-1", "agent-1", "刷新前", "2026-06-25T01:00:00Z");
    const after = agentRunEntry("run-child", "agent-child", "SubAgent", "2026-06-25T02:00:00Z");
    mockedFetchProjectAgentRuns
      .mockResolvedValueOnce(listView([before]))
      .mockResolvedValueOnce(listView([after]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListStateForProjectEvent(
      agentRunListInvalidated("project-1"),
      "project-1",
    );

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(2);
    expect(
      useAgentRunListStateStore.getState().byProjectId["project-1"]?.entries[0]?.run_ref.run_id,
    ).toBe("run-child");
  });

  it("first-page refresh in-flight 时收到 AgentRunList invalidation 会串行补一次刷新", async () => {
    const stale = agentRunEntry("run-stale", "agent-stale", "旧快照", "2026-06-25T01:00:00Z");
    const fresh = agentRunEntry("run-fresh", "agent-fresh", "新快照", "2026-06-25T02:00:00Z");
    const firstRefresh = deferred<AgentRunWorkspaceListView>();
    const secondRefresh = deferred<AgentRunWorkspaceListView>();
    mockedFetchProjectAgentRuns
      .mockReturnValueOnce(firstRefresh.promise)
      .mockReturnValueOnce(secondRefresh.promise);

    const initialRefresh = useAgentRunListStateStore
      .getState()
      .refreshProject("project-1", "initial");
    await vi.waitFor(() => {
      expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
    });

    const invalidationRefresh = invalidateAgentRunListStateForProjectEvent(
      agentRunListInvalidated("project-1"),
      "project-1",
    );
    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);

    firstRefresh.resolve(listView([stale]));
    await vi.waitFor(() => {
      expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(2);
    });
    expect(mockedFetchProjectAgentRuns).toHaveBeenLastCalledWith("project-1", { limit: 30 });

    secondRefresh.resolve(listView([fresh]));
    await Promise.all([initialRefresh, invalidationRefresh]);

    expect(
      useAgentRunListStateStore.getState().byProjectId["project-1"]?.entries[0]?.run_ref.run_id,
    ).toBe("run-fresh");
  });

  it("first-page refresh 失败时不会因 dirty generation 无限重试，下一次失效可恢复", async () => {
    const fresh = agentRunEntry("run-fresh", "agent-fresh", "恢复快照", "2026-06-25T02:00:00Z");
    const firstRefresh = deferred<AgentRunWorkspaceListView>();
    mockedFetchProjectAgentRuns
      .mockReturnValueOnce(firstRefresh.promise)
      .mockResolvedValueOnce(listView([fresh]));

    const initialRefresh = useAgentRunListStateStore
      .getState()
      .refreshProject("project-1", "initial");
    await vi.waitFor(() => {
      expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
    });

    const invalidationRefresh = invalidateAgentRunListStateForProjectEvent(
      agentRunListInvalidated("project-1"),
      "project-1",
    );
    firstRefresh.reject(new Error("network down"));
    await Promise.all([initialRefresh, invalidationRefresh]);

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
    expect(useAgentRunListStateStore.getState().byProjectId["project-1"]?.status).toBe("error");

    await invalidateAgentRunListStateForProjectEvent(
      agentRunListInvalidated("project-1"),
      "project-1",
    );

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(2);
    expect(useAgentRunListStateStore.getState().byProjectId["project-1"]?.status).toBe("ready");
    expect(
      useAgentRunListStateStore.getState().byProjectId["project-1"]?.entries[0]?.run_ref.run_id,
    ).toBe("run-fresh");
  });

  it("忽略非 AgentRunList projection invalidation", async () => {
    const entry = agentRunEntry("run-1", "agent-1", "当前项目", "2026-06-25T01:00:00Z");
    mockedFetchProjectAgentRuns.mockResolvedValueOnce(listView([entry]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListStateForProjectEvent(mailboxInvalidated("project-1"), "project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
  });

  it("忽略其他 Project 的事件", async () => {
    const entry = agentRunEntry("run-1", "agent-1", "当前项目", "2026-06-25T01:00:00Z");
    mockedFetchProjectAgentRuns.mockResolvedValueOnce(listView([entry]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListStateForProjectEvent(projectStateChanged("project-2"), "project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
  });

  it("加载更多只追加后端下一页顺序，不重排已加载窗口", async () => {
    const first = agentRunEntry("run-first", "agent-first", "第一页", "2026-06-25T02:00:00Z");
    const second = agentRunEntry("run-second", "agent-second", "第二页", "2026-06-25T01:00:00Z");
    mockedFetchProjectAgentRuns
      .mockResolvedValueOnce(listView([first], "cursor-1"))
      .mockResolvedValueOnce(listView([second]));

    await useAgentRunListStateStore.getState().ensureFirstPage("project-1");
    await useAgentRunListStateStore.getState().loadMore("project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenLastCalledWith("project-1", {
      limit: 30,
      cursor: "cursor-1",
    });
    expect(
      useAgentRunListStateStore
        .getState()
        .byProjectId["project-1"]
        ?.entries
        .map((entry) => entry.run_ref.run_id),
    ).toEqual(["run-first", "run-second"]);
  });

  it("shortcut 与完整列表不注册固定周期 poller", () => {
    const shortcutSource = readFileSync(
      new URL("../../components/layout/AgentRunShortcutList.tsx", import.meta.url),
      "utf8",
    );
    const activeListSource = readFileSync(
      new URL("./active-agent-run-list.tsx", import.meta.url),
      "utf8",
    );

    expect(shortcutSource).not.toContain("setInterval");
    expect(activeListSource).not.toContain("setInterval");
  });

  it("ActiveAgentRunList 主行删除入口使用确认、刷新列表状态和安全导航", () => {
    const source = readFileSync(new URL("./active-agent-run-list.tsx", import.meta.url), "utf8");

    expect(source).toContain("CardMenu");
    expect(source).toContain("ConfirmDialog");
    expect(source).toContain("tone=\"danger\"");
    expect(source).toContain("deleteAgentRun(projectId, deleteTarget.runId)");
    expect(source).toContain("refreshProjectAgentRuns(projectId, \"agent_run_deleted\")");
    expect(source).toContain("useMatch(\"/agent-runs/:runId/:agentId\")");
    expect(source).toContain("navigate(\"/dashboard/agent\")");
    expect(source).toContain("onRequestDelete: (entry: AgentRunWorkspaceListEntry) => void");
    expect(source).toContain("event.target !== event.currentTarget");
    expect(source).not.toContain("expectedValue={deleteTarget");
    expect(source).not.toContain("onInputValueChange");
    expect(source).not.toContain("onRequestDelete: (child: AgentRunListChild) => void");
  });
});
