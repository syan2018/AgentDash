/**
 * 共享 MCP transport 编辑器。
 *
 * 只负责 transport 连接参数，不再承载 server name 或 route policy 语义。
 */

import type { McpEnvVar, McpHttpHeader, McpTransportConfig } from "../../types";

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

export interface McpTransportConfigEditorProps {
  value: McpTransportConfig;
  onChange: (next: McpTransportConfig) => void;
  disabled?: boolean;
}

export function McpTransportConfigEditor({
  value,
  onChange,
  disabled,
}: McpTransportConfigEditorProps) {
  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center gap-2">
        <select
          value={value.type}
          onChange={(e) => {
            const t = e.target.value as McpTransportConfig["type"];
            if (t === "stdio") {
              onChange({
                type: "stdio",
                command: "",
                args: [],
                env: [],
              });
            } else {
              onChange({
                type: t,
                url: "",
                headers: [],
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
        <span className="text-xs text-muted-foreground">
          这里只配置 transport 参数；工具标识和路由策略在外层单独配置
        </span>
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
