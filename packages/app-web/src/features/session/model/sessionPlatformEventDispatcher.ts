import type { PlatformEvent } from "../../../generated/backbone-protocol";
import type { TerminalProcessState } from "../../../types/terminal";
import type { SessionEventEnvelope } from "./types";
import { useTerminalStore } from "./useTerminalStore";

function isTerminalProcessState(value: string): value is TerminalProcessState {
  return value === "starting" ||
    value === "running" ||
    value === "exited" ||
    value === "lost" ||
    value === "killed";
}

export function dispatchSessionPlatformEvent(event: SessionEventEnvelope, onError?: (error: Error) => void): boolean {
  const bbEvent = event.notification.event;
  if (bbEvent.type !== "platform") return false;

  const platform: PlatformEvent = bbEvent.payload;
  if (platform.kind === "terminal_output") {
    useTerminalStore
      .getState()
      .appendOutput(platform.data.terminal_id, platform.data.data);
    return true;
  }

  if (platform.kind === "terminal_state_changed") {
    if (!isTerminalProcessState(platform.data.state)) {
      onError?.(new Error(`非法终端状态: ${platform.data.state}`));
      return true;
    }
    useTerminalStore
      .getState()
      .updateTerminalState(
        platform.data.terminal_id,
        platform.data.state,
        platform.data.exit_code ?? undefined,
      );
    return true;
  }

  return false;
}
