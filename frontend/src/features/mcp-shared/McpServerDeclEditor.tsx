/**
 * 共享 MCP Server Decl 编辑器。
 *
 * 背景：
 * - `agent-preset-editor.tsx` 的 MCP Server 列表项编辑器（`McpServerEntry`）与
 *   MCP Preset 资产的单条 Server 编辑表单字段 100% 对齐。
 * - 本组件将"单个 McpServerDecl 的编辑 UI"抽取到 `features/mcp-shared/`，
 *   供 `features/project/agent-preset-editor.tsx` 与
 *   `features/assets-panel/categories/McpPresetCategoryPanel.tsx` 复用。
 *
 * 契约：
 * - `value` / `onChange` 标准受控模式
 * - `disabled` 覆盖所有子输入（用于 builtin 只读查看）
 * - `onRemove` 可选：当外层是列表时由列表提供；Preset 单条编辑时不传
 * - Transport 切换 (http / sse / stdio) 时保留同名字段（name / relay），
 *   其他字段置为该 transport 的空默认值（保持 discriminated union narrow 成立）
 *
 * 字段 / 样式完全对齐原 agent-preset-editor，避免视觉迁移。
 */

import type { McpEnvVar, McpHttpHeader, McpServerDecl } from "../../types";

/* ─── KeyValue / String 列表（供 headers / args / env 复用） ─── */

export function KeyValueList({
  items,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
  disabled,
}: {
  items: McpHttpHeader[] | McpEnvVar[];
  onChange: (items: McpHttpHeader[]) => void;
  keyPlaceholder: string;
  valuePlaceholder: string;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item.name}
            onChange={(e) => {
              const next = [...items] as McpHttpHeader[];
              next[i] = { ...next[i], name: e.target.value };
              onChange(next);
            }}
            placeholder={keyPlaceholder}
            disabled={disabled}
            className="agentdash-form-input flex-1"
          />
          <input
            value={item.value}
            onChange={(e) => {
              const next = [...items] as McpHttpHeader[];
              next[i] = { ...next[i], value: e.target.value };
              onChange(next);
            }}
            placeholder={valuePlaceholder}
            disabled={disabled}
            className="agentdash-form-input flex-1"
          />
          {!disabled && (
            <button
              type="button"
              onClick={() => {
                const next = items.filter((_, j) => j !== i) as McpHttpHeader[];
                onChange(next);
              }}
              className="shrink-0 rounded-[6px] border border-destructive/30 px-2 text-xs text-destructive hover:bg-destructive/10"
            >
              ×
            </button>
          )}
        </div>
      ))}
      {!disabled && (
        <button
          type="button"
          onClick={() => onChange([...(items as McpHttpHeader[]), { name: "", value: "" }])}
          className="text-[10px] text-muted-foreground hover:text-foreground"
        >
          + 添加
        </button>
      )}
    </div>
  );
}

export function StringList({
  items,
  onChange,
  placeholder,
  disabled,
}: {
  items: string[];
  onChange: (items: string[]) => void;
  placeholder: string;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item}
            onChange={(e) => {
              const next = [...items];
              next[i] = e.target.value;
              onChange(next);
            }}
            placeholder={placeholder}
            disabled={disabled}
            className="agentdash-form-input flex-1"
          />
          {!disabled && (
            <button
              type="button"
              onClick={() => onChange(items.filter((_, j) => j !== i))}
              className="shrink-0 rounded-[6px] border border-destructive/30 px-2 text-xs text-destructive hover:bg-destructive/10"
            >
              ×
            </button>
          )}
        </div>
      ))}
      {!disabled && (
        <button
          type="button"
          onClick={() => onChange([...items, ""])}
          className="text-[10px] text-muted-foreground hover:text-foreground"
        >
          + 添加
        </button>
      )}
    </div>
  );
}

/* ─── 主编辑器 ─── */

export interface McpServerDeclEditorProps {
  value: McpServerDecl;
  onChange: (next: McpServerDecl) => void;
  /** 可选的删除回调。仅当外层是列表（如 agent preset）时传入；Preset 单条编辑不传。 */
  onRemove?: () => void;
  /** 只读模式：用于 builtin Preset 查看。disabled 会覆盖全部子输入。 */
  disabled?: boolean;
}

export function McpServerDeclEditor({
  value,
  onChange,
  onRemove,
  disabled,
}: McpServerDeclEditorProps) {
  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center gap-2">
        <select
          value={value.type}
          onChange={(e) => {
            // Transport 切换：保留 name 与 relay，其他字段重置为该 transport 的空默认
            const t = e.target.value as McpServerDecl["type"];
            const preservedName = value.name;
            const preservedRelay = value.relay;
            if (t === "stdio") {
              onChange({
                type: "stdio",
                name: preservedName,
                command: "",
                args: [],
                env: [],
                ...(preservedRelay !== undefined ? { relay: preservedRelay } : {}),
              });
            } else {
              onChange({
                type: t,
                name: preservedName,
                url: "",
                headers: [],
                ...(preservedRelay !== undefined ? { relay: preservedRelay } : {}),
              });
            }
          }}
          disabled={disabled}
          className="agentdash-form-select w-24"
        >
          <option value="http">HTTP</option>
          <option value="sse">SSE</option>
          <option value="stdio">Stdio</option>
        </select>
        <input
          value={value.name}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
          placeholder="服务名称"
          disabled={disabled}
          className="agentdash-form-input flex-1"
        />
        <label className="flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
          <input
            type="checkbox"
            checked={value.relay ?? value.type === "stdio"}
            onChange={(e) => onChange({ ...value, relay: e.target.checked })}
            disabled={disabled}
            className="h-3 w-3"
          />
          Relay
        </label>
        {onRemove && !disabled && (
          <button
            type="button"
            onClick={onRemove}
            className="shrink-0 rounded-[6px] border border-destructive/30 px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
          >
            删除
          </button>
        )}
      </div>

      {(value.type === "http" || value.type === "sse") && (
        <>
          <div>
            <label className="agentdash-form-label">URL</label>
            <input
              value={value.url}
              onChange={(e) => onChange({ ...value, url: e.target.value })}
              placeholder="https://example.com/mcp"
              disabled={disabled}
              className="agentdash-form-input"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Headers</label>
            <KeyValueList
              items={value.headers ?? []}
              onChange={(h) => onChange({ ...value, headers: h })}
              keyPlaceholder="Header 名称"
              valuePlaceholder="值"
              disabled={disabled}
            />
          </div>
        </>
      )}

      {value.type === "stdio" && (
        <>
          <div>
            <label className="agentdash-form-label">Command</label>
            <input
              value={value.command}
              onChange={(e) => onChange({ ...value, command: e.target.value })}
              placeholder="npx / python / /path/to/binary"
              disabled={disabled}
              className="agentdash-form-input"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Args</label>
            <StringList
              items={value.args ?? []}
              onChange={(a) => onChange({ ...value, args: a })}
              placeholder="参数"
              disabled={disabled}
            />
          </div>
          <div>
            <label className="agentdash-form-label">Env</label>
            <KeyValueList
              items={(value.env ?? []) as McpHttpHeader[]}
              onChange={(e) => onChange({ ...value, env: e as McpEnvVar[] })}
              keyPlaceholder="变量名"
              valuePlaceholder="值"
              disabled={disabled}
            />
          </div>
        </>
      )}
    </div>
  );
}
