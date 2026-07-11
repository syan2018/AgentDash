import { describe, expect, it } from "vitest";
import { isAgentRunWorkspaceActionRunning } from "./SessionChatViewModel";
import {
  isSessionComposerSubmitDisabled,
  isSessionModelRequirementSatisfied,
} from "./SessionChatComposerState";

describe("isSessionComposerSubmitDisabled", () => {
  it("command 不可用时即使有输入也不可提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: false,
      requirePromptText: true,
      inputValue: "hello",
      isCancelling: false,
      isSending: false,
    })).toBe(true);
  });

  it("command 可用但需要输入时空文本不可提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: true,
      requirePromptText: true,
      inputValue: "",
      isCancelling: false,
      isSending: false,
    })).toBe(true);
  });

  it("command 可用且有输入时允许提交", () => {
    expect(isSessionComposerSubmitDisabled({
      commandEnabled: true,
      requirePromptText: true,
      inputValue: "hello",
      isCancelling: false,
      isSending: false,
    })).toBe(false);
  });
});

describe("isAgentRunWorkspaceActionRunning", () => {
  it("uses AgentRun execution projection without requiring a runtime trace session id", () => {
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "running_active",
    })).toBe(true);
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "ready",
    })).toBe(false);
    expect(isAgentRunWorkspaceActionRunning({
      executionStatus: "cancelling",
    })).toBe(true);
  });
});

describe("isSessionModelRequirementSatisfied", () => {
  it("keeps model_required blocked without a complete explicit override", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
    })).toBe(false);
  });

  it("allows model_required to be satisfied by explicit provider and model selection", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "gpt-5.4-mini",
    })).toBe(true);
  });

  it("allows model_required when the selected model has reasoning even if thinking level is unset", () => {
    expect(isSessionModelRequirementSatisfied("model_required", {
      executor: "PI_AGENT",
      provider_id: "openai",
      model_id: "reasoning-model",
      thinking_level: undefined,
    })).toBe(true);
  });
});
