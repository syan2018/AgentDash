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
