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

/**
 * 读取可为空串的字符串字段：
 * - 字段值为 string（含空串）→ 原样返回
 * - 缺失 / null / undefined / 非字符串 → 返回空串
 *
 * 与 `requireStringField` 对应：当后端契约允许该字段留空（例如 description、notes
 * 这类可选说明字段），前端不应因空串而拒绝整条记录的映射。
 */
export function optStringField(
  raw: Record<string, unknown>,
  field: string,
): string {
  const value = raw[field];
  return typeof value === "string" ? value : "";
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
