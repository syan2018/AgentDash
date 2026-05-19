/**
 * AssetPickerDrawer — Marketplace 顶部"发布资产"主入口背后的资产选择器。
 *
 * 流程：选 type → 选 project asset → 把选中的项作为 publish defaults 透传给上层，
 *         上层挂载 PublishLibraryAssetDialog 完成发布闭环。
 */

import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../../stores/projectStore";
import { useWorkflowStore } from "../../../stores/workflowStore";
import { fetchProjectMcpPresets } from "../../../services/mcpPreset";
import { fetchProjectSkillAssets } from "../../../services/skillAsset";
import type {
  LifecycleDefinition,
  McpPresetDto,
  ProjectAgentLink,
  PublishLibraryAssetKind,
  SkillAssetDto,
} from "../../../types";

export interface AssetPickerSelection {
  assetKind: PublishLibraryAssetKind;
  projectAssetId: string;
  defaults: {
    key: string;
    display_name: string;
    description?: string | null;
  };
}

export interface AssetPickerDrawerProps {
  projectId: string;
  /** 默认聚焦的 type；未传则进入 type 选择步骤 */
  initialKind?: PublishLibraryAssetKind | null;
  onClose: () => void;
  onPick: (selection: AssetPickerSelection) => void;
}

const KIND_OPTIONS: Array<{ value: PublishLibraryAssetKind; label: string; hint: string }> = [
  { value: "project_agent", label: "Agent", hint: "已链接的 Project Agent → agent_template" },
  { value: "mcp_preset", label: "MCP Server", hint: "Project MCP Preset → mcp_server_template" },
  { value: "workflow_bundle", label: "Workflow", hint: "Project Lifecycle bundle → workflow_template" },
  { value: "skill_asset", label: "Skill", hint: "Project Skill 资产 → skill_template" },
];

export function AssetPickerDrawer({
  projectId,
  initialKind = null,
  onClose,
  onPick,
}: AssetPickerDrawerProps) {
  const [kind, setKind] = useState<PublishLibraryAssetKind | null>(initialKind);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <>
      <div className="fixed inset-0 z-[80] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <aside
        className="fixed right-0 top-0 z-[81] flex h-full w-[480px] max-w-full flex-col border-l border-border bg-background shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
          <div>
            <span className="agentdash-panel-header-tag">Shared Library</span>
            <h3 className="mt-1 text-base font-semibold text-foreground">发布资产到资源市场</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              选择资产类型与具体项目资产，发布后将出现在「我发布的」列表。
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-secondary"
          >
            关闭
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-5">
          {kind === null ? (
            <KindStep onPick={setKind} />
          ) : (
            <AssetStep
              projectId={projectId}
              kind={kind}
              onBack={() => setKind(null)}
              onPick={onPick}
            />
          )}
        </div>
      </aside>
    </>
  );
}

