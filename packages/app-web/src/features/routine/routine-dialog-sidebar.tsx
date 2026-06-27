import type { RoutineTriggerType, RoutineDispatchMode, ProjectAgent, Routine } from "../../types";
import type { RoutineFormState } from "./form-state";
import { CronScheduleSelector } from "./cron-schedule-selector";
import { useState } from "react";
import { useRegenerateRoutineTokenMutation } from "./model/routineQueries";

interface SidebarProps {
  form: RoutineFormState;
  patchForm: (patch: Partial<RoutineFormState>) => void;
  projectAgents: ProjectAgent[];
  mode: "create" | "edit";
  editingRoutine?: Routine;
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <label className="block text-[11px] font-semibold tracking-[0.08em] uppercase text-muted-foreground mb-2">
      {children}
    </label>
  );
}

export function RoutineDialogSidebar({ form, patchForm, projectAgents, mode, editingRoutine }: SidebarProps) {
  const regenerateToken = useRegenerateRoutineTokenMutation();
  const [regeneratedToken, setRegeneratedToken] = useState<{ endpoint_id: string; token: string } | null>(null);

  const handleRegenerate = async () => {
    if (!editingRoutine) return;
    const result = await regenerateToken.mutateAsync(editingRoutine.id);
    if (result) {
      setRegeneratedToken({ endpoint_id: result.endpoint_id, token: result.webhook_token });
    }
  };

  const triggerOptions: Array<{ value: RoutineTriggerType; label: string }> = [
    { value: "scheduled", label: "定时" },
    { value: "webhook", label: "Webhook" },
  ];

  const isPluginType = form.trigger_type === "plugin";
  const isWebhookEdit = mode === "edit" && editingRoutine?.trigger_config.type === "webhook";
  const webhookEndpoint =
    editingRoutine?.trigger_config.type === "webhook"
      ? editingRoutine.trigger_config.endpoint_id
      : null;

  return (
    <aside className="w-[320px] shrink-0 border-l border-border bg-secondary/5 p-5 overflow-y-auto max-lg:w-full max-lg:border-l-0 max-lg:border-t">
      <div className="space-y-5">
        {/* Agent 选择 */}
        <div>
          <SectionLabel>执行 Agent</SectionLabel>
          <select
            value={form.project_agent_id}
            onChange={(e) => patchForm({ project_agent_id: e.target.value })}
            className="agentdash-form-select"
          >
            <option value="">请选择 Agent</option>
            {projectAgents.map((agent) => (
              <option key={agent.id} value={agent.id}>{agent.name}</option>
            ))}
          </select>
        </div>

        {/* 触发方式 */}
        <div>
          <SectionLabel>触发方式</SectionLabel>
          {isPluginType ? (
            <div className="rounded-[8px] border border-primary/20 bg-primary/5 px-3 py-2">
              <span className="text-xs text-primary">Plugin: {form.provider_key}</span>
            </div>
          ) : (
            <div className="flex gap-1 rounded-[8px] border border-border bg-secondary/30 p-1">
              {triggerOptions.map((opt) => (
                <button
                  key={opt.value}
                  type="button"
                  disabled={mode === "edit"}
                  onClick={() => patchForm({ trigger_type: opt.value })}
                  className={`flex-1 rounded-[6px] px-3 py-1.5 text-xs font-medium transition-colors ${
                    form.trigger_type === opt.value
                      ? "bg-background text-foreground shadow-sm border border-border"
                      : "text-muted-foreground border border-transparent"
                  } ${mode === "edit" ? "cursor-default" : "cursor-pointer"}`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* 定时配置 */}
        {form.trigger_type === "scheduled" && (
          <div className="rounded-[8px] border border-border bg-secondary/15 p-3">
            <CronScheduleSelector
              value={form.cron_expression}
              onChange={(cron) => patchForm({ cron_expression: cron })}
            />
          </div>
        )}

        {/* Webhook 信息 */}
        {form.trigger_type === "webhook" && mode === "create" && (
          <div className="rounded-[8px] border border-info/20 bg-info/5 p-3">
            <p className="text-xs text-muted-foreground">
              Endpoint 和 Token 将在创建后自动生成。
            </p>
          </div>
        )}

        {isWebhookEdit && webhookEndpoint && (
          <div className="rounded-[8px] border border-border bg-secondary/15 p-3 space-y-3">
            <div>
              <p className="text-[11px] font-medium text-muted-foreground">触发端点</p>
              <code className="mt-1 block font-mono text-[11px] text-foreground break-all">
                POST /api/routine-triggers/{webhookEndpoint}/fire
              </code>
            </div>
            <button
              type="button"
              onClick={() => void handleRegenerate()}
              disabled={regenerateToken.isPending}
              className="agentdash-button-secondary text-xs"
            >
              {regenerateToken.isPending ? "生成中..." : "重新生成 Token"}
            </button>
            {regeneratedToken && (
              <div className="rounded-[8px] border border-warning/20 bg-warning/5 p-3 space-y-1">
                <p className="text-xs font-medium text-warning">新 Token（仅此一次可见）</p>
                <code className="block font-mono text-xs text-foreground break-all">{regeneratedToken.token}</code>
              </div>
            )}
          </div>
        )}

        {/* Plugin 配置（仅编辑模式展示） */}
        {isPluginType && mode === "edit" && (
          <div className="space-y-3 rounded-[8px] border border-border bg-secondary/15 p-3">
            <div>
              <p className="text-[11px] font-medium text-muted-foreground">Provider Key</p>
              <code className="mt-1 block font-mono text-xs text-foreground">{form.provider_key}</code>
            </div>
            <div>
              <p className="text-[11px] font-medium text-muted-foreground">Provider Config</p>
              <textarea
                value={form.provider_config_json}
                onChange={(e) => patchForm({ provider_config_json: e.target.value })}
                rows={4}
                className="agentdash-form-textarea mt-1 font-mono text-xs"
              />
            </div>
          </div>
        )}

        {/* 高级设置 */}
        <details className="rounded-[8px] border border-border/50 bg-secondary/10 p-2">
          <summary className="cursor-pointer px-2 py-1.5 text-[11px] font-medium text-muted-foreground select-none">
            高级设置
          </summary>
          <div className="mt-2 space-y-3 px-2 pb-1">
            <div>
              <label className="text-[11px] font-medium text-muted-foreground">Dispatch 策略</label>
              <select
                value={form.dispatch_mode}
                onChange={(e) => patchForm({ dispatch_mode: e.target.value as RoutineDispatchMode })}
                className="agentdash-form-select mt-1"
              >
                <option value="fresh">每次新建 (Fresh)</option>
                <option value="reuse">复用已有 (Reuse)</option>
                {form.trigger_type === "webhook" && (
                  <option value="per_entity">按实体分配 (Per Entity)</option>
                )}
              </select>
            </div>
            {form.dispatch_mode === "per_entity" && (
              <div>
                <label className="text-[11px] font-medium text-muted-foreground">Entity Key Path</label>
                <input
                  value={form.entity_key_path}
                  onChange={(e) => patchForm({ entity_key_path: e.target.value })}
                  placeholder="如: pull_request.number"
                  className="agentdash-form-input mt-1 font-mono text-xs"
                />
                <p className="mt-1 text-[10px] text-muted-foreground">
                  从 trigger payload 中按此 JSON 路径提取 entity key，相同 key 复用同一 dispatch 目标
                </p>
              </div>
            )}
          </div>
        </details>
      </div>
    </aside>
  );
}
