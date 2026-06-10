/**
 * Session Capabilities 交互卡片
 *
 * 解析 agentdash://session-capabilities/* 资源块，
 * 渲染 skills 的快捷预览面板。
 */

import { useMemo, useState } from "react";
import type { ContentBlock } from "../model/types";
import {
  isDefaultExposedSkill,
  isModelInvocationVisibleSkill,
  skillDisplayLabel,
  skillIdentityKey,
} from "../../../types/context";
import type {
  SessionBaselineCapabilities,
  SkillCapabilityEntry,
  SkillEntry,
  SkillProviderCluster,
} from "../../../types/context";

const CAPABILITY_URI_PREFIX = "agentdash://session-capabilities/";

function parseCapabilitiesBlock(block: ContentBlock): SessionBaselineCapabilities | null {
  if (block.type !== "resource") return null;
  const { uri } = block.resource;
  if (!uri.startsWith(CAPABILITY_URI_PREFIX)) return null;

  const text =
    "text" in block.resource && typeof block.resource.text === "string"
      ? block.resource.text
      : "";
  if (!text) return null;

  try {
    return normalizeCapabilities(JSON.parse(text));
  } catch {
    return null;
  }
}

function normalizeCapabilities(value: unknown): SessionBaselineCapabilities | null {
  if (!isRecord(value)) return null;
  return {
    skills: Array.isArray(value.skills) ? (value.skills as SkillEntry[]) : [],
    skill_clusters: Array.isArray(value.skill_clusters)
      ? (value.skill_clusters as SkillProviderCluster[])
      : [],
    skill_diagnostics: Array.isArray(value.skill_diagnostics)
      ? (value.skill_diagnostics as SessionBaselineCapabilities["skill_diagnostics"])
      : [],
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

// eslint-disable-next-line react-refresh/only-export-components
export function isSessionCapabilitiesBlock(block: ContentBlock): boolean {
  if (block.type !== "resource") return false;
  return block.resource?.uri?.startsWith(CAPABILITY_URI_PREFIX) ?? false;
}

export interface SessionCapabilityCardProps {
  block: ContentBlock;
  defaultExpanded?: boolean;
}

export function SessionCapabilityCard({ block, defaultExpanded = false }: SessionCapabilityCardProps) {
  const caps = useMemo(() => parseCapabilitiesBlock(block), [block]);
  const [expanded, setExpanded] = useState(defaultExpanded);

  if (!caps) return null;

  const clusters = visibleClusters(caps);
  const usesClusters = clusters.length > 0;
  const visibleSkills = usesClusters ? [] : caps.skills.filter(isModelInvocationVisibleSkill);
  const skillCount = usesClusters
    ? clusters.reduce((total, cluster) => total + defaultExposedSkills(cluster).length, 0)
    : visibleSkills.length;

  if (!usesClusters && skillCount === 0) return null;

  const summaryParts: string[] = [];
  if (usesClusters) summaryParts.push(`${clusters.length} 个 Provider`);
  if (skillCount > 0) summaryParts.push(`${skillCount} 个默认暴露 Skill`);

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors cursor-pointer hover:bg-secondary/35"
      >
        <span className="inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] border-info/25 bg-info/10 text-info">
          CAP
        </span>
        <span className="min-w-0 flex-1 truncate text-sm text-foreground/80">
          Session Capabilities
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/50">
          {summaryParts.join(" · ")}
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2.5 space-y-3">
          {usesClusters ? (
            <SkillClustersSection clusters={clusters} />
          ) : (
            <SkillsSection skills={visibleSkills} title="默认暴露 Skills" />
          )}
        </div>
      )}
    </div>
  );
}

function SkillClustersSection({ clusters }: { clusters: SkillProviderCluster[] }) {
  return (
    <div className="space-y-2">
      {clusters.map((cluster) => (
        <SkillClusterBlock key={cluster.provider_key} cluster={cluster} />
      ))}
    </div>
  );
}

