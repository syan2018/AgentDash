import type { ExecutorConfig } from "../../../services/executor";

export interface SessionComposerActionState {
  commandEnabled: boolean;
  requirePromptText: boolean;
  isCancelling: boolean;
  isSending: boolean;
  inputValue: string;
}

export function isSessionComposerSubmitDisabled(state: SessionComposerActionState): boolean {
  return state.isSending ||
    state.isCancelling ||
    !state.commandEnabled ||
    (state.requirePromptText && !state.inputValue.trim());
}

export function isSessionModelRequirementSatisfied(
  modelStatus: "resolved" | "model_required",
  executorConfig: ExecutorConfig | undefined,
): boolean {
  if (modelStatus !== "model_required") return true;
  return Boolean(
    executorConfig?.executor.trim() &&
    executorConfig.provider_id?.trim() &&
    executorConfig.model_id?.trim(),
  );
}
