# @引用工作空间文件（ACP 官方方案对齐）

## 背景与目标

当前需求是：在 Session 输入框支持 `@` 引用文件，把文件上下文传给 Agent。

本 PRD 改为严格对齐 ACP 官方能力，避免自定义协议字段导致后续远端 ACP/多 Agent 接入时返工。

ACP 对齐结论：
- 用户消息应通过 `session/prompt` 的 `ContentBlock[]` 发送。
- 文件上下文优先使用 `ContentBlock::Resource`（embedded context）。
- 所有 Agent 基线支持 `ContentBlock::ResourceLink`。
- Agent 侧按需读取文件走 `fs/read_text_file`（能力协商后可用）。

## 目标

1. 在 Prompt 输入框支持 `@` 触发文件选择。
2. 将文件引用转换为 ACP 标准 `ContentBlock`（`resource` / `resource_link`）。
3. 构建后端“通用资源暴露能力”，统一服务于：
   - UI 文件选择
   - Prompt 上下文组装
   - ACP `fs/read_text_file` 请求处理
4. 保持现有 Session 页面交互体验（可视化引用、删除、搜索、键盘选择）。

## 非目标

- 不实现智能文件推荐。
- 不实现跨工作空间引用。
- 不实现文件夹递归注入。
- 不在本期实现复杂上下文压缩算法（只做大小限制与策略降级）。

## 官方方案映射（必须遵守）

| 需求场景 | ACP 标准能力 | 说明 |
|---|---|---|
| 用户在输入框 `@` 引用文件并发送上下文 | `session/prompt` + `ContentBlock::Resource` | 首选方案，直接嵌入文本内容 |
| Agent 已知资源位置但自行读取 | `ContentBlock::ResourceLink` + `fs/read_text_file` | `resource_link` 为基线支持，读取能力需协商 |
| 获取文件列表用于文件选择器 | 非 ACP 标准（项目内部 API） | 保留 `workspace` 维度的内部接口 |

## 当前状态分析

### 已有基础

- Session 页面已有 Prompt 输入框。
- 前端已使用 ACP 类型（`@agentclientprotocol/sdk`）。
- 后端已有工作空间文件管理 API（可用于文件选择器）。
- 后端会话流已是 ACP `SessionNotification` 渲染链路。

### 需要补齐

- Prompt 入参仍是 `prompt: string`，未支持 `ContentBlock[]`。
- `@` 解析后尚未生成 ACP `resource/resource_link`。
- 缺少能力协商后的发送策略（`embeddedContext` / fallback）。
- 缺少可复用的后端资源暴露层（目前仅有散点文件读取诉求）。

## 需求规格

### FR-1: `@` 触发文件选择

在 Prompt 输入框输入 `@` 时打开文件选择浮层。

交互要求：
- 支持实时搜索过滤。
- 支持方向键上下选择与 Enter 确认。
- 支持 ESC/点击外部关闭。
- 支持路径前缀补全（例如 `@src/comp`）。

### FR-2: 引用语法仅作为 UI 输入语法

输入中的 `@path/to/file` 仅作为编辑器友好语法，不作为网络协议字段。

发送前前端必须转换为 ACP `ContentBlock[]`。

示例：

```text
请帮我分析 @src/main.rs 的错误处理
```

转换（概念）：

```typescript
[
  { type: 'text', text: '请帮我分析 src/main.rs 的错误处理' },
  {
    type: 'resource',
    resource: {
      uri: 'file:///abs/workspace/src/main.rs',
      mimeType: 'text/x-rust',
      text: '...文件内容...'
    }
  }
]
```

### FR-3: Prompt API 对齐 ACP 内容模型

新增/升级请求结构，支持标准内容块：

```typescript
interface PromptSessionRequest {
  promptBlocks?: ContentBlock[]; // 新：ACP 标准
  prompt?: string;               // 兼容旧版本，后端会转为单 text block
  workingDir?: string;
  env?: Record<string, string>;
  executorConfig?: ExecutorConfig;
}
```

