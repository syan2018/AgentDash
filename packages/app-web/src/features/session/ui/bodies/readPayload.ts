/**
 * Read 工具返回文本解析器
 *
 * 解析 `file: <path>` 标头和 `N | text` 行号前缀，
 * 返回干净的文件内容供 ReadCardBody 渲染。
 */

export interface ParsedReadOutput {
  filePath?: string;
  lines: Array<{ lineNo: number; text: string }>;
  bodyText: string;
  rawText: string;
  parsedLineNumbers: boolean;
}

const FILE_HEADER_RE = /^file:\s*(.+)$/;
const LINE_NO_RE = /^\s*(\d+)\s*\|\s?(.*)$/;

export function parseReadToolText(rawText: string, fallbackStartLine: number): ParsedReadOutput {
  const rawLines = rawText.split("\n");
  let filePath: string | undefined;
  let bodyStart = 0;

  // 1. 检测 file: 标头
  for (let i = 0; i < rawLines.length; i++) {
    const trimmed = rawLines[i].trim();
    if (trimmed.length === 0) continue;
    const m = FILE_HEADER_RE.exec(trimmed);
    if (m) {
      filePath = m[1].trim();
      bodyStart = i + 1;
    }
    break;
  }

  const bodyLines = rawLines.slice(bodyStart);

  // 2. 尝试匹配行号前缀
  let matchedCount = 0;
  let nonEmptyCount = 0;
  const parsed: Array<{ lineNo: number; text: string }> = [];

  for (const line of bodyLines) {
    if (line.trim().length === 0) {
      parsed.push({ lineNo: 0, text: "" });
      continue;
    }
    nonEmptyCount++;
    const m = LINE_NO_RE.exec(line);
    if (m) {
      matchedCount++;
      parsed.push({ lineNo: Number(m[1]), text: m[2] });
    } else {
      parsed.push({ lineNo: 0, text: line });
    }
  }

  // 3. 判断是否为行号格式：至少连续两行匹配，或大多数非空行匹配
  const isLineNumberFormat =
    nonEmptyCount > 0 && (matchedCount >= 2 || matchedCount / nonEmptyCount >= 0.5);

  if (isLineNumberFormat) {
    // 对于没匹配到行号的行（非空），保守保留原文
    let lastLineNo = 0;
    for (const p of parsed) {
      if (p.lineNo > 0) {
        lastLineNo = p.lineNo;
      } else if (p.text.length > 0) {
        lastLineNo++;
        p.lineNo = lastLineNo;
      }
    }
    // 空行也需要编号
    let prevLineNo = 0;
    for (const p of parsed) {
      if (p.lineNo > 0) {
        prevLineNo = p.lineNo;
      } else {
        prevLineNo++;
        p.lineNo = prevLineNo;
      }
    }

    const bodyText = parsed.map((p) => p.text).join("\n");
    return { filePath, lines: parsed, bodyText, rawText, parsedLineNumbers: true };
  }

  // 4. fallback：顺序编号
  const fallbackParsed = bodyLines.map((line, i) => ({
    lineNo: fallbackStartLine + i,
    text: line,
  }));
  const bodyText = bodyLines.join("\n");
  return { filePath, lines: fallbackParsed, bodyText, rawText, parsedLineNumbers: false };
}
