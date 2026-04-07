/**
 * 共享的 API Response 映射工具
 *
 * 将后端 JSON 响应安全地转换为前端类型，集中管理避免各 service 文件重复定义。
 */

export function asRecord(raw: unknown): Record<string, unknown> | null {
  return raw != null && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
}

export function asRecordArray(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw)
    ? raw.filter(
        (item): item is Record<string, unknown> =>
          item != null && typeof item === "object",
      )
    : [];
}

export function asStringArray(raw: unknown): string[] {
  return Array.isArray(raw)
    ? raw.filter((item): item is string => typeof item === "string")
    : [];
}

export function optString(raw: unknown): string | null {
  return raw != null ? String(raw) : null;
}

export function requireStringField(
  raw: Record<string, unknown>,
  field: string,
): string {
  const value = raw[field];
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`缺少或非法的字段 ${field}`);
  }
  return value;
}

export function requireNumberField(
  raw: Record<string, unknown>,
  field: string,
): number {
  const value = raw[field];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`缺少或非法的数字字段 ${field}`);
  }
  return value;
}
