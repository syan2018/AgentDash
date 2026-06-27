/**
 * 共享 MCP transport 编辑器。
 *
 * 只负责 transport 连接参数，不承载 server name 或 route policy 语义。
 * 在 views 包中定义以便 local-runtime / app-web 等消费方共享。
 */

export interface McpTransportConfigEditorEntry {
  name: string
  value: string
}

export type McpTransportConfigEditorValue =
  | { type: 'http'; url: string; headers?: McpTransportConfigEditorEntry[] }
  | { type: 'sse'; url: string; headers?: McpTransportConfigEditorEntry[] }
  | {
      type: 'stdio'
      command: string
      args?: string[]
      env?: McpTransportConfigEditorEntry[]
      cwd?: string
    }

// ── Key-Value 列表 ──

export function KeyValueList({
  items,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
  disabled,
}: {
  items: McpTransportConfigEditorEntry[]
  onChange: (items: McpTransportConfigEditorEntry[]) => void
  keyPlaceholder: string
  valuePlaceholder: string
  disabled?: boolean
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item.name}
            onChange={(e) => {
              const next = [...items]
              next[i] = { ...next[i], name: e.target.value }
              onChange(next)
            }}
            placeholder={keyPlaceholder}
            disabled={disabled}
            className="agentdash-form-input flex-1"
          />
          <input
            value={item.value}
            onChange={(e) => {
              const next = [...items]
              next[i] = { ...next[i], value: e.target.value }
              onChange(next)
            }}
            placeholder={valuePlaceholder}
            disabled={disabled}
            className="agentdash-form-input flex-1"
          />
          {!disabled && (
            <button
              type="button"
              onClick={() => {
                const next = items.filter((_, j) => j !== i)
                onChange(next)
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
          onClick={() => onChange([...items, { name: '', value: '' }])}
          className="text-[10px] text-muted-foreground hover:text-foreground"
        >
          + 添加
        </button>
      )}
    </div>
  )
}

// ── 字符串列表 ──

export function StringList({
  items,
  onChange,
  placeholder,
  disabled,
}: {
  items: string[]
  onChange: (items: string[]) => void
  placeholder: string
  disabled?: boolean
}) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex gap-1.5">
          <input
            value={item}
            onChange={(e) => {
              const next = [...items]
              next[i] = e.target.value
              onChange(next)
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
          onClick={() => onChange([...items, ''])}
          className="text-[10px] text-muted-foreground hover:text-foreground"
        >
          + 添加
        </button>
      )}
    </div>
  )
}

// ── Transport 编辑器 ──

export interface McpTransportConfigEditorProps {
  value: McpTransportConfigEditorValue
  onChange: (next: McpTransportConfigEditorValue) => void
  disabled?: boolean
}

export function McpTransportConfigEditor({
  value,
  onChange,
  disabled,
}: McpTransportConfigEditorProps) {
  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div>
        <label className="agentdash-form-label">Transport</label>
        <select
          value={value.type}
          onChange={(e) => {
            const nextType = e.target.value
            if (nextType === 'stdio') {
              onChange({ type: 'stdio', command: '', args: [], env: [], cwd: '' })
            } else if (nextType === 'http' || nextType === 'sse') {
              onChange({ type: nextType, url: '', headers: [] })
            }
          }}
          disabled={disabled}
          className="agentdash-form-select w-full"
        >
          <option value="stdio">Stdio (本地进程)</option>
          <option value="http">HTTP (Streamable)</option>
          <option value="sse">SSE (Server-Sent Events)</option>
        </select>
      </div>

      {(value.type === 'http' || value.type === 'sse') && (
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

      {value.type === 'stdio' && (
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
            <label className="agentdash-form-label">CWD</label>
            <input
              value={value.cwd ?? ''}
              onChange={(e) => onChange({ ...value, cwd: e.target.value })}
              placeholder="/workspace/project"
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
              items={value.env ?? []}
              onChange={(env) => onChange({ ...value, env })}
              keyPlaceholder="变量名"
              valuePlaceholder="值"
              disabled={disabled}
            />
          </div>
        </>
      )}
    </div>
  )
}
