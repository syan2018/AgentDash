import { describe, expect, it } from "vitest";

import type {
  AgentRunOwnershipView,
  ConversationCommandPlacement,
  ConversationCommandView,
  ConversationMailboxSnapshotView,
  ConversationModelConfigView,
} from "../../../generated/workflow-contracts";
import type {
  ConversationCommandKind,
  ConversationCommandStaleGuardView,
  MailboxMessageView,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ProjectAgentSummary } from "../../../types";
import {
  buildDraftSessionCommandState,
  buildRuntimeSessionCommandState,
  projectSessionChatCommandState,
  projectSessionChatMailboxModel,
} from "./conversationCommandState";

const ownership: AgentRunOwnershipView = {
  run_created_by_user_id: "owner-user",
  agent_created_by_user_id: "owner-user",
  current_user_controls_run: true,
};

function staleGuard(commandId: string): ConversationCommandStaleGuardView {
  return {
    snapshot_id: "snapshot-1",
    run_id: "run-1",
    agent_id: "agent-1",
    runtime_session_id: "session-1",
    active_turn_id: commandId === "cancel" ? "turn-1" : undefined,
  };
}

function command(input: {
  kind: ConversationCommandKind;
  command_id: string;
  enabled?: boolean;
  unavailable_reason?: string;
  disabled_code?: string;
  shortcut?: string;
  requires_input?: boolean;
  executor_config_policy?: string;
  placement?: ConversationCommandPlacement[];
}): ConversationCommandView {
  return {
    kind: input.kind,
    command_id: input.command_id,
    enabled: input.enabled ?? true,
    unavailable_reason: input.unavailable_reason,
    disabled_code: input.disabled_code,
    shortcut: input.shortcut,
    requires_input: input.requires_input ?? input.kind === "submit_message",
    executor_config_policy: input.executor_config_policy ?? "optional",
    placement: input.placement ?? ["composer_primary"],
    stale_guard: staleGuard(input.kind),
  };
}

function resolvedModelConfig(): ConversationModelConfigView {
  return {
    status: "resolved",
    missing_fields: [],
    effective_executor_config: {
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      source: "project_agent_preset",
    },
  };
}

function mailboxMessage(): MailboxMessageView {
  return {
    id: "mailbox-1",
    origin: "user",
    source: {
      namespace: "core",
      kind: "composer",
      actor: "user",
      display_label_key: "mailbox.source.core.composer",
    },
    delivery: { kind: "launch_or_continue_turn" },
    barrier: "agent_run_turn_boundary",
    drain_mode: "one",
    status: "queued",
    preview: "queued message",
    has_images: false,
    attempt_count: 0,
    created_at: "2026-06-30T00:00:00.000Z",
    updated_at: "2026-06-30T00:00:00.000Z",
    can_promote: true,
    can_delete: true,
    can_reorder: true,
    can_recall: true,
  };
}

