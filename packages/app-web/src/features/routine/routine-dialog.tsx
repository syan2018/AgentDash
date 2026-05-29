import { useState } from "react";
import type { Routine, ProjectAgent } from "../../types";
import type { RoutineFormState } from "./form-state";
import { formToPayload, validateForm } from "./form-state";
import { RoutineDialogSidebar } from "./routine-dialog-sidebar";

interface RoutineDialogProps {
  mode: "create" | "edit";
  initial: RoutineFormState;
  projectAgents: ProjectAgent[];
  editingRoutine?: Routine;
  onSave: (payload: ReturnType<typeof formToPayload>) => Promise<void>;
  onClose: () => void;
}

export function RoutineDialog({ mode, initial, projectAgents, editingRoutine, onSave, onClose }: RoutineDialogProps) {
  const [form, setForm] = useState<RoutineFormState>(initial);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [showVars, setShowVars] = useState(false);

  const patchForm = (patch: Partial<RoutineFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setError(null);
  };

  const handleSubmit = async () => {
    const validationError = validateForm(form);
    if (validationError) {
      setError(validationError);
      return;
    }
    setError(null);
    setSaving(true);
    try {
      await onSave(formToPayload(form));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="flex w-full max-w-[min(960px,80vw)] flex-col rounded-[12px] border border-border bg-background shadow-2xl" style={{ maxHeight: "75vh", minHeight: "480px" }}>
          {/* Header */}
          <div className="shrink-0 border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Routine</span>
            <h4 className="text-base font-semibold text-foreground">
              {mode === "create" ? "创建 Routine" : "编辑 Routine"}
            </h4>
            <p className="mt-0.5 text-xs text-muted-foreground">
              配置一个自动化任务，让 Agent 按计划或事件自动执行
            </p>
          </div>

          {/* Body — two columns */}
          <div className="flex flex-1 overflow-hidden max-lg:flex-col">
            {/* Left column */}
            <div className="flex flex-1 flex-col overflow-y-auto p-5">
              {error && (
                <div className="mb-4 rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                  {error}
                </div>
              )}

              <div className="space-y-4">
                {/* 名称 */}
                <div>
                  <label className="agentdash-form-label">名称</label>
                  <input
                    value={form.name}
                    onChange={(e) => patchForm({ name: e.target.value })}
                    placeholder="如: daily-code-review"
                    className="agentdash-form-input"
                    autoFocus
                  />
                </div>

                {/* 执行指令 */}
                <div className="flex flex-1 flex-col">
                  <label className="agentdash-form-label">执行指令</label>
                  <textarea
                    value={form.prompt_template}
                    onChange={(e) => patchForm({ prompt_template: e.target.value })}
                    placeholder={"描述你希望 Agent 在每次触发时执行的任务...\n\n例：检查最近 24 小时的 PR，对每个 PR 给出代码质量评分和改进建议。"}
                    rows={8}
                    className="agentdash-form-textarea flex-1 text-sm"
                  />
                </div>

                {/* 模板变量折叠提示 */}
                <div>
                  <button
                    type="button"
                    onClick={() => setShowVars(!showVars)}
                    className="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {showVars ? "▾" : "▸"} 支持模板变量
                  </button>
                  {showVars && (
                    <div className="mt-2 rounded-[8px] border border-border/50 bg-secondary/10 p-3">
                      <p className="text-[11px] text-muted-foreground mb-1.5">可在指令中使用 Tera/Jinja2 模板语法：</p>
                      <ul className="space-y-1 font-mono text-[10px] text-muted-foreground">
                        <li><code className="text-foreground/80">{"{{ trigger.source }}"}</code> — 触发来源</li>
                        <li><code className="text-foreground/80">{"{{ trigger.timestamp }}"}</code> — 触发时间</li>
                        <li><code className="text-foreground/80">{"{{ trigger.payload.* }}"}</code> — Webhook payload 字段</li>
                        <li><code className="text-foreground/80">{"{{ routine.name }}"}</code> — Routine 名称</li>
                        <li><code className="text-foreground/80">{"{{ routine.project_id }}"}</code> — 所属项目 ID</li>
                      </ul>
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* Right column */}
            <RoutineDialogSidebar
              form={form}
              patchForm={patchForm}
              projectAgents={projectAgents}
              mode={mode}
              editingRoutine={editingRoutine}
            />
          </div>

          {/* Footer */}
          <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} className="agentdash-button-secondary">
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={saving}
              className="agentdash-button-primary"
            >
              {saving ? "保存中..." : mode === "create" ? "创建" : "保存"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
