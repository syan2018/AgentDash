const DEFAULT_SUMMARY_CHARS = 220;
const STRUCTURED_DETAIL_LINE_PATTERN = /^[A-Za-z_][\w.-]{0,80}=.{0,240}$/;
const HTML_START_PATTERN = /<(?:!doctype\s+html|html|head|body|style|script|svg|div|span|p|meta)\b/i;
const HTTP_STATUS_SUMMARY_PATTERN =
  /(?:Codex API|OpenAI API|Provider|模型服务|LLM[^\s:<]*)[^:<\n]{0,80}(?:返回|responded|returned)[^:<\n]{0,80}\b\d{3}\b[^:<\n]{0,80}/i;

export interface DiagnosticTextProjection {
  summary: string;
  overflowText: string | null;
}

export interface DiagnosticDetailsProjection {
  structuredLines: string[];
  overflowText: string | null;
}

export function projectDiagnosticText(
  rawText: string,
  maxSummaryChars = DEFAULT_SUMMARY_CHARS,
): DiagnosticTextProjection {
  const text = rawText.trim();
  if (text.length === 0) {
    return { summary: "", overflowText: null };
  }

  const htmlIndex = findHtmlStartIndex(text);
  if (htmlIndex >= 0) {
    const statusSummary = text.match(HTTP_STATUS_SUMMARY_PATTERN)?.[0]?.trim();
    const prefix = text.slice(0, htmlIndex).replace(/[:\s]+$/, "").trim();
    const htmlSummary = prefix.endsWith("=") ? `${prefix}HTML 错误响应` : prefix;
    const summarySource = statusSummary ?? htmlSummary;
    return {
      summary: boundPlainSummary(summarySource || "Provider 返回 HTML 错误响应", maxSummaryChars),
      overflowText: text,
    };
  }

  if (text.length > maxSummaryChars) {
    return {
      summary: boundPlainSummary(text, maxSummaryChars),
      overflowText: text,
    };
  }

  return { summary: text, overflowText: null };
}

export function projectDiagnosticDetails(rawDetails: string): DiagnosticDetailsProjection {
  const lines = rawDetails
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  const structuredLines: string[] = [];
  const overflowLines: string[] = [];

  for (const line of lines) {
    if (STRUCTURED_DETAIL_LINE_PATTERN.test(line)) {
      structuredLines.push(line);
    } else {
      overflowLines.push(line);
    }
  }

  return {
    structuredLines,
    overflowText: overflowLines.length > 0 ? overflowLines.join("\n") : null,
  };
}

function findHtmlStartIndex(text: string): number {
  const match = HTML_START_PATTERN.exec(text);
  return match?.index ?? -1;
}

function boundPlainSummary(text: string, maxChars: number): string {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (normalized.length <= maxChars) return normalized;
  return `${normalized.slice(0, maxChars).trimEnd()}...`;
}
