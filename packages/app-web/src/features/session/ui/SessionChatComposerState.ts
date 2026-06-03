export interface SessionComposerActionState {
  hasDispatcher: boolean;
  hasSession: boolean;
  isActionRunning: boolean;
  isCancelling: boolean;
  isSending: boolean;
  inputValue: string;
}

export function isSessionComposerSendDisabled(state: SessionComposerActionState): boolean {
  return state.isSending ||
    state.isCancelling ||
    (!state.hasDispatcher && !(state.hasSession && state.isActionRunning)) ||
    (
      state.hasSession && state.isActionRunning
        ? false
        : state.hasDispatcher ? false : !state.inputValue.trim()
    );
}
