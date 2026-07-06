/**
 * 从 commandExecution / shellExec item 的 aggregatedOutput 中解析终端元数据。
 *
 * 新终端协议下，aggregatedOutput 包含结构化的 key-value 文本：
 *   command: <cmd>
 *   cwd: <path>
 *   state: running
 *   terminal_id: term-xxx
 *   next_seq: 0
 *
 * 终端 read 操作格式：
 *   operation: read
 *   terminal_id: term-xxx
 *   cwd: <path>
 *   state: completed
 *   exit_code: 0
 *   next_seq: N
 *   <blank line>
 *   <actual output...>
 */

export interface TerminalItemMeta {
  terminalId: string | null;
  operation: string | null;
  /** 元数据行之后的实际输出内容（仅 read 操作可能携带） */
  outputContent: string | null;
}

const TERMINAL_ID_RE = /^terminal_id:\s*(.+)$/m;
const OPERATION_RE = /^operation:\s*(.+)$/m;

export function parseTerminalItemMeta(aggregatedOutput: string | null | undefined): TerminalItemMeta {
  if (!aggregatedOutput) {
    return { terminalId: null, operation: null, outputContent: null };
  }

  const terminalIdMatch = aggregatedOutput.match(TERMINAL_ID_RE);
  const operationMatch = aggregatedOutput.match(OPERATION_RE);

  let outputContent: string | null = null;
  if (operationMatch) {
    const blankLineIdx = aggregatedOutput.indexOf("\n\n");
    if (blankLineIdx !== -1) {
      const afterBlank = aggregatedOutput.slice(blankLineIdx + 2);
      if (afterBlank.length > 0) {
        outputContent = afterBlank;
      }
    }
  }

  return {
    terminalId: terminalIdMatch?.[1]?.trim() ?? null,
    operation: operationMatch?.[1]?.trim() ?? null,
    outputContent,
  };
}

export function isTerminalReadOperation(aggregatedOutput: string | null | undefined): boolean {
  if (!aggregatedOutput) return false;
  return OPERATION_RE.test(aggregatedOutput);
}

export function extractTerminalIdFromItem(item: {
  aggregatedOutput?: string | null;
  processId?: string | null;
}): string | null {
  if (item.processId && item.processId.startsWith("term-")) {
    return item.processId;
  }
  const meta = parseTerminalItemMeta(item.aggregatedOutput);
  return meta.terminalId;
}