兼容规则：
- `promptBlocks` 存在时优先使用。
- 仅有 `prompt` 时，后端转换为 `[TextContent]`。
- 二者同时存在时，返回 400（避免语义冲突）。

### FR-4: 能力协商与发送策略

必须依据 ACP 能力协商结果选择发送策略：

1. Agent `promptCapabilities.embeddedContext = true`：
   - 使用 `ContentBlock::Resource`（优先）。
2. Agent 不支持 `embeddedContext`：
   - 使用 `ContentBlock::ResourceLink`。
3. Agent 需要读取文件正文时：
   - 通过 `fs/read_text_file` 向 Client 请求（需 `clientCapabilities.fs.readTextFile = true`）。
4. 若以上能力均不可用：
   - 降级为纯文本提示，并告警“当前连接不支持结构化文件上下文”。

### FR-5: 文件选择器继续使用内部 API（非 ACP）

保留工作空间文件列表接口，明确为内部能力：

```http
GET /workspaces/{workspace_id}/files?pattern=*
```

说明：
- 该接口仅用于 UI 选择器，不属于 ACP 协议本身。
- 返回相对路径；发送前由后端统一解析为绝对路径与 `file://` URI。

### FR-6: 后端通用资源暴露能力（核心）

新增独立模块：`Resource Exposure`，统一管理“可被 Agent/Prompt 使用的资源”。

```rust
pub struct ResourceHandle {
    pub workspace_id: String,
    pub rel_path: String,
    pub abs_path: std::path::PathBuf,
    pub uri: String, // file:///...
    pub mime_type: Option<String>,
    pub size: u64,
}

#[async_trait::async_trait]
pub trait ResourceProvider: Send + Sync {
    async fn list(&self, workspace_id: &str, pattern: Option<&str>) -> Result<Vec<ResourceHandle>, ResourceError>;
    async fn resolve(&self, workspace_id: &str, rel_path: &str) -> Result<ResourceHandle, ResourceError>;
    async fn read_text(&self, handle: &ResourceHandle, line: Option<u32>, limit: Option<u32>) -> Result<String, ResourceError>;
}
```

该模块统一服务三类调用：
- UI 文件选择器（`list`）。
- Prompt `resource` 组装（`resolve + read_text`）。
- ACP `fs/read_text_file`（`resolve/read_text` 的协议适配层）。

### FR-7: 安全与限制

限制策略统一由 `Resource Exposure` 实施：

| 限制项 | 值 | 说明 |
|---|---:|---|
| 单文件大小 | 100KB | 超限默认转 `resource_link` 或拒绝嵌入 |
| 总嵌入大小 | 500KB | 超限报错并提示分批引用 |
| 单次引用数 | 10 | 防止请求爆炸 |

安全要求：
- 严格防止路径穿越（`../`、符号链接逃逸）。
- 仅允许工作空间白名单目录。
- 非文本内容默认不嵌入 `resource.text`，仅可生成 `resource_link`。

### FR-8: 引用可视化反馈

输入框下方展示已引用文件，支持删除。

删除行为：
- 移除对应 `@path` 标记。
- 移除对应待发送的 `ContentBlock::Resource/ResourceLink`。

## 后端实现规划（通用能力）

### Phase 1: 资源暴露模块落地（基础能力）

目标：落地 `ResourceProvider` 与安全解析。

交付：
- 新建 `crates/agentdash-resource/`（或并入 executor 子模块）。
- 实现 `list/resolve/read_text`。
- 提供 MIME 推断、大小限制、路径安全校验。
- 为现有 `/workspaces/{id}/files` 复用同一 provider，避免重复逻辑。

### Phase 2: Prompt 入参升级与兼容

目标：让 API/Hub 支持 `ContentBlock[]`。

交付：
- `PromptSessionRequest` 增加 `promptBlocks`。
- `ExecutorHub::start_prompt` 支持 block 模式。
- 旧 `prompt: string` 自动转换为 text block。
- 增加请求校验与错误矩阵（仅 `prompt`、仅 `promptBlocks`、冲突输入、空输入）。

