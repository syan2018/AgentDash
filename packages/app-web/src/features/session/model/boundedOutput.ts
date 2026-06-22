export interface BoundedOutputInfo {
  truncated: boolean;
  lifecyclePath?: string;
  policy?: string;
  omittedBytes?: number;
  originalBytes?: number;
  inlineBytes?: number;
  source: "tool_result" | "shell_output" | "terminal_output";
}

const TOOL_TRUNCATED_MARKER = "[tool result truncated]";
const TERMINAL_TRUNCATED_PATTERN = /\[terminal output truncated: omitted_bytes=(\d+)\]/;
const SHELL_TRUNCATED_PATTERN = /output_truncated:\s*true(?:\s*\(omitted_bytes=(\d+)\))?/;
const OMITTED_BYTES_PATTERN = /\[\.\.\. omitted (\d+) bytes \.\.\.\]/;

export function parseBoundedOutputText(text: string | null | undefined): BoundedOutputInfo | null {
  if (!text) return null;

  const toolTruncated = text.includes(TOOL_TRUNCATED_MARKER);
  const shellMatch = text.match(SHELL_TRUNCATED_PATTERN);
  const terminalMatch = text.match(TERMINAL_TRUNCATED_PATTERN);

  if (!toolTruncated && !shellMatch && !terminalMatch) {
    return null;
  }

  const source: BoundedOutputInfo["source"] = terminalMatch
    ? "terminal_output"
    : shellMatch
      ? "shell_output"
      : "tool_result";

  return {
    truncated: true,
    source,
    lifecyclePath: readLineValue(text, "lifecycle_path"),
    policy: readLineValue(text, "policy"),
    omittedBytes:
      readFirstNumber(terminalMatch?.[1], shellMatch?.[1]) ??
      readOmittedBytesFromText(text),
  };
}

export function parseTruncationDetails(value: unknown): BoundedOutputInfo | null {
  const root = asRecord(value);
  if (!root) return null;

  const details = asRecord(root.details) ?? root;
  const truncation = asRecord(details.truncation);
  if (!truncation || truncation.truncated !== true) {
    return null;
  }

  return {
    truncated: true,
    source: "tool_result",
    lifecyclePath: stringField(details.lifecycle_path),
    policy: stringField(truncation.policy),
    originalBytes: numberField(truncation.original_bytes),
    inlineBytes: numberField(truncation.inline_bytes),
    omittedBytes: numberField(truncation.omitted_bytes),
  };
}

export function mergeBoundedOutputInfo(
  primary: BoundedOutputInfo | null,
  fallback: BoundedOutputInfo | null,
): BoundedOutputInfo | null {
  if (!primary) return fallback;
  if (!fallback) return primary;
  return {
    ...fallback,
    ...primary,
    lifecyclePath: primary.lifecyclePath ?? fallback.lifecyclePath,
    policy: primary.policy ?? fallback.policy,
    omittedBytes: primary.omittedBytes ?? fallback.omittedBytes,
    originalBytes: primary.originalBytes ?? fallback.originalBytes,
    inlineBytes: primary.inlineBytes ?? fallback.inlineBytes,
  };
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "0 B";
  if (value < 1024) return `${value} B`;
  const kib = value / 1024;
  if (kib < 1024) return `${formatNumber(kib)} KiB`;
  return `${formatNumber(kib / 1024)} MiB`;
}

function readLineValue(text: string, key: string): string | undefined {
  const prefix = `${key}:`;
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith(prefix)) continue;
    const value = trimmed.slice(prefix.length).trim();
    return value.length > 0 ? value : undefined;
  }
  return undefined;
}

function readOmittedBytesFromText(text: string): number | undefined {
  const match = text.match(OMITTED_BYTES_PATTERN);
  return readFirstNumber(match?.[1]);
}

function readFirstNumber(...values: Array<string | undefined>): number | undefined {
  for (const value of values) {
    if (value == null) continue;
    const parsed = Number(value);
    if (Number.isFinite(parsed) && parsed >= 0) return parsed;
  }
  return undefined;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function stringField(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberField(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function formatNumber(value: number): string {
  return value >= 10 ? value.toFixed(0) : value.toFixed(1);
}