function KindStep({ onPick }: { onPick: (kind: PublishLibraryAssetKind) => void }) {
  return (
    <section className="flex flex-col gap-2">
      <p className="text-xs uppercase tracking-[0.14em] text-muted-foreground">第 1 步：选择类型</p>
      <ul className="mt-1 grid gap-2">
        {KIND_OPTIONS.map((option) => (
          <li key={option.value}>
            <button
              type="button"
              onClick={() => onPick(option.value)}
              className="flex w-full items-start gap-3 rounded-[10px] border border-border bg-background p-3 text-left transition-colors hover:border-primary/30 hover:bg-secondary/30"
            >
              <span className="rounded-[6px] border border-border bg-secondary/60 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                {option.label}
              </span>
              <span className="text-xs text-foreground/80">{option.hint}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function AssetStep({
  projectId,
  kind,
  onBack,
  onPick,
}: {
  projectId: string;
  kind: PublishLibraryAssetKind;
  onBack: () => void;
  onPick: (selection: AssetPickerSelection) => void;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs uppercase tracking-[0.14em] text-muted-foreground">
          第 2 步：选择 {kindLabel(kind)} 资产
        </p>
        <button
          type="button"
          onClick={onBack}
          className="text-xs text-muted-foreground transition-colors hover:text-foreground"
        >
          ← 重新选择类型
        </button>
      </div>
      {kind === "project_agent" && (
        <AgentList projectId={projectId} onPick={onPick} />
      )}
      {kind === "mcp_preset" && (
        <McpList projectId={projectId} onPick={onPick} />
      )}
      {kind === "workflow_bundle" && <WorkflowList onPick={onPick} />}
      {kind === "skill_asset" && (
        <SkillList projectId={projectId} onPick={onPick} />
      )}
    </section>
  );
}

function kindLabel(kind: PublishLibraryAssetKind): string {
  return KIND_OPTIONS.find((o) => o.value === kind)?.label ?? kind;
}

/* ─── 各 type 列表 ─── */

function AgentList({
  projectId,
  onPick,
}: {
  projectId: string;
  onPick: (s: AssetPickerSelection) => void;
}) {
  const links = useProjectStore((s) => s.agentLinksByProjectId[projectId]);
  const fetchLinks = useProjectStore((s) => s.fetchProjectAgentLinks);

  useEffect(() => {
    if (!links) void fetchLinks(projectId);
  }, [fetchLinks, projectId, links]);

  if (!links) {
    return <Hint message="正在加载 Agent…" />;
  }
  if (links.length === 0) {
    return <Hint message="项目内暂无可发布的 Agent。请先在 Agent 视图中链接或创建 Agent。" />;
  }
  return (
    <PickList
      items={links}
      keyOf={(l) => l.id}
      titleOf={(l) => l.agent_name}
      hintOf={(l) => `agent_id: ${l.agent_id}`}
      onPick={(link) =>
        onPick({
          assetKind: "project_agent",
          projectAssetId: link.id,
          defaults: {
            key: link.agent_name,
            display_name: link.agent_name,
            description: null,
          },
        })
      }
    />
  );
}

function McpList({
  projectId,
  onPick,
}: {
  projectId: string;
  onPick: (s: AssetPickerSelection) => void;
}) {
  const [items, setItems] = useState<McpPresetDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchProjectMcpPresets(projectId)
      .then((list) => {
        if (cancelled) return;
        setItems(list);
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : "加载失败");
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  // 只显示可发布的（非 builtin、非 installed_source）
  const visible = useMemo(
    () => (items ?? []).filter((p) => p.source !== "builtin" && !p.installed_source),
    [items],
  );

  if (error) return <Hint message={error} tone="danger" />;
  if (items === null) return <Hint message="正在加载 MCP Preset…" />;
  if (visible.length === 0) {
    return (
      <Hint message="项目内暂无可发布的 user MCP Preset。Builtin / 已从市场安装的 Preset 不能再次发布。" />
    );
  }
  return (
    <PickList
      items={visible}
      keyOf={(p) => p.id}
      titleOf={(p) => p.display_name}
      hintOf={(p) => `key: ${p.key}`}
      onPick={(preset) =>
        onPick({
          assetKind: "mcp_preset",
          projectAssetId: preset.id,
          defaults: {
            key: preset.key,
            display_name: preset.display_name,
            description: preset.description ?? null,
          },
        })
      }
    />
  );
}

function WorkflowList({ onPick }: { onPick: (s: AssetPickerSelection) => void }) {
  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  // 与 MCP 一致：过滤 builtin_seed 与 installed
  const visible = useMemo(
    () =>
      lifecycles.filter(
        (lc) => lc.source !== "builtin_seed" && !lc.installed_source,
      ),
    [lifecycles],
  );

  if (lifecycles.length === 0) {
    return <Hint message="项目内暂无 Workflow / Lifecycle。请先在 Workflow 视图创建。" />;
  }
  if (visible.length === 0) {
    return <Hint message="所有 Workflow 都来自 builtin 或 marketplace 安装，不可再次发布。" />;
  }
  return (
    <PickList
      items={visible}
      keyOf={(lc) => lc.id}
      titleOf={(lc: LifecycleDefinition) => lc.name}
      hintOf={(lc) => `key: ${lc.key}`}
      onPick={(lc) =>
        onPick({
          assetKind: "workflow_bundle",
          projectAssetId: lc.id,
          defaults: {
            key: lc.key,
            display_name: lc.name,
            description: lc.description ?? null,
          },
        })
      }
    />
  );
}

function SkillList({
  projectId,
  onPick,
}: {
  projectId: string;
  onPick: (s: AssetPickerSelection) => void;
}) {
  const [items, setItems] = useState<SkillAssetDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchProjectSkillAssets(projectId)
      .then((list) => {
        if (cancelled) return;
        setItems(list);
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : "加载失败");
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const visible = useMemo(
    () =>
      (items ?? []).filter(
        (s) => s.source !== "builtin_seed" && !s.installed_source,
      ),
    [items],
  );

  if (error) return <Hint message={error} tone="danger" />;
  if (items === null) return <Hint message="正在加载 Skill…" />;
  if (visible.length === 0) {
    return <Hint message="项目内暂无可发布的 user Skill。" />;
  }
  return (
    <PickList
      items={visible}
      keyOf={(s) => s.id}
      titleOf={(s) => s.display_name}
      hintOf={(s) => `skills/${s.key}/SKILL.md`}
      onPick={(skill) =>
        onPick({
          assetKind: "skill_asset",
          projectAssetId: skill.id,
          defaults: {
            key: skill.key,
            display_name: skill.display_name,
            description: skill.description,
          },
        })
      }
    />
  );
}

/* ─── 通用列表 / 提示 ─── */

function PickList<T>({
  items,
  keyOf,
  titleOf,
  hintOf,
  onPick,
}: {
  items: T[];
  keyOf: (item: T) => string;
  titleOf: (item: T) => string;
  hintOf: (item: T) => string;
  onPick: (item: T) => void;
}) {
  return (
    <ul className="grid gap-2">
      {items.map((item) => (
        <li key={keyOf(item)}>
          <button
            type="button"
            onClick={() => onPick(item)}
            className="flex w-full flex-col rounded-[10px] border border-border bg-background p-3 text-left transition-colors hover:border-primary/30 hover:bg-secondary/30"
          >
            <span className="text-sm font-medium text-foreground">{titleOf(item)}</span>
            <span className="mt-0.5 text-xs text-muted-foreground">{hintOf(item)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function Hint({ message, tone }: { message: string; tone?: "danger" }) {
  const className =
    tone === "danger"
      ? "rounded-[10px] border border-destructive/20 bg-destructive/5 px-3 py-3 text-xs text-destructive"
      : "rounded-[10px] border border-dashed border-border bg-secondary/20 px-3 py-3 text-xs text-muted-foreground";
  return <p className={className}>{message}</p>;
}

// 防止 TypeScript 把未引用的类型告警 — 让 ProjectAgentLink 在签名层面被 import 使用
export type { ProjectAgentLink };
