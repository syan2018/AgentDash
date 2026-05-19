import type { OriginBadgeTone } from "@agentdash/ui";

/**
 * Asset 面板共享的 origin → OriginBadge tone/label 映射。
 *
 * 标准对齐 `/dev/design-system` 中 ORIGIN_PREVIEW，三个资产 panel
 * （Skill / Mcp / Workflow / 未来可扩展）必须走同一套映射，不要各写各的。
 */

export interface OriginBadgeMeta {
  label: string;
  tone: OriginBadgeTone;
}

const ORIGIN_TONE_MAP: Record<string, OriginBadgeMeta> = {
  builtin_seed: { label: "builtin", tone: "neutral" },
  builtin: { label: "builtin", tone: "neutral" },
  user: { label: "user", tone: "accent" },
  user_authored: { label: "user", tone: "accent" },
  github: { label: "github", tone: "info" },
  clawhub: { label: "clawhub", tone: "success" },
  skills_sh: { label: "skills.sh", tone: "warning" },
  cloned: { label: "cloned", tone: "info" },
  marketplace: { label: "marketplace", tone: "success" },
};

const FALLBACK: OriginBadgeMeta = { label: "user", tone: "accent" };

/**
 * 解析资产来源 → OriginBadge meta。
 *
 * - `installed === true` 一律视为 marketplace 安装来源
 * - 否则按 source 字符串映射；未识别值回退到 "user / accent"（保守但可视）
 */
export function resolveOriginBadge(source: string, installed: boolean): OriginBadgeMeta {
  if (installed) return ORIGIN_TONE_MAP.marketplace;
  return ORIGIN_TONE_MAP[source] ?? { ...FALLBACK, label: source || FALLBACK.label };
}
