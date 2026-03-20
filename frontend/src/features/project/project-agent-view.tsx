import type { ProjectAgentSummary } from "../../types";

export interface ProjectAgentViewProps {
  projectName: string;
  agents: ProjectAgentSummary[];
  isLoading?: boolean;
  error?: string | null;
  onOpenAgent: (agent: ProjectAgentSummary) => void;
}

function formatWritebackMode(mode: ProjectAgentSummary["writeback_mode"]): string {
  return mode === "confirm_before_write" ? "确认后写回" : "只读";
}

export function ProjectAgentView({
  projectName,
  agents,
  isLoading = false,
  error = null,
  onOpenAgent,
}: ProjectAgentViewProps) {
  if (isLoading && agents.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="mb-5 max-w-3xl">
        <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
          Agent Hub
        </p>
        <h2 className="mt-1 text-2xl font-semibold text-foreground">{projectName} 的协作 Agent</h2>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          这里展示的是用户可以直接对话的 Project Agent，而不是底层模板或权限拼装细节。每个 Agent
          都对应一条项目级会话入口，用来维护共享资料、沉淀背景信息，或者为后续 Story 做准备。
        </p>
      </div>

      {error && (
        <div className="mb-4 rounded-[14px] border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          Agent 列表加载异常：{error}
        </div>
      )}

      {agents.length === 0 ? (
        <div className="rounded-[18px] border border-dashed border-border bg-background/60 px-6 py-10 text-center">
          <p className="text-sm text-foreground">当前 Project 还没有可直接使用的 Agent。</p>
          <p className="mt-2 text-xs leading-5 text-muted-foreground">
            至少配置 `default_agent_type` 或一个 `agent_preset` 后，这里就会出现对应的 Project Agent
            入口。
          </p>
        </div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-2 2xl:grid-cols-3">
          {agents.map((agent) => (
            <article
              key={agent.key}
              className="flex min-h-[250px] flex-col rounded-[22px] border border-border bg-background/75 p-5 shadow-sm"
            >
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-lg font-semibold text-foreground">{agent.display_name}</p>
                  <p className="mt-1 text-sm leading-6 text-muted-foreground">{agent.description}</p>
                </div>
                <span className="rounded-full border border-border bg-secondary px-2.5 py-1 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                  {agent.executor.executor}
                </span>
              </div>

              <div className="mt-4 flex flex-wrap gap-2">
                <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-1 text-[11px] text-muted-foreground">
                  {formatWritebackMode(agent.writeback_mode)}
                </span>
                {agent.preset_name && (
                  <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-1 text-[11px] text-muted-foreground">
                    预设: {agent.preset_name}
                  </span>
                )}
                {agent.executor.variant && (
                  <span className="rounded-full border border-border bg-secondary/60 px-2.5 py-1 text-[11px] text-muted-foreground">
                    variant: {agent.executor.variant}
                  </span>
                )}
              </div>

              <div className="mt-5">
                <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
                  共享资料
                </p>
                {agent.shared_context_mounts.length > 0 ? (
                  <div className="mt-2 flex flex-wrap gap-2">
                    {agent.shared_context_mounts.map((mount) => (
                      <span
                        key={`${agent.key}-${mount.mount_id}`}
                        className="rounded-[10px] border border-border bg-secondary/40 px-2.5 py-1 text-xs text-foreground/85"
                      >
                        {mount.display_name}
                        <span className="ml-1 font-mono text-[10px] text-muted-foreground">
                          /{mount.mount_id}
                        </span>
                      </span>
                    ))}
                  </div>
                ) : (
                  <p className="mt-2 text-xs text-muted-foreground">
                    当前没有显式暴露给 Project Session 的共享资料容器。
                  </p>
                )}
              </div>

              <div className="mt-auto pt-5">
                {agent.session?.session_title && (
                  <p className="mb-2 text-[11px] text-muted-foreground">
                    最近会话：{agent.session.session_title}
                  </p>
                )}
                <button
                  type="button"
                  onClick={() => onOpenAgent(agent)}
                  className="w-full rounded-[12px] border border-primary bg-primary px-3 py-2.5 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-95"
                >
                  打开 Agent 会话
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
