# Marketplace 用户发布配置资产 — Design

## Architecture

发布流程作为 Shared Library 的应用层能力实现，入口从 Project 资源出发：

```text
Project Asset UI
  -> POST /api/projects/{project_id}/shared-library/publish
  -> application/shared_library/publish.rs
  -> load Project resource by asset_kind + id
  -> typed mapper builds LibraryAsset payload
  -> LibraryAsset::new validates payload
  -> create / update library_assets
```

前端不直接构造 `LibraryAsset.payload`，只提交发布目标、元数据和覆盖策略。后端负责读取当前 Project 资源的权威状态。

## API Shape

新增：

```text
POST /api/projects/{project_id}/shared-library/publish
```

请求草案：

```jsonc
{
  "asset_kind": "project_agent | mcp_preset | workflow_bundle | skill_asset",
  "project_asset_id": "uuid",
  "scope": "user",
  "key": "review_agent",
  "display_name": "Review Agent",
  "description": "代码审阅助手",
  "version": "1.0.0",
  "overwrite": false
}
```

响应返回 `LibraryAssetResponse`，便于前端发布后打开详情或刷新 Marketplace。

## Asset Mapping

### Project Agent -> AgentTemplate

来源：

- `Agent.base_config`
- `ProjectAgentLink.config_override`
- Project link 元数据

第一版建议发布“当前 Project Agent 的可复用 baseline”，不是 session 临时状态。mapper 将可跨 Project 复用的字段写入 `AgentTemplateConfig`：

- executor / provider / model / thinking / permission
- system prompt / prompt mode
- capability directives
- abstract mcp slots

不得发布 Project 私有资源 key、workspace、story/task 默认标记、knowledge/memory、companion whitelist。

### MCP Preset -> McpServerTemplate

mapper 只允许 template-safe transport 和 route policy。需要新增 sanitizer / validator：

- 明确拒绝 secret、token、credential 字段。
- 对本地 stdio command/env/path 做安全判断；无法证明可共享时拒绝发布。
- 保留 parameter_schema 和 capability description。

如果当前 `McpPreset` 结构无法区分 template 和 connection material，本任务应先收窄可发布范围，返回明确错误，避免静默泄露。

### Workflow Bundle -> WorkflowTemplate

发布单位是 lifecycle + 其引用的 workflow definitions。

mapper 应：

- 从 lifecycle steps 收集 workflow references。
- 校验所有引用 workflow 都属于同一 Project 且存在。
- 构造 `BuiltinWorkflowTemplateBundle` 兼容 payload。
- 使用新 key 或发布 key 作为 bundle key，避免安装时残留旧 Project id。

### SkillAsset -> SkillTemplate

mapper 读取 `SkillAsset.files` 并转换为 `SkillTemplateFilePayload`，保留 `disable_model_invocation`。

## Identity And Versioning

`LibraryAsset` identity 继续使用：

```text
asset_type + scope + owner_id + key
```

发布到 user scope 时：

- `owner_id = current_user.user_id`
- `source = user_authored`
- `source_ref = user:{user_id}:{asset_type}:{key}`

`payload_digest` 使用现有 seed digest 规则，基于 payload JSON 计算 sha256。

覆盖发布时保留原 asset id 与 created_at，更新 payload、version、digest、description、updated_at。

## UI

Project Assets 四类面板增加资源动作：

- Agent 卡片 / 配置详情：发布到资源市场
- MCP Preset 卡片 / 详情：发布到资源市场
- Workflow/Lifecycle 详情：发布 bundle 到资源市场
- SkillAsset 卡片 / 详情：发布到资源市场

发布弹窗字段：

- key
- display name
- description
- version
- overwrite checkbox（仅冲突后或检测到已有 asset 时展示）

Marketplace 可新增 `scope/source` 过滤，但不改变现有安装主流程。

## Error Matrix

- Project 不存在或无 edit 权限 -> 403 / 404
- Project 资源不存在 -> 404
- key / display_name / version 为空 -> 400
- 同 identity 已存在且 overwrite=false -> 409
- MCP 含不可发布 connection material -> 400，返回具体字段说明
- Workflow lifecycle 引用缺失 workflow -> 400
- payload 与 asset_type schema 不匹配 -> 400

## Tests

后端：

- publish AgentTemplate roundtrip。
- publish SkillTemplate files roundtrip。
- publish MCP rejects secret/local-only unsafe fields。
- publish Workflow bundle keeps lifecycle/workflow consistency。
- overwrite updates digest and source-status becomes update_available。
- permission check for Project edit。

前端：

- sharedLibrary service 新增 publish API mapper。
- 发布弹窗字段校验。
- Marketplace 刷新后可见 user-authored asset。
