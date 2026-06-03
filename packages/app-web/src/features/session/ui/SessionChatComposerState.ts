export interface SessionComposerActionState {
  primaryActionEnabled: boolean;
  requirePromptText: boolean;
  isCancelling: boolean;
  isSending: boolean;
  inputValue: string;
}

export function isSessionComposerPrimaryDisabled(state: SessionComposerActionState): boolean {
  return state.isSending ||
    state.isCancelling ||
    !state.primaryActionEnabled ||
    (state.requirePromptText && !state.inputValue.trim());
}
