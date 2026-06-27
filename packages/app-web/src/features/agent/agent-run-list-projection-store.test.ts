/// <reference types="node" />

import { readFileSync } from "node:fs";
import { describe, expect, it, beforeEach, vi } from "vitest";

import type { ProjectEventStreamEnvelope } from "../../generated/project-contracts";
import { fetchProjectAgentRuns } from "../../services/lifecycle";
import type { AgentRunWorkspaceListEntry, AgentRunWorkspaceListView } from "../../types";
import {
  invalidateAgentRunListProjectionForProjectEvent,
  useAgentRunListProjectionStore,
} from "./agent-run-list-projection-store";

vi.mock("../../services/lifecycle", () => ({
  fetchProjectAgentRuns: vi.fn(),
}));

const mockedFetchProjectAgentRuns = vi.mocked(fetchProjectAgentRuns);

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
      workspace_status: "ready",
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

describe("agent-run list projection store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAgentRunListProjectionStore.setState({ byProjectId: {} });
  });

  it("保持后端列表顺序，不按 shell.last_activity_at 二次重排", async () => {
    const older = agentRunEntry("run-old", "agent-old", "后端第一条", "2026-06-25T01:00:00Z");
    const newer = agentRunEntry("run-new", "agent-new", "后端第二条", "2026-06-25T02:00:00Z");
    mockedFetchProjectAgentRuns.mockResolvedValueOnce(listView([older, newer]));

    await useAgentRunListProjectionStore.getState().ensureFirstPage("project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledWith("project-1", { limit: 30 });
    expect(
      useAgentRunListProjectionStore
        .getState()
        .byProjectId["project-1"]
        ?.entries
        .map((entry) => entry.run_ref.run_id),
    ).toEqual(["run-old", "run-new"]);
  });

  it("Project 事件触发同一 Project 的 list projection refresh", async () => {
    const before = agentRunEntry("run-1", "agent-1", "刷新前", "2026-06-25T01:00:00Z");
    const after = agentRunEntry("run-2", "agent-2", "刷新后", "2026-06-25T02:00:00Z");
    mockedFetchProjectAgentRuns
      .mockResolvedValueOnce(listView([before]))
      .mockResolvedValueOnce(listView([after]));

    await useAgentRunListProjectionStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListProjectionForProjectEvent(projectStateChanged("project-1"), "project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(2);
    expect(
      useAgentRunListProjectionStore.getState().byProjectId["project-1"]?.entries[0]?.run_ref.run_id,
    ).toBe("run-2");
  });

  it("忽略其他 Project 的事件", async () => {
    const entry = agentRunEntry("run-1", "agent-1", "当前项目", "2026-06-25T01:00:00Z");
    mockedFetchProjectAgentRuns.mockResolvedValueOnce(listView([entry]));

    await useAgentRunListProjectionStore.getState().ensureFirstPage("project-1");
    await invalidateAgentRunListProjectionForProjectEvent(projectStateChanged("project-2"), "project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenCalledTimes(1);
  });

  it("加载更多只追加后端下一页顺序，不重排已加载窗口", async () => {
    const first = agentRunEntry("run-first", "agent-first", "第一页", "2026-06-25T02:00:00Z");
    const second = agentRunEntry("run-second", "agent-second", "第二页", "2026-06-25T01:00:00Z");
    mockedFetchProjectAgentRuns
      .mockResolvedValueOnce(listView([first], "cursor-1"))
      .mockResolvedValueOnce(listView([second]));

    await useAgentRunListProjectionStore.getState().ensureFirstPage("project-1");
    await useAgentRunListProjectionStore.getState().loadMore("project-1");

    expect(mockedFetchProjectAgentRuns).toHaveBeenLastCalledWith("project-1", {
      limit: 30,
      cursor: "cursor-1",
    });
    expect(
      useAgentRunListProjectionStore
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
});
