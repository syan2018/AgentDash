/**
 * Diff 解析 / 合成 helper
 *
 * 抽出的纯函数模块，避免跟 DiffCardBody 组件文件混在一起触发
 * react-refresh/only-export-components 限制。
 */

export type DiffLine =
  | { kind: "context"; oldNo: number | null; newNo: number | null; text: string }
  | { kind: "add"; oldNo: null; newNo: number; text: string }
  | { kind: "remove"; oldNo: number; newNo: null; text: string }
  | { kind: "hunk"; text: string }
  | { kind: "meta"; text: string };

export interface DiffPayload {
  lines: DiffLine[];
  added: number;
  removed: number;
}

/**
 * 解析 unified diff 文本为 DiffLine[] 与统计。
 *
 * 支持常规 unified 格式 + hunk 头 (`@@ -a,b +c,d @@`)。
 * 行号若在 hunk 头声明则按 hunk 推进；否则从 1 起算。
 */
export function parseUnifiedDiff(diff: string): DiffPayload {
  if (!diff) return { lines: [], added: 0, removed: 0 };

  const lines: DiffLine[] = [];
  let added = 0;
  let removed = 0;
  let oldNo = 0;
  let newNo = 0;
  let inHunk = false;

  for (const raw of diff.split("\n")) {
    if (raw.startsWith("--- ") || raw.startsWith("+++ ")) {
      lines.push({ kind: "meta", text: raw });
      continue;
    }
    if (raw.startsWith("@@")) {
      const m = /@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s*@@/.exec(raw);
      if (m) {
        oldNo = Number(m[1]);
        newNo = Number(m[2]);
      }
      lines.push({ kind: "hunk", text: raw });
      inHunk = true;
      continue;
    }
    if (!inHunk && lines.length === 0 && raw.length === 0) continue;

    if (raw.startsWith("+")) {
      lines.push({ kind: "add", oldNo: null, newNo, text: raw.slice(1) });
      newNo++;
      added++;
      continue;
    }
    if (raw.startsWith("-")) {
      lines.push({ kind: "remove", oldNo, newNo: null, text: raw.slice(1) });
      oldNo++;
      removed++;
      continue;
    }
    const text = raw.startsWith(" ") ? raw.slice(1) : raw;
    lines.push({ kind: "context", oldNo, newNo, text });
    oldNo++;
    newNo++;
  }

  return { lines, added, removed };
}

/**
 * 把 old/new 文本对合成一个最简 unified diff（全部当一段替换）。
 *
 * 不做行级 LCS，简单列出 -old / +new。后续可升级为 myers diff。
 */
export function synthesizeFromOldNew(oldText: string, newText: string): DiffPayload {
  const oldLines = oldText.length === 0 ? [] : oldText.split("\n");
  const newLines = newText.length === 0 ? [] : newText.split("\n");
  const lines: DiffLine[] = [];

  oldLines.forEach((text, i) => {
    lines.push({ kind: "remove", oldNo: i + 1, newNo: null, text });
  });
  newLines.forEach((text, i) => {
    lines.push({ kind: "add", oldNo: null, newNo: i + 1, text });
  });

  return { lines, added: newLines.length, removed: oldLines.length };
}
