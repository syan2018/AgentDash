/**
 * 共享的展示层格式化工具。
 *
 * 历史上 formatBytes / formatDateTime / formatRelativeTime 在多个 feature 组件里
 * 各自重复实现，行为略有漂移。此处收口为单一来源，调用方通过参数选择呈现风格，
 * 避免再次散落副本。
 */

/** 字节数 → 人类可读（B / KB / MB）。接受 number 或 bigint。 */
export function formatBytes(value: number | bigint): string {
  const numericValue = typeof value === "bigint" ? Number(value) : value;
  if (numericValue < 1024) return `${numericValue} B`;
  if (numericValue < 1024 * 1024) return `${(numericValue / 1024).toFixed(1)} KB`;
  return `${(numericValue / (1024 * 1024)).toFixed(1)} MB`;
}

/** ISO 时间串 → zh-CN 本地化 `MM-DD HH:mm`；非法输入原样返回。 */
export function formatDateTime(value: string): string {
  const time = new Date(value);
  if (Number.isNaN(time.getTime())) return value;
  return time.toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export interface RelativeTimeOptions {
  /** 输入为空（null/undefined）时的占位文案，默认 "—"。 */
  emptyLabel?: string;
  /**
   * 超出小时区间后的呈现风格：
   * - "verbose"（默认）："${days} 天前"
   * - "compact"：天用 "${days}d"（30 天内），超出回落到日期
   * - "datetime"：回落到 "M/D HH:MM"
   */
  longStyle?: "verbose" | "compact" | "datetime";
}

/**
 * 相对时间格式化，覆盖原先散落的三类输入与三种长区间呈现。
 *
 * @param input 毫秒/秒时间戳（< 1e12 视为秒）、ISO 字符串，或空值
 */
export function formatRelativeTime(
  input: number | string | null | undefined,
  options: RelativeTimeOptions = {},
): string {
  const { emptyLabel = "—", longStyle = "verbose" } = options;
  if (input == null) return emptyLabel;

  let ts: number;
  if (typeof input === "string") {
    ts = new Date(input).getTime();
    if (Number.isNaN(ts)) return emptyLabel;
  } else {
    ts = input < 1e12 ? input * 1000 : input;
  }

  const diffMs = Date.now() - ts;
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  const date = new Date(ts);

  if (longStyle === "compact") {
    if (minutes < 60) return `${minutes}m`;
    if (hours < 24) return `${hours}h`;
    if (days < 30) return `${days}d`;
    return `${date.getMonth() + 1}/${date.getDate()}`;
  }

  if (longStyle === "datetime") {
    if (minutes < 60) return `${minutes} 分钟前`;
    if (hours < 24) return `${hours} 小时前`;
    const hh = date.getHours().toString().padStart(2, "0");
    const mm = date.getMinutes().toString().padStart(2, "0");
    return `${date.getMonth() + 1}/${date.getDate()} ${hh}:${mm}`;
  }

  // verbose
  if (minutes < 60) return `${minutes} 分钟前`;
  if (hours < 24) return `${hours} 小时前`;
  return `${days} 天前`;
}
