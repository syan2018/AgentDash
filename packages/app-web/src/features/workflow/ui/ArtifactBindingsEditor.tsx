/**
 * ArtifactBindingsEditor —— transition 的 artifact_bindings 列表编辑。
 *
 * 仅在 transition.kind === "artifact" 时渲染；接受所有 lifecycle activity 列表
 * 用于 from_activity / from_port 选择，目标 activity 的 input_ports 用于 to_port。
 */

import type {
  ActivityDefinition,
  ArtifactAliasPolicy,
  ArtifactBinding,
} from "../../../types";

export interface ArtifactBindingsEditorProps {
  bindings: ArtifactBinding[];
  /** 全部 lifecycle activity（含 from / to 自身） */
  activities: ActivityDefinition[];
  /** 目标 activity（transition.to）—— 用于 to_port 候选 */
  toActivity: ActivityDefinition | null;
  /** 默认 from_activity（通常 transition.from） */
  defaultFromActivity: string;
  onAdd: (binding: ArtifactBinding) => void;
  onUpdate: (idx: number, patch: Partial<ArtifactBinding>) => void;
  onRemove: (idx: number) => void;
}

const ALIAS_OPTIONS: ArtifactAliasPolicy[] = ["latest", "per_attempt", "latest_and_history"];

export function ArtifactBindingsEditor({
  bindings,
  activities,
  toActivity,
  defaultFromActivity,
  onAdd,
  onUpdate,
  onRemove,
}: ArtifactBindingsEditorProps) {
  const handleAdd = () => {
    onAdd({
      from_activity: defaultFromActivity || undefined,
      from_port: "",
      to_port: "",
      alias: "latest",
    });
  };

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className="agentdash-form-label m-0">Artifact Bindings ({bindings.length})</label>
        <button
          type="button"
          onClick={handleAdd}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          + 添加
        </button>
      </div>

      {bindings.length === 0 ? (
        <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-3 text-center text-xs text-muted-foreground">
          暂无 binding；artifact transition 至少需要一条
        </p>
      ) : (
        <div className="space-y-2">
          {bindings.map((b, idx) => (
            <BindingRow
              key={idx}
              binding={b}
              activities={activities}
              toActivity={toActivity}
              onChange={(patch) => onUpdate(idx, patch)}
              onRemove={() => onRemove(idx)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function BindingRow({
  binding,
  activities,
  toActivity,
  onChange,
  onRemove,
}: {
  binding: ArtifactBinding;
  activities: ActivityDefinition[];
  toActivity: ActivityDefinition | null;
  onChange: (patch: Partial<ArtifactBinding>) => void;
  onRemove: () => void;
}) {
  const fromActivity = binding.from_activity ?? "";
  const fromActivityDef = activities.find((a) => a.key === fromActivity);
  const fromPorts = fromActivityDef?.output_ports ?? [];
  const toPorts = toActivity?.input_ports ?? [];

  return (
    <div className="rounded-[8px] border border-border bg-background p-2.5 space-y-1.5">
      <div className="grid grid-cols-2 gap-2">
        <div>
          <label className="agentdash-form-label">From Activity</label>
          <select
            value={fromActivity}
            onChange={(e) => onChange({ from_activity: e.target.value || undefined })}
            className="agentdash-form-select"
          >
            <option value="">(任意 / 上游)</option>
            {activities.map((a) => (
              <option key={a.key} value={a.key}>
                {a.key}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="agentdash-form-label">From Port</label>
          <input
            value={binding.from_port}
            list={`bind-fromports-${fromActivity}`}
            onChange={(e) => onChange({ from_port: e.target.value })}
            className="agentdash-form-input"
            placeholder="output port key"
          />
          <datalist id={`bind-fromports-${fromActivity}`}>
            {fromPorts.map((p) => (
              <option key={p.key} value={p.key} />
            ))}
          </datalist>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div>
          <label className="agentdash-form-label">To Port</label>
          <input
            value={binding.to_port}
            list={`bind-toports-${toActivity?.key ?? "none"}`}
            onChange={(e) => onChange({ to_port: e.target.value })}
            className="agentdash-form-input"
            placeholder="input port key"
          />
          <datalist id={`bind-toports-${toActivity?.key ?? "none"}`}>
            {toPorts.map((p) => (
              <option key={p.key} value={p.key} />
            ))}
          </datalist>
        </div>
        <div>
          <label className="agentdash-form-label">Alias</label>
          <select
            value={binding.alias}
            onChange={(e) => onChange({ alias: e.target.value as ArtifactAliasPolicy })}
            className="agentdash-form-select"
          >
            {ALIAS_OPTIONS.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="flex justify-end">
        <button
          type="button"
          onClick={onRemove}
          className="rounded-[6px] px-2 py-0.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
        >
          删除
        </button>
      </div>
    </div>
  );
}
