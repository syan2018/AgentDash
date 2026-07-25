import { isWorkflowJsonValue } from "../../../types";
import type {
  ActivityCompletionPolicy,
  ActivityDefinition,
  ActivityExecutorSpec,
  ActivityJoinPolicy,
  AgentReusePolicy,
  ArtifactAliasPolicy,
  CapabilityDirective,
  HookRulePreset,
  InputPortDefinition,
  OutputPortDefinition,
  RuntimeThreadPolicy,
  WorkflowContextBinding,
  AgentProcedure,
  WorkflowHookRuleSpec,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../../types";
import type { AgentProcedureDraft } from "../../../stores/workflowStore";
import {
  CapabilityPanel,
  HookRulesPanel,
  InjectionPanel,
  InputPortItem,
  OutputPortItem,
  PortsPanel,
} from "./panels";

export function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex-1 rounded-[8px] px-2 py-1.5 text-xs font-medium transition-colors ${
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

// ─── Header ─────────────────────────────────────────────

export function Header({
  activity,
  isEntry,
  onSetEntry,
  onClose,
}: {
  activity: ActivityDefinition;
  isEntry: boolean;
  onSetEntry: () => void;
  onClose: () => void;
}) {
  return (
    <header className="sticky top-0 z-10 flex shrink-0 items-center justify-between border-b border-border bg-background px-4 py-3">
      <div className="overflow-hidden">
        <p className="truncate text-sm font-semibold text-foreground">
          {activity.key || "(no key)"}
        </p>
        {isEntry ? (
          <p className="text-[10px] text-success">入口节点</p>
        ) : (
          <button
            type="button"
            onClick={onSetEntry}
            className="mt-0.5 rounded-[6px] px-1.5 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/10"
          >
            设为入口
          </button>
        )}
      </div>
      <button
        type="button"
        onClick={onClose}
        className="rounded-[8px] p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="关闭面板"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M18 6 6 18" />
          <path d="m6 6 12 12" />
        </svg>
      </button>
    </header>
  );
}

// ─── §1 Identity ────────────────────────────────────────

export function IdentitySection({
  activity,
  onActivityChange,
  onSetIterationPolicy,
  onSetJoinPolicy,
}: {
  activity: ActivityDefinition;
  onActivityChange: (patch: Partial<ActivityDefinition>) => void;
  onSetIterationPolicy: (patch: Partial<ActivityDefinition["iteration_policy"]>) => void;
  onSetJoinPolicy: (policy: ActivityJoinPolicy) => void;
}) {
  const maxAttempts = activity.iteration_policy.max_attempts;
  const isInfinite = maxAttempts === null || maxAttempts === undefined;

  const joinKind: "all" | "any" | "first" | "n_of_m" =
    typeof activity.join_policy === "string" ? activity.join_policy : "n_of_m";
  const joinN = typeof activity.join_policy === "object" ? activity.join_policy.n_of_m.n : 1;

  const handleJoinKindChange = (kind: "all" | "any" | "first" | "n_of_m") => {
    if (kind === "n_of_m") {
      onSetJoinPolicy({ n_of_m: { n: Math.max(1, joinN) } });
    } else {
      onSetJoinPolicy(kind);
    }
  };

  return (
    <section className="space-y-3">
      <SectionTitle>Identity</SectionTitle>

      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={activity.key}
          onChange={(e) => onActivityChange({ key: e.target.value })}
          className="agentdash-form-input"
          placeholder="implement"
        />
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={activity.description}
          onChange={(e) => onActivityChange({ description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea"
          placeholder="Activity 的职责与边界"
        />
      </div>

      {/* iteration / join 是 lifecycle 进阶语义，默认折叠以减轻视觉负担 */}
      <details className="rounded-[8px] border border-border bg-secondary/15 p-2">
        <summary className="cursor-pointer text-[11px] font-medium text-foreground">
          高级（迭代 / 汇聚）
          <span className="ml-2 text-[10px] font-normal text-muted-foreground">
            iter:{isInfinite ? "∞" : maxAttempts}/{activity.iteration_policy.artifact_alias} · join:{joinKind === "n_of_m" ? `n_of_m(${joinN})` : joinKind}
          </span>
        </summary>
        <div className="mt-2 space-y-3">
          <div>
            <label className="agentdash-form-label">Iteration Policy</label>
            <div className="grid gap-2">
              <div className="flex items-center gap-2">
                <input
                  type="number"
                  min={1}
                  value={isInfinite ? "" : maxAttempts ?? ""}
                  disabled={isInfinite}
                  onChange={(e) => {
                    const n = Number.parseInt(e.target.value, 10);
                    onSetIterationPolicy({ max_attempts: Number.isFinite(n) && n > 0 ? n : 1 });
                  }}
                  className="agentdash-form-input flex-1 disabled:opacity-50"
                  placeholder="max_attempts"
                />
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  <input
                    type="checkbox"
                    checked={isInfinite}
                    onChange={(e) => onSetIterationPolicy({ max_attempts: e.target.checked ? undefined : 1 })}
                  />
                  无限
                </label>
              </div>
              <select
                value={activity.iteration_policy.artifact_alias}
                onChange={(e) =>
                  onSetIterationPolicy({ artifact_alias: e.target.value as ArtifactAliasPolicy })
                }
                className="agentdash-form-select"
              >
                <option value="latest">latest</option>
                <option value="per_attempt">per_attempt</option>
                <option value="latest_and_history">latest_and_history</option>
              </select>
            </div>
          </div>

          <div>
            <label className="agentdash-form-label">Join Policy</label>
            <div className="flex items-center gap-2">
              <select
                value={joinKind}
                onChange={(e) =>
                  handleJoinKindChange(e.target.value as "all" | "any" | "first" | "n_of_m")
                }
                className="agentdash-form-select flex-1"
              >
                <option value="all">all</option>
                <option value="any">any</option>
                <option value="first">first</option>
                <option value="n_of_m">n_of_m</option>
              </select>
              {joinKind === "n_of_m" && (
                <input
                  type="number"
                  min={1}
                  value={joinN}
                  onChange={(e) => {
                    const n = Number.parseInt(e.target.value, 10);
                    onSetJoinPolicy({ n_of_m: { n: Number.isFinite(n) && n > 0 ? n : 1 } });
                  }}
                  className="agentdash-form-input w-20"
                  placeholder="n"
                />
              )}
            </div>
          </div>
        </div>
      </details>
    </section>
  );
}

// ─── §2 Executor ────────────────────────────────────────

export function ExecutorSection({
  activity,
  procedureDraft,
  availableProcedures,
  isEntry,
  onExecutorChange,
}: {
  activity: ActivityDefinition;
  procedureDraft: AgentProcedureDraft;
  availableProcedures: AgentProcedure[];
  isEntry: boolean;
  onExecutorChange: (next: ActivityExecutorSpec) => void;
}) {
  const handleKindSwitch = (kind: ActivityExecutorSpec["kind"]) => {
    if (kind === activity.executor.kind) return;
    if (kind === "agent") {
      onExecutorChange({
        kind: "agent",
        procedure_key: procedureDraft.key,
        agent_reuse_policy: "create_activity_agent",
        runtime_thread_policy: "create_new",
      });
    } else if (kind === "human") {
      onExecutorChange({
        kind: "human",
        type: "approval",
        form_schema_key: "approval",
        title: undefined,
      });
    } else {
      onExecutorChange({
        kind: "function",
        type: "bash_exec",
        command: "",
        args: [],
        working_directory: undefined,
      });
    }
  };

  return (
    <section className="space-y-3">
      <SectionTitle>Executor</SectionTitle>

      <select
        value={activity.executor.kind}
        onChange={(e) => handleKindSwitch(e.target.value as ActivityExecutorSpec["kind"])}
        className="agentdash-form-select"
      >
        <option value="agent">Agent</option>
        <option value="human">Human Approval</option>
        <option value="function" disabled={isEntry}>
          Function{isEntry ? "（入口暂不支持）" : ""}
        </option>
      </select>

      {activity.executor.kind === "agent" && (
        <AgentExecutorForm
          executor={activity.executor}
          procedureKeyHint={procedureDraft.key}
          availableProcedures={availableProcedures}
          onChange={onExecutorChange}
        />
      )}

      {activity.executor.kind === "function" && (
        <FunctionExecutorForm executor={activity.executor} onChange={onExecutorChange} />
      )}

      {activity.executor.kind === "human" && (
        <HumanExecutorForm executor={activity.executor} onChange={onExecutorChange} />
      )}
    </section>
  );
}

function AgentExecutorForm({
  executor,
  procedureKeyHint,
  availableProcedures,
  onChange,
}: {
  executor: Extract<ActivityExecutorSpec, { kind: "agent" }>;
  procedureKeyHint: string;
  availableProcedures: AgentProcedure[];
  onChange: (next: ActivityExecutorSpec) => void;
}) {
  const sortedProcedures = [...availableProcedures].sort((a, b) =>
    a.name.localeCompare(b.name, "zh-CN"),
  );
  const isOwn = executor.procedure_key === procedureKeyHint;
  const mode: "own" | "reference" = isOwn ? "own" : "reference";

  return (
    <div className="grid gap-2">
      <div>
        <label className="agentdash-form-label">Procedure 来源</label>
        <div className="flex gap-1 rounded-[8px] border border-border bg-secondary/35 p-1">
          <ModeButton
            active={mode === "own"}
            onClick={() => onChange({ ...executor, procedure_key: procedureKeyHint })}
          >
            专属（随此 activity 创建）
          </ModeButton>
          <ModeButton
            active={mode === "reference"}
            onClick={() => {
              const first = sortedProcedures.find((procedure) => procedure.key !== procedureKeyHint);
              onChange({ ...executor, procedure_key: first?.key ?? "" });
            }}
          >
            引用已有
          </ModeButton>
        </div>
      </div>

      {mode === "own" ? (
        <div className="rounded-[8px] border border-primary/30 bg-primary/5 px-3 py-2">
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground">Procedure Key</p>
          <p className="mt-0.5 truncate font-mono text-xs text-foreground">{procedureKeyHint}</p>
        </div>
      ) : (
        <div>
          <label className="agentdash-form-label">引用 Procedure</label>
          <select
            value={executor.procedure_key}
            onChange={(e) => onChange({ ...executor, procedure_key: e.target.value })}
            className="agentdash-form-select"
          >
            {!sortedProcedures.some((procedure) => procedure.key === executor.procedure_key) && (
              <option value={executor.procedure_key}>
                {executor.procedure_key || "(请选择)"}
              </option>
            )}
            {sortedProcedures
              .filter((procedure) => procedure.key !== procedureKeyHint)
              .map((procedure) => (
                <option key={procedure.id} value={procedure.key}>
                  {procedure.name}{procedure.name !== procedure.key ? ` · ${procedure.key}` : ""}
                </option>
              ))}
          </select>
          <p className="mt-1 text-[11px] text-warning">
            修改 Contract 会影响所有引用此 procedure 的 activity
          </p>
        </div>
      )}

      <div>
        <label className="agentdash-form-label">Agent Reuse</label>
        <select
          value={executor.agent_reuse_policy}
          onChange={(e) =>
            onChange({ ...executor, agent_reuse_policy: e.target.value as AgentReusePolicy })
          }
          className="agentdash-form-select"
        >
          <option value="create_activity_agent">Create Activity Agent</option>
          <option value="continue_current_agent">Continue Current Agent</option>
        </select>
      </div>

      <div>
        <label className="agentdash-form-label">Runtime Session</label>
        <select
          value={executor.runtime_thread_policy}
          onChange={(e) =>
            onChange({
              ...executor,
              runtime_thread_policy: e.target.value as RuntimeThreadPolicy,
            })
          }
          className="agentdash-form-select"
        >
          <option value="create_new">Create New</option>
          <option value="deliver_to_current_thread">Deliver To Current Trace</option>
        </select>
      </div>
    </div>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex-1 rounded-[6px] px-2 py-1 text-[11px] font-medium transition-colors ${
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function FunctionExecutorForm({
  executor,
  onChange,
}: {
  executor: Extract<ActivityExecutorSpec, { kind: "function" }>;
  onChange: (next: ActivityExecutorSpec) => void;
}) {
  const handleTypeSwitch = (type: "api_request" | "bash_exec") => {
    if (type === executor.type) return;
    if (type === "api_request") {
      onChange({
        kind: "function",
        type: "api_request",
        method: "POST",
        url_template: "",
        body_template: undefined,
      });
    } else {
      onChange({
        kind: "function",
        type: "bash_exec",
        command: "",
        args: [],
        working_directory: undefined,
      });
    }
  };

  return (
    <div className="grid gap-2">
      <div>
        <label className="agentdash-form-label">Function Type</label>
        <select
          value={executor.type}
          onChange={(e) => handleTypeSwitch(e.target.value as "api_request" | "bash_exec")}
          className="agentdash-form-select"
        >
          <option value="api_request">API Request</option>
          <option value="bash_exec">Bash Exec</option>
        </select>
      </div>

      {executor.type === "api_request" && (
        <>
          <div>
            <label className="agentdash-form-label">Method</label>
            <select
              value={executor.method}
              onChange={(e) => onChange({ ...executor, method: e.target.value })}
              className="agentdash-form-select"
            >
              <option value="GET">GET</option>
              <option value="POST">POST</option>
              <option value="PUT">PUT</option>
              <option value="PATCH">PATCH</option>
              <option value="DELETE">DELETE</option>
            </select>
          </div>
          <div>
            <label className="agentdash-form-label">URL Template</label>
            <input
              value={executor.url_template}
              onChange={(e) => onChange({ ...executor, url_template: e.target.value })}
              className="agentdash-form-input"
              placeholder="https://api.example.com/{{ inputs.path }}"
            />
          </div>
          <details className="rounded-[8px] border border-border bg-secondary/15 p-2">
            <summary className="cursor-pointer text-[11px] font-medium text-foreground">
              高级（请求体）
              <span className="ml-2 text-[10px] font-normal text-muted-foreground">
                {executor.body_template ? "已配置" : "无"}
              </span>
            </summary>
            <div className="mt-2">
              <label className="agentdash-form-label">Body Template (JSON)</label>
              <textarea
                value={executor.body_template ? JSON.stringify(executor.body_template, null, 2) : ""}
                onChange={(e) => {
                  const raw = e.target.value.trim();
                  if (raw === "") {
                    onChange({ ...executor, body_template: null });
                    return;
                  }
                  try {
                    const parsed: unknown = JSON.parse(raw);
                    if (isWorkflowJsonValue(parsed)) {
                      onChange({ ...executor, body_template: parsed });
                    }
                  } catch {
                    // keep typing；不抛错
                  }
                }}
                rows={4}
                className="agentdash-form-textarea font-mono text-xs"
                placeholder={`{\n  "key": "{{ inputs.value }}"\n}`}
              />
            </div>
          </details>
        </>
      )}

      {executor.type === "bash_exec" && (
        <>
          <div>
            <label className="agentdash-form-label">Command</label>
            <input
              value={executor.command}
              onChange={(e) => onChange({ ...executor, command: e.target.value })}
              className="agentdash-form-input"
              placeholder="pnpm"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Args</label>
            <input
              value={(executor.args ?? []).join(" ")}
              onChange={(e) =>
                onChange({ ...executor, args: e.target.value.split(" ").filter(Boolean) })
              }
              className="agentdash-form-input"
              placeholder="test workflow"
            />
          </div>
          <details className="rounded-[8px] border border-border bg-secondary/15 p-2">
            <summary className="cursor-pointer text-[11px] font-medium text-foreground">
              高级（工作目录）
              <span className="ml-2 text-[10px] font-normal text-muted-foreground">
                {executor.working_directory ?? "默认"}
              </span>
            </summary>
            <div className="mt-2">
              <label className="agentdash-form-label">Working Directory</label>
              <input
                value={executor.working_directory ?? ""}
                onChange={(e) =>
                  onChange({ ...executor, working_directory: e.target.value || undefined })
                }
                className="agentdash-form-input"
                placeholder="(可空，默认 lifecycle 工作区)"
              />
            </div>
          </details>
        </>
      )}
    </div>
  );
}

function HumanExecutorForm({
  executor,
  onChange,
}: {
  executor: Extract<ActivityExecutorSpec, { kind: "human" }>;
  onChange: (next: ActivityExecutorSpec) => void;
}) {
  return (
    <div className="grid gap-2">
      <div>
        <label className="agentdash-form-label">Form Schema Key</label>
        <input
          value={executor.form_schema_key}
          onChange={(e) => onChange({ ...executor, form_schema_key: e.target.value })}
          className="agentdash-form-input"
          placeholder="approval"
        />
      </div>
      <details className="rounded-[8px] border border-border bg-secondary/15 p-2">
        <summary className="cursor-pointer text-[11px] font-medium text-foreground">
          高级（标题）
          <span className="ml-2 text-[10px] font-normal text-muted-foreground">
            {executor.title ?? "默认"}
          </span>
        </summary>
        <div className="mt-2">
          <label className="agentdash-form-label">标题</label>
          <input
            value={executor.title ?? ""}
            onChange={(e) => onChange({ ...executor, title: e.target.value || undefined })}
            className="agentdash-form-input"
            placeholder="等待人工审批"
          />
        </div>
      </details>
    </div>
  );
}

// ─── §3 Ports & Policy ──────────────────────────────────

export function PortsAndPolicySection({
  activity,
  contractOutputKeys,
  contractInputKeys,
  onActivityChange,
  onSetCompletionPolicy,
}: {
  activity: ActivityDefinition;
  contractOutputKeys: Set<string>;
  contractInputKeys: Set<string>;
  onActivityChange: (patch: Partial<ActivityDefinition>) => void;
  onSetCompletionPolicy: (policy: ActivityCompletionPolicy) => void;
}) {
  return (
    <section className="space-y-3">
      <SectionTitle>Ports &amp; Policy</SectionTitle>

      <ActivityOutputPortsList
        ports={activity.output_ports}
        contractKeys={contractOutputKeys}
        onChange={(next) => onActivityChange({ output_ports: next })}
      />

      <ActivityInputPortsList
        ports={activity.input_ports}
        contractKeys={contractInputKeys}
        onChange={(next) => onActivityChange({ input_ports: next })}
      />

      <CompletionPolicyEditor
        activity={activity}
        onChange={onSetCompletionPolicy}
      />
    </section>
  );
}

const POLICY_KIND_BY_EXECUTOR: Record<
  ActivityExecutorSpec["kind"],
  ReadonlyArray<ActivityCompletionPolicy["kind"]>
> = {
  agent: ["output_ports", "executor_terminal", "hook_gate", "open_ended"],
  function: ["output_ports", "executor_terminal"],
  human: ["human_decision"],
};

function CompletionPolicyEditor({
  activity,
  onChange,
}: {
  activity: ActivityDefinition;
  onChange: (next: ActivityCompletionPolicy) => void;
}) {
  const policy = activity.completion_policy;
  const allowedKinds = POLICY_KIND_BY_EXECUTOR[activity.executor.kind] ?? [];
  const handleKindChange = (kind: ActivityCompletionPolicy["kind"]) => {
    if (kind === policy.kind) return;
    switch (kind) {
      case "output_ports":
        onChange({ kind, required_ports: activity.output_ports.map((p) => p.key) });
        break;
      case "executor_terminal":
        onChange({ kind });
        break;
      case "human_decision":
        onChange({ kind, decision_port: "decision" });
        break;
      case "hook_gate":
        onChange({ kind, hook_key: "" });
        break;
      case "open_ended":
        onChange({ kind });
        break;
    }
  };

  return (
    <div>
      <label className="agentdash-form-label">Completion Policy</label>
      {allowedKinds.length > 1 ? (
        <select
          value={policy.kind}
          onChange={(e) => handleKindChange(e.target.value as ActivityCompletionPolicy["kind"])}
          className="agentdash-form-select"
        >
          {allowedKinds.map((kind) => (
            <option key={kind} value={kind}>
              {kind}
            </option>
          ))}
        </select>
      ) : (
        <p className="rounded-[8px] border border-dashed border-border bg-secondary/15 px-2 py-1.5 font-mono text-[11px] text-muted-foreground">
          {policy.kind}
        </p>
      )}

      {policy.kind === "output_ports" && (
        <div className="mt-2 space-y-1.5 rounded-[8px] border border-border bg-secondary/20 p-2">
          <p className="text-[11px] text-muted-foreground">勾选必须交付才视为完成</p>
          {activity.output_ports.length === 0 ? (
            <p className="text-[11px] text-muted-foreground">先添加 output ports</p>
          ) : (
            activity.output_ports.map((p) => {
              const checked = policy.required_ports.includes(p.key);
              return (
                <label key={p.key} className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => {
                      const next = checked
                        ? policy.required_ports.filter((k) => k !== p.key)
                        : [...policy.required_ports, p.key];
                      onChange({ kind: "output_ports", required_ports: next });
                    }}
                  />
                  <span className="font-mono">{p.key}</span>
                </label>
              );
            })
          )}
        </div>
      )}

      {policy.kind === "human_decision" && (
        <div className="mt-2">
          <label className="agentdash-form-label">Decision Port</label>
          <input
            value={policy.decision_port}
            list="completion-decision-port-opts"
            onChange={(e) => onChange({ kind: "human_decision", decision_port: e.target.value })}
            className="agentdash-form-input"
            placeholder="decision"
          />
          <datalist id="completion-decision-port-opts">
            {activity.output_ports.map((p) => (
              <option key={p.key} value={p.key} />
            ))}
          </datalist>
        </div>
      )}

      {policy.kind === "hook_gate" && (
        <div className="mt-2">
          <label className="agentdash-form-label">Hook Key</label>
          <input
            value={policy.hook_key}
            onChange={(e) => onChange({ kind: "hook_gate", hook_key: e.target.value })}
            className="agentdash-form-input"
            placeholder="my_hook"
          />
        </div>
      )}
    </div>
  );
}

// ─── Activity ports 列表（contract 标记只读 / extras 可编辑） ─────

function ActivityOutputPortsList({
  ports,
  contractKeys,
  onChange,
}: {
  ports: OutputPortDefinition[];
  contractKeys: Set<string>;
  onChange: (next: OutputPortDefinition[]) => void;
}) {
  const handleAdd = () =>
    onChange([...ports, { key: "", description: "", gate_strategy: "existence" }]);

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className="agentdash-form-label m-0">Output Ports ({ports.length})</label>
        <button
          type="button"
          onClick={handleAdd}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          + 添加
        </button>
      </div>
      <div className="space-y-1.5">
        {ports.length === 0 && (
          <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
        )}
        {ports.map((p, idx) => {
          const isContract = p.key !== "" && contractKeys.has(p.key);
          return (
            <OutputPortItem
              key={idx}
              port={p}
              readOnly={isContract}
              badge={isContract ? "标准" : undefined}
              onChange={
                isContract
                  ? undefined
                  : (next) => {
                      const n = [...ports];
                      n[idx] = next;
                      onChange(n);
                    }
              }
              onRemove={
                isContract ? undefined : () => onChange(ports.filter((_, i) => i !== idx))
              }
            />
          );
        })}
      </div>
    </div>
  );
}

function ActivityInputPortsList({
  ports,
  contractKeys,
  onChange,
}: {
  ports: InputPortDefinition[];
  contractKeys: Set<string>;
  onChange: (next: InputPortDefinition[]) => void;
}) {
  const handleAdd = () =>
    onChange([
      ...ports,
      {
        key: "",
        description: "",
        context_strategy: "full",
        standalone_fulfillment: "required",
      },
    ]);

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className="agentdash-form-label m-0">Input Ports ({ports.length})</label>
        <button
          type="button"
          onClick={handleAdd}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          + 添加
        </button>
      </div>
      <div className="space-y-1.5">
        {ports.length === 0 && (
          <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
        )}
        {ports.map((p, idx) => {
          const isContract = p.key !== "" && contractKeys.has(p.key);
          return (
            <InputPortItem
              key={idx}
              port={p}
              readOnly={isContract}
              badge={isContract ? "标准" : undefined}
              onChange={
                isContract
                  ? undefined
                  : (next) => {
                      const n = [...ports];
                      n[idx] = next;
                      onChange(n);
                    }
              }
              onRemove={
                isContract ? undefined : () => onChange(ports.filter((_, i) => i !== idx))
              }
            />
          );
        })}
      </div>
    </div>
  );
}

// ─── Contract tab 内容 ─────────────────────────────────

export function AgentProcedureContractTabContent({
  procedureDraft,
  hookPresets,
  targetKinds,
  projectId,
  onUpdateInjection,
  onBindingChange,
  onBindingAdd,
  onBindingRemove,
  onAddHookRule,
  onToggleHookRule,
  onRemoveHookRule,
  onDirectivesChange,
  onContractOutputPortsChange,
  onContractInputPortsChange,
}: {
  procedureDraft: AgentProcedureDraft;
  hookPresets: HookRulePreset[];
  targetKinds: WorkflowTargetKind[];
  projectId: string;
  onUpdateInjection: (patch: Partial<WorkflowInjectionSpec>) => void;
  onBindingChange: (idx: number, patch: Partial<WorkflowContextBinding>) => void;
  onBindingAdd: () => void;
  onBindingRemove: (idx: number) => void;
  onAddHookRule: (rule: WorkflowHookRuleSpec) => void;
  onToggleHookRule: (key: string) => void;
  onRemoveHookRule: (key: string) => void;
  onDirectivesChange: (next: CapabilityDirective[]) => void;
  onContractOutputPortsChange: (next: OutputPortDefinition[]) => void;
  onContractInputPortsChange: (next: InputPortDefinition[]) => void;
}) {
  return (
    <div className="space-y-3">
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/15 px-3 py-2">
        <p className="text-[10px] uppercase tracking-wider text-muted-foreground">Procedure Key</p>
        <p className="mt-0.5 truncate font-mono text-xs text-foreground">
          {procedureDraft.key || "(未命名)"}
        </p>
      </div>
      <InjectionPanel
        injection={procedureDraft.contract.injection}
        onGuidanceChange={(guidance) => onUpdateInjection({ guidance: guidance ?? undefined })}
        onBindingChange={onBindingChange}
        onBindingAdd={onBindingAdd}
        onBindingRemove={onBindingRemove}
      />
      <CapabilityPanel
        projectId={projectId}
        targetKinds={targetKinds}
        directives={procedureDraft.contract.capability_config.tool_directives}
        onDirectivesChange={onDirectivesChange}
      />
      <HookRulesPanel
        hookRules={procedureDraft.contract.hook_rules}
        presets={hookPresets}
        onAdd={onAddHookRule}
        onToggle={onToggleHookRule}
        onRemove={onRemoveHookRule}
      />
      <PortsPanel
        outputPorts={procedureDraft.contract.output_ports ?? []}
        inputPorts={procedureDraft.contract.input_ports ?? []}
        onOutputChange={onContractOutputPortsChange}
        onInputChange={onContractInputPortsChange}
      />
    </div>
  );
}

// ─── 辅助 ───────────────────────────────────────────────

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="sticky top-0 -mx-1 bg-background/95 px-1 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground backdrop-blur">
      {children}
    </h3>
  );
}

/**
 * 双层 ports 同步：contract 改后，把改动合并到 activity ports，保留 step-extra
 * （activity 自加的、不在旧 contract 集合里的 port）。
 */
