import type { SkillAssetDto } from "../../../types";

export interface SkillAssetCardPolicy {
  isBuiltin: boolean;
  detailKind: "edit" | "view";
  primaryLabel: "编辑" | "查看";
  canPublish: boolean;
  canDelete: boolean;
}

export function resolveSkillAssetCardPolicy(
  skill: Pick<SkillAssetDto, "source" | "installed_source">,
): SkillAssetCardPolicy {
  const isBuiltin = skill.source === "builtin_seed";
  return {
    isBuiltin,
    detailKind: isBuiltin ? "view" : "edit",
    primaryLabel: isBuiltin ? "查看" : "编辑",
    canPublish: !isBuiltin && !skill.installed_source,
    canDelete: !isBuiltin,
  };
}