function SkillClusterBlock({ cluster }: { cluster: SkillProviderCluster }) {
  const skills = defaultExposedSkills(cluster);
  const summary = cluster.ui_summary ?? cluster.model_summary ?? "";
  return (
    <section className="space-y-2 rounded-[8px] border border-border/70 bg-secondary/20 px-2.5 py-2">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground">
            {cluster.display_name || cluster.provider_key}
          </p>
          {summary && (
            <p className="mt-0.5 text-[11px] leading-5 text-muted-foreground">
              {summary}
            </p>
          )}
        </div>
        {cluster.inventory_count != null && (
          <span className="shrink-0 rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
            inventory {cluster.inventory_count}
          </span>
        )}
      </div>
      {cluster.inventory_hint && (
        <p className="rounded-[6px] border border-border/70 bg-background px-2 py-1.5 text-[11px] leading-5 text-muted-foreground">
          {cluster.inventory_hint}
        </p>
      )}
      {skills.length > 0 ? (
        <SkillsSection skills={skills} title="默认暴露 Skills" />
      ) : (
        <p className="text-[11px] text-muted-foreground/70">当前没有默认暴露 Skill。</p>
      )}
    </section>
  );
}

function SkillsSection({
  skills,
  title,
}: {
  skills: Array<SkillEntry | SkillCapabilityEntry>;
  title: string;
}) {
  const [showAll, setShowAll] = useState(false);
  const INITIAL_SHOW = 5;
  const displayed = showAll ? skills : skills.slice(0, INITIAL_SHOW);
  const hasMore = skills.length > INITIAL_SHOW;

  return (
    <div>
      <p className="mb-1.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">
        {title}
      </p>
      <div className="space-y-1">
        {displayed.map((skill) => (
          <SkillRow key={skillIdentityKey(skill)} skill={skill} />
        ))}
      </div>
      {hasMore && !showAll && (
        <button
          type="button"
          onClick={() => setShowAll(true)}
          className="mt-1.5 text-[11px] text-primary hover:underline"
        >
          展示全部 {skills.length} 个 Skills…
        </button>
      )}
    </div>
  );
}

function SkillRow({ skill }: { skill: SkillEntry | SkillCapabilityEntry }) {
  const [showPath, setShowPath] = useState(false);
  const displayLabel = skillDisplayLabel(skill);
  const identity = skillIdentityKey(skill);

  return (
    <div
      className="rounded-[6px] border border-border/70 bg-secondary/25 px-2.5 py-1.5 cursor-pointer hover:bg-secondary/50 transition-colors"
      onClick={() => setShowPath((v) => !v)}
    >
      <div className="flex items-start gap-2">
        <span className="shrink-0 text-xs font-medium text-foreground">{displayLabel}</span>
        <span className="flex-1 truncate text-[11px] text-muted-foreground">
          {skill.description.length > 80
            ? `${skill.description.slice(0, 80)}…`
            : skill.description}
        </span>
      </div>
      {(skill.provider_key || identity !== displayLabel) && (
        <div className="mt-1 flex flex-wrap gap-1">
          {skill.provider_key && <SkillChip label={skill.provider_key} />}
          {identity !== displayLabel && <SkillChip label={identity} />}
        </div>
      )}
      {showPath && (
        <p className="mt-1 font-mono text-[10px] text-muted-foreground/60 break-all">
          {skill.file_path}
        </p>
      )}
    </div>
  );
}

function SkillChip({ label }: { label: string }) {
  return (
    <span className="rounded-[4px] border border-border bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground/70">
      {label}
    </span>
  );
}

function visibleClusters(capabilities: SessionBaselineCapabilities): SkillProviderCluster[] {
  return (capabilities.skill_clusters ?? []).filter((cluster) => (
    Boolean(cluster.ui_summary)
    || Boolean(cluster.model_summary)
    || Boolean(cluster.inventory_hint)
    || cluster.inventory_count != null
    || defaultExposedSkills(cluster).length > 0
  ));
}

function defaultExposedSkills(cluster: SkillProviderCluster): SkillCapabilityEntry[] {
  return (cluster.default_exposed_skills ?? []).filter(isDefaultExposedSkill);
}

export default SessionCapabilityCard;
