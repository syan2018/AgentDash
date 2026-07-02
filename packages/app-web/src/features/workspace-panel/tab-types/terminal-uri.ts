const TERMINAL_URI_PREFIX = "terminal://";
const COMMAND_OUTPUT_REPLAY_URI_PREFIX = "terminal://output/";
const COMMAND_OUTPUT_REPLAY_ID_PREFIX = "command-output:";

export type ParsedTerminalUri =
  | { terminalId: string; mode: "terminal"; itemId?: string }
  | { terminalId: string; mode: "output"; itemId: string };

export function buildCommandOutputReplayTerminalId(itemId: string): string {
  return `${COMMAND_OUTPUT_REPLAY_ID_PREFIX}${itemId}`;
}

export function buildCommandOutputReplayTerminalUri(itemId: string): string {
  return `${COMMAND_OUTPUT_REPLAY_URI_PREFIX}${encodeURIComponent(itemId)}`;
}

export function buildInteractiveTerminalUri(terminalId: string): string {
  return `${TERMINAL_URI_PREFIX}${terminalId}`;
}

export function parseTerminalUri(uri: string): ParsedTerminalUri | null {
  if (uri.startsWith(COMMAND_OUTPUT_REPLAY_URI_PREFIX)) {
    const encoded = uri.slice(COMMAND_OUTPUT_REPLAY_URI_PREFIX.length);
    if (!encoded) return null;
    let itemId: string;
    try {
      itemId = decodeURIComponent(encoded);
    } catch {
      return null;
    }
    return {
      terminalId: buildCommandOutputReplayTerminalId(itemId),
      mode: "output",
      itemId,
    };
  }
  if (!uri.startsWith(TERMINAL_URI_PREFIX)) return null;
  const terminalId = uri.slice(TERMINAL_URI_PREFIX.length);
  return terminalId ? { terminalId, mode: "terminal" } : null;
}