describe("AgentRun conversation command state", () => {
  it("projects runtime keyboard, primary command, cancel command, and helper text", () => {
    const submit = command({
      kind: "submit_message",
      command_id: "cmd-submit",
      shortcut: "enter",
      placement: ["composer_primary"],
    });
    const cancel = command({
      kind: "cancel",
      command_id: "cmd-cancel",
      enabled: false,
      unavailable_reason: "当前没有运行中的 turn。",
      disabled_code: "not_running",
      requires_input: false,
      executor_config_policy: "forbidden",
      placement: ["header"],
    });
    const commandState = buildRuntimeSessionCommandState({
      conversation: {
        execution: {
          status: "running_active",
          reason: "正在运行",
        },
        commands: {
          ownership,
          keyboard: {
            enter: "cmd-submit",
            ctrl_enter: "cmd-submit-steer",
          },
          commands: [cancel, submit],
        },
        model_config: resolvedModelConfig(),
      },
      projectionStatus: "ready",
      projectionError: null,
    });

    const model = projectSessionChatCommandState(commandState);

    expect(model.mode).toBe("runtime");
    expect(model.executionStatus).toBe("running_active");
    expect(model.keyboard).toEqual({
      enter: "cmd-submit",
      ctrl_enter: "cmd-submit-steer",
    });
    expect(model.primaryCommandId).toBe("cmd-submit");
    expect(model.cancelCommand).toEqual({
      command_id: "cmd-cancel",
      kind: "cancel",
      enabled: false,
      unavailable_reason: "当前没有运行中的 turn。",
      disabled_code: "not_running",
      requires_input: false,
      executor_config_policy: "forbidden",
      shortcut: undefined,
    });
    expect(model.modelConfig.status).toBe("resolved");
    expect(model.helperText).toBe("正在运行");
  });

  it("keeps projection loading and error states visible when conversation snapshot is missing", () => {
    const commandState = buildRuntimeSessionCommandState({
      conversation: null,
      projectionStatus: "error",
      projectionError: "工作台投影加载失败",
    });

    const model = projectSessionChatCommandState(commandState);

    expect(model.executionStatus).toBe("error");
    expect(model.commands).toEqual([]);
    expect(model.modelConfig).toEqual({
      status: "model_required",
      missing_fields: [],
      message: "工作台投影加载失败",
    });
    expect(model.helperText).toBe("工作台投影加载失败");
  });

  it("uses draft model policy as the local draft command authority", () => {
    const agent: ProjectAgentSummary = {
      key: "agent-key",
      display_name: "Draft Agent",
      description: "Draft agent",
      source: "project_agent",
      executor: {
        executor: "CODEX",
        provider_id: null,
        model_id: null,
      },
    };

    const missingModel = buildDraftSessionCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      projectionReady: true,
    });
    expect(missingModel.executionStatus).toBe("model_required");
    expect(missingModel.localDraftAction?.enabled).toBe(false);
    expect(missingModel.localDraftAction?.disabled_code).toBe("model_required");

    const ready = buildDraftSessionCommandState({
      projectId: "project-1",
      agentKey: "agent-key",
      agent,
      projectionReady: true,
      explicitExecutorConfigOverride: {
        executor: "CODEX",
        provider_id: "openai",
        model_id: "gpt-test",
      },
    });
    const model = projectSessionChatCommandState(ready);

    expect(ready.executionStatus).toBe("draft");
    expect(ready.localDraftAction?.enabled).toBe(true);
    expect(model.mode).toBe("draft");
    expect(model.keyboard.enter).toBe("draft:start_local:resolved");
    expect(model.primaryCommandId).toBe("draft:start_local:resolved");
    expect(model.modelConfig.effective_executor_config).toEqual({
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      agent_id: undefined,
      thinking_level: undefined,
      permission_policy: undefined,
      source: "user_override",
    });
  });

  it("projects mailbox actions from mailbox snapshot and mailbox-row commands", () => {
    const resume = command({
      kind: "resume_mailbox",
      command_id: "cmd-resume",
      requires_input: false,
      placement: ["mailbox_banner"],
    });
    const promote = command({
      kind: "promote_mailbox_message",
      command_id: "cmd-promote",
      requires_input: false,
      placement: ["mailbox_row"],
    });
    const deleteCommand = command({
      kind: "delete_mailbox_message",
      command_id: "cmd-delete",
      requires_input: false,
      placement: ["mailbox_row"],
    });
    const commandState = buildRuntimeSessionCommandState({
      conversation: {
        execution: { status: "ready" },
        commands: {
          ownership,
          keyboard: {},
          commands: [promote, deleteCommand],
        },
        model_config: resolvedModelConfig(),
      },
      projectionStatus: "ready",
      projectionError: null,
    });
    const mailbox: ConversationMailboxSnapshotView = {
      visible_message_count: 1,
      paused: false,
      user_attention: true,
      resume_command: resume,
      state: {
        paused: true,
        can_resume: true,
        hide_system_steer_messages: true,
      },
      messages: [mailboxMessage()],
    };

    const model = projectSessionChatMailboxModel(commandState, mailbox);

    expect(model.messages).toEqual([mailboxMessage()]);
    expect(model.paused).toBe(true);
    expect(model.user_attention).toBe(true);
    expect(model.hide_system_steer_messages).toBe(true);
    expect(model.can_resume).toBe(true);
    expect(model.resumeAction?.command_id).toBe("cmd-resume");
    expect(model.promoteAction?.command_id).toBe("cmd-promote");
    expect(model.deleteAction?.command_id).toBe("cmd-delete");
  });
});