### Phase 3: 连接器适配（统一 PromptPayload）

目标：把连接器入参从纯字符串演进为可扩展 payload。

建议接口：

```rust
pub enum PromptPayload {
    Text(String),
    Blocks(Vec<agent_client_protocol::ContentBlock>),
}
```

落地策略：
- `AgentConnector::prompt` 改为接收 `PromptPayload`。
- 本地执行器 connector 可先做“Blocks -> 文本拼接”过渡适配。
- 远程 ACP connector 直接透传为 `session/prompt` 标准结构。

### Phase 4: ACP 文件系统桥接（通用暴露能力对外化）

目标：支持 Agent 发起 `fs/read_text_file`。

交付：
- 在 ACP 会话桥接层处理 `fs/read_text_file` 请求。
- 路由到 `ResourceProvider::read_text`。
- 严格校验 `clientCapabilities.fs.readTextFile`。
- 输出标准 ACP 响应错误码（例如 not_found、invalid_params、access_denied）。

### Phase 5: 可观测性与治理

目标：让该能力可控可审计。

交付：
- 记录资源读取审计日志（sessionId、path、line/limit、结果大小）。
- 增加指标：引用次数、嵌入总字节、降级次数、拒绝次数。
- 增加熔断阈值（超大引用或高频读取）。

## 前端实现要点

- 新增 `useFileReference`：负责 `@` 解析、引用状态管理、删除同步。
- 新增 `buildPromptBlocks()`：根据协商能力输出 `resource` 或 `resource_link`。
- 发送前调用后端 `resolve`（或后端在 prompt 入口统一 resolve）。
- UI 上保持 `@path` 可见，协议上只发送标准 `ContentBlock[]`。

## 错误矩阵（最小集）

| 场景 | HTTP | 提示 |
|---|---:|---|
| `prompt` 与 `promptBlocks` 同时传入 | 400 | 请求参数冲突 |
| 引用文件不存在 | 404 | 文件不存在或已移动 |
| 路径越界 | 403 | 禁止访问工作空间外文件 |
| 单文件/总大小超限 | 413 | 文件过大，请减少引用 |
| 不支持 embedded context 且无 readTextFile | 422 | 当前连接不支持文件上下文 |

## 验收标准

- [ ] 输入 `@` 弹出文件选择浮层，支持搜索与键盘选择。
- [ ] 发送请求使用 ACP 标准 `ContentBlock[]`（非自定义 `file_references`）。
- [ ] 能根据协商能力在 `resource` 与 `resource_link` 之间正确切换。
- [ ] 后端存在可复用的 `ResourceProvider`，并被文件选择与 prompt 注入复用。
- [ ] 路径穿越攻击被阻止。
- [ ] 文件大小与数量限制生效，并给出清晰错误。
- [ ] 兼容旧 `prompt: string` 调用路径。

## 依赖与风险

### 依赖

- 远程 ACP connector 完整实现（当前仍为骨架）。
- 会话初始化阶段能力协商信息可被持久化或缓存。

### 风险

- R1: 连接器仍以纯文本 prompt 运行，短期内 block 能力只能“降级拼接”。
- R2: 文件 URI 与 workspace 相对路径映射不一致导致引用错位。
- R3: 嵌入上下文导致 token 预算不可控。

缓解：
- 分阶段落地，先统一 provider 与 payload，再切远程透传。
- 将 path->URI 映射逻辑集中在后端单点实现。
- 强制大小限制并暴露监控指标。

## 参考

- ACP Content: https://agentclientprotocol.com/protocol/content
- ACP Prompt Turn: https://agentclientprotocol.com/protocol/prompt-turn
- ACP Initialization: https://agentclientprotocol.com/protocol/initialization
- ACP File System: https://agentclientprotocol.com/protocol/file-system
- 本地协议副本：`third_party/agent-client-protocol/schema/schema.json`
- 本地任务：`.trellis/tasks/02-28-context-injection-design/prd.md`
