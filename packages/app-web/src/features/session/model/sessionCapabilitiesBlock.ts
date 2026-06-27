import {
  isDefaultExposedSkill,
} from "../../../types/context";
import type {
  SessionBaselineCapabilities,
  SkillCapabilityEntry,
  SkillDiscoveryDiagnostic,
  SkillEntry,
  SkillProviderCluster,
} from "../../../types/context";
import type { ContentBlock } from "./types";

export const SESSION_CAPABILITIES_URI_PREFIX = "agentdash://session-capabilities/";

type ResourceContentBlock = Extract<ContentBlock, { type: "resource" }>;

export interface SessionCapabilitiesBlockViewModel {
  clusters: SkillProviderCluster[];
  skillCount: number;
  summaryParts: string[];
}

export function isSessionCapabilitiesBlock(block: ContentBlock): block is ResourceContentBlock {
  return (
    block.type === "resource"
    && block.resource.uri.startsWith(SESSION_CAPABILITIES_URI_PREFIX)
  );
}

export function parseCapabilitiesBlock(block: ContentBlock): SessionBaselineCapabilities | null {
  if (!isSessionCapabilitiesBlock(block)) return null;
  const text = "text" in block.resource ? block.resource.text : "";
  if (!text) return null;

  try {
    return normalizeCapabilities(JSON.parse(text));
  } catch {
    return null;
  }
}

export function buildSessionCapabilitiesBlockViewModel(
  block: ContentBlock,
): SessionCapabilitiesBlockViewModel | null {
  const capabilities = parseCapabilitiesBlock(block);
  if (!capabilities) return null;

  const clusters = getVisibleCapabilityClusters(capabilities);
  const skillCount = clusters.reduce((total, cluster) => total + getDefaultExposedSkills(cluster).length, 0);

  if (clusters.length === 0 && skillCount === 0) return null;

  const summaryParts: string[] = [];
  summaryParts.push(`${clusters.length} 个 Provider`);
  if (skillCount > 0) summaryParts.push(`${skillCount} 个默认暴露 Skill`);

  return {
    clusters,
    skillCount,
    summaryParts,
  };
}

export function getVisibleCapabilityClusters(
  capabilities: SessionBaselineCapabilities,
): SkillProviderCluster[] {
  return (capabilities.skill_clusters ?? []).filter((cluster) => (
    Boolean(cluster.ui_summary)
    || Boolean(cluster.model_summary)
    || Boolean(cluster.inventory_hint)
    || cluster.inventory_count != null
    || getDefaultExposedSkills(cluster).length > 0
  ));
}

export function getDefaultExposedSkills(
  cluster: SkillProviderCluster,
): SkillCapabilityEntry[] {
  return (cluster.default_exposed_skills ?? []).filter(isDefaultExposedSkill);
}

function normalizeCapabilities(value: unknown): SessionBaselineCapabilities | null {
  if (!isRecord(value)) return null;
  return {
    skills: readArray<SkillEntry>(value.skills),
    skill_clusters: readArray<SkillProviderCluster>(value.skill_clusters),
    skill_diagnostics: readArray<SkillDiscoveryDiagnostic>(value.skill_diagnostics),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function readArray<T>(value: unknown): T[] {
  if (!Array.isArray(value)) return [];
  return value.filter((_item): _item is T => true);
}
