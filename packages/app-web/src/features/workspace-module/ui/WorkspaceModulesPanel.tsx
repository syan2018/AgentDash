import { useProjectWorkspaceModules } from "../model/useProjectWorkspaceModules";
import type { WorkspaceModuleDescriptor } from "../../../generated/workspace-module-contracts";

const KIND_LABELS: Record<string, string> = {
  extension: "Extension",
  canvas: "Canvas",
  builtin: "Builtin",
};

function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

function StatusBadge({ module }: { module: WorkspaceModuleDescriptor }) {
  const { status } = module.summary;
  if (status.kind === "ready") {
    return (
      <span className="inline-flex items-center rounded-[8px] border border-success/25 bg-success/10 px-2 py-0.5 text-xs font-medium text-success">
        Ready
      </span>
    );
  }
  return (
    <span
      className="inline-flex items-center rounded-[8px] border border-warning/25 bg-warning/10 px-2 py-0.5 text-xs font-medium text-warning"
      title={status.reason ?? undefined}
    >
      Unavailable
    </span>
  );
}

/**
 * 项目层 WorkspaceModule 合并认知视图：把 Canvas + Extension 贡献的 module
 * 统一列出（kind / title / source / status / operations 数 / ui_entries 数）。
 *
 * 复用 Child 1 的 canonical projection（`useProjectWorkspaceModules`），不引入第二份 DTO。
 * 本区块聚焦"合并认知 + 诊断"；enable/disable 复用现有 extension 安装管理，不在此重建。
 * per-frame 可见性裁切编辑属 agent frame 编辑面，不在项目设置页（design §5）。
 */
export function WorkspaceModulesPanel({ projectId }: { projectId: string }) {
  const state = useProjectWorkspaceModules(projectId);

  if (state.status === "loading" || state.status === "idle") {
    return <p className="text-xs text-muted-foreground">正在加载 Workspace Module...</p>;
  }

  if (state.status === "error") {
    return (
      <p className="text-xs text-destructive">
        Workspace Module 加载失败：{state.error ?? "未知错误"}
      </p>
    );
  }

  if (state.modules.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        当前项目没有任何 Canvas 或 Extension 贡献的 Workspace Module。
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {state.status === "refreshing" && (
        <p className="text-xs text-muted-foreground">正在刷新...</p>
      )}
      {state.modules.map((module) => {
        const { summary } = module;
        const unavailable = summary.status.kind === "unavailable";
        return (
          <div
            key={summary.module_id}
            className="rounded-lg border border-border/70 bg-card/40 p-3"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold text-foreground">{summary.title}</span>
                  <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    {kindLabel(summary.kind)}
                  </span>
                  <StatusBadge module={module} />
                </div>
                <p className="text-xs text-muted-foreground">
                  来源 <code className="text-[11px]">{summary.source}</code>
                </p>
                {summary.description && (
                  <p className="text-xs leading-5 text-muted-foreground">{summary.description}</p>
                )}
              </div>
              <div className="shrink-0 text-right text-xs text-muted-foreground">
                <div>{module.operations.length} operations</div>
                <div>{module.ui_entries.length} UI entries</div>
              </div>
            </div>
            {unavailable && summary.status.reason && (
              <p className="mt-2 rounded-[8px] border border-warning/20 bg-warning/10 px-2 py-1 text-xs text-warning">
                诊断：{summary.status.reason}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
