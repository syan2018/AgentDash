/**
 * Session Capabilities 交互卡片
 *
 * 解析 agentdash://session-capabilities/* 资源块，
 * 渲染 companion agents 与 skills 的快捷预览面板。
 */

import { useMemo, useState } from "react";
import type { ContentBlock } from "../model/types";
import type { SessionBaselineCapabilities, CompanionAgentEntry, SkillEntry } from "../../../types/context";

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
    return JSON.parse(text) as SessionBaselineCapabilities;
  } catch {
    return null;
  }
}

// eslint-disable-next-line react-refresh/only-export-components
export function isSessionCapabilitiesBlock(block: ContentBlock): boolean {
  if (block.type !== "resource") return false;
  return block.resource?.uri?.startsWith(CAPABILITY_URI_PREFIX) ?? false;
}

export interface AcpSessionCapabilityCardProps {
  block: ContentBlock;
}

export function AcpSessionCapabilityCard({ block }: AcpSessionCapabilityCardProps) {
  const caps = useMemo(() => parseCapabilitiesBlock(block), [block]);
  const [expanded, setExpanded] = useState(false);

  if (!caps) return null;

  const companionCount = caps.companion_agents.length;
  const visibleSkills = caps.skills.filter((s) => !s.disable_model_invocation);
  const skillCount = visibleSkills.length;

  if (companionCount === 0 && skillCount === 0) return null;

  const summaryParts: string[] = [];
  if (companionCount > 0) summaryParts.push(`${companionCount} 个关联 Agent`);
  if (skillCount > 0) summaryParts.push(`${skillCount} 个可用 Skill`);

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors cursor-pointer hover:bg-secondary/35"
      >
        <span className="inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] border-indigo-500/25 bg-indigo-500/8 text-indigo-600 dark:text-indigo-400">
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
          {companionCount > 0 && (
            <CompanionAgentsSection agents={caps.companion_agents} />
          )}
          {skillCount > 0 && (
            <SkillsSection skills={visibleSkills} />
          )}
        </div>
      )}
    </div>
  );
}

function CompanionAgentsSection({ agents }: { agents: CompanionAgentEntry[] }) {
  return (
    <div>
      <p className="mb-1.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">
        关联 Agents
      </p>
      <div className="flex flex-wrap gap-2">
        {agents.map((agent) => (
          <AgentChip key={agent.name} agent={agent} />
        ))}
      </div>
    </div>
  );
}

function AgentChip({ agent }: { agent: CompanionAgentEntry }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(agent.name).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={`点击复制 agent_key: ${agent.name}`}
      className="group flex items-center gap-1.5 rounded-[8px] border border-border bg-secondary/40 px-2.5 py-1.5 transition-colors hover:bg-secondary/70"
    >
      <span className="text-xs font-medium text-foreground">{agent.display_name}</span>
      <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
        {agent.executor}
      </span>
      <span className="text-[10px] text-muted-foreground/50 opacity-0 group-hover:opacity-100 transition-opacity">
        {copied ? "✓" : "copy"}
      </span>
    </button>
  );
}

function SkillsSection({ skills }: { skills: SkillEntry[] }) {
  const [showAll, setShowAll] = useState(false);
  const INITIAL_SHOW = 5;
  const displayed = showAll ? skills : skills.slice(0, INITIAL_SHOW);
  const hasMore = skills.length > INITIAL_SHOW;

  return (
    <div>
      <p className="mb-1.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">
        可用 Skills
      </p>
      <div className="space-y-1">
        {displayed.map((skill) => (
          <SkillRow key={skill.name} skill={skill} />
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

function SkillRow({ skill }: { skill: SkillEntry }) {
  const [showPath, setShowPath] = useState(false);

  return (
    <div
      className="rounded-[6px] border border-border/70 bg-secondary/25 px-2.5 py-1.5 cursor-pointer hover:bg-secondary/50 transition-colors"
      onClick={() => setShowPath((v) => !v)}
    >
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium text-foreground">{skill.name}</span>
        <span className="flex-1 truncate text-[11px] text-muted-foreground">
          {skill.description.length > 80
            ? `${skill.description.slice(0, 80)}…`
            : skill.description}
        </span>
      </div>
      {showPath && (
        <p className="mt-1 font-mono text-[10px] text-muted-foreground/60 break-all">
          {skill.file_path}
        </p>
      )}
    </div>
  );
}

export default AcpSessionCapabilityCard;
