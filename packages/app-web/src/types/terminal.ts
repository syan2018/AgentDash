/**
 * Terminal instance type definitions.
 *
 * Corresponds to the runtime terminal state in AgentRunTerminalRegistry.
 */

export type TerminalProcessState =
  | "starting"
  | "running"
  | "exited"
  | "lost"
  | "killed";

export type TerminalCapability =
  | "interactive"
  | "read_only_output"
  | "state_only";

export interface TerminalInfo {
  id: string;
  /** AgentRun scope run_id (optional, may not be available for output-replay terminals) */
  runId?: string;
  /** AgentRun scope agent_id (optional, may not be available for output-replay terminals) */
  agentId?: string;
  capability: TerminalCapability;
  backendId?: string;
  mountRootRef?: string;
  cwd: string;
  shell?: string;
  processId?: number;
  state: TerminalProcessState;
  /** 与 process state 正交；断线只改变 availability，不推断进程已 Lost。 */
  availability?: "online" | "offline" | "reconciling";
  exitCode?: number;
  /** Associated tool call item ID (set on serial command promote) */
  linkedItemId?: string;
  createdAt: number;
  exitedAt?: number;
}

export interface TerminalSpawnRequest {
  cwd?: string;
  shell?: string;
  cols?: number;
  rows?: number;
}

export interface TerminalSpawnResult {
  terminal_id: string;
  process_id?: number;
}

export interface TerminalInputRequest {
  data: string;
}

export interface TerminalResizeRequest {
  cols: number;
  rows: number;
}
