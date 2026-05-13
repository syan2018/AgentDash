/**
 * 终端实例类型定义
 *
 * 对应后端 SessionTerminalCache 中的运行时终端状态。
 */

export type TerminalProcessState =
  | "starting"
  | "running"
  | "exited"
  | "lost"
  | "killed";

export interface TerminalInfo {
  id: string;
  sessionId: string;
  backendId?: string;
  mountRootRef?: string;
  cwd: string;
  shell?: string;
  processId?: number;
  state: TerminalProcessState;
  exitCode?: number;
  /** 关联的工具调用 item ID（串行命令 promote 时设置） */
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
