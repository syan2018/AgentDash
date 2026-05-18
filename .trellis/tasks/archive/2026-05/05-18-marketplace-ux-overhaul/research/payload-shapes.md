# LibraryAssetDto.payload 形状取样

权威来源：`crates/agentdash-domain/src/shared_library/value_objects.rs`
（serde tag/content 编码：`{ asset_type: ..., payload: ... }`，但 API DTO 已 flatten，
前端 `LibraryAssetDto.payload` 直接对应下面的 inner shape）

## agent_template

```ts
interface AgentTemplatePayload {
  config: {
    executor?: string;
    provider_id?: string;
    model_id?: string;
    agent_id?: string;
    thinking_level?: ThinkingLevel;
    permission_policy?: string;
    system_prompt?: string;
    system_prompt_mode?: SystemPromptMode;
    capability_directives?: ToolCapabilityDirective[];
    mcp_slots?: Array<{ key: string; description?: string; required?: boolean }>;
  };
}
```

抽屉展示字段：`config.model_id`、`config.executor`、`config.system_prompt`（截断 200 字）、`mcp_slots.length`、`capability_directives.length`。

## mcp_server_template

```ts
interface McpServerTemplatePayload {
  transport: McpTransportConfig;        // 已有 TS 类型
  route_policy?: McpRoutePolicy;
  parameter_schema?: unknown;
  capabilities?: string[];
}
```

抽屉展示字段：`transport.type` + 关键 field（http/sse 的 url，stdio 的 command），
`route_policy`，`capabilities` 作为 chip 列表。**不主动 probe**（与 PRD 一致）。

## workflow_template

```ts
// payload 外层
interface WorkflowTemplatePayload {
  template: BuiltinWorkflowTemplateBundle;     // 见下
  schema_version?: string;
}

interface BuiltinWorkflowTemplateBundle {
  key: string;
  name: string;
  description: string;
  binding_kinds: WorkflowBindingKind[];
  workflows: BuiltinWorkflowTemplate[];        // 子工作流
  lifecycle: {
    key: string;
    name: string;
    description: string;
    entry_step_key: string;
    steps: LifecycleStepDefinition[];
    edges: LifecycleEdge[];
  };
}

interface BuiltinWorkflowTemplate {
  key: string;
  name: string;
  description: string;
  contract: WorkflowContract;                  // 不展开
}
```

抽屉展示字段：`template.lifecycle.steps.length`、`template.lifecycle.edges.length`、
`template.workflows.length`、`template.binding_kinds`，附 step 名列表
（key + name，最多展示 8 项后折叠）。

## skill_template

```ts
interface SkillTemplatePayload {
  files: Array<{
    path: string;
    content: string;
    kind: SkillAssetFileKind;        // primary / extra / etc.
  }>;
  disable_model_invocation?: boolean;
}
```

抽屉展示字段：`files.length`、文件列表（path + 内容大小）+ `disable_model_invocation`
chip + `SKILL.md` 摘要（找 `kind === "primary"` 或 `path.endsWith("SKILL.md")` 的 entry，
取 frontmatter 后前 200 字）。

## fallback 解析策略

每个 type-specific body 的解析流程：

```ts
function parseSkillPayload(raw: unknown): SkillPayload | null {
  if (!isObject(raw)) return null;
  const files = (raw as any).files;
  if (!Array.isArray(files)) return null;
  // 仅校验关键字段，多余字段宽容
  return {
    files: files.filter(isObject).map(/* ... */),
    disable_model_invocation: Boolean((raw as any).disable_model_invocation),
  };
}
```

解析失败 → 降级到 `<RawPayloadFallback>`（折叠 JSON），永不 throw。

## 已验证

- [x] 四类 type 的 Rust struct 与 serde 字段名（`#[serde(rename_all = "snake_case")]`
      不存在于 these structs；字段名直接是 `model_id` / `system_prompt` 等 snake_case）
- [x] `LibraryAssetPayload::from_value` 用 `serde_json::from_value`，零裁剪
- [x] DTO 层 [crates/agentdash-api/src/dto/shared_library.rs](../../../../crates/agentdash-api/src/dto/shared_library.rs) 的 payload 字段
      直接透传 `serde_json::Value`，不重新结构化（已通过 grep 确认）
