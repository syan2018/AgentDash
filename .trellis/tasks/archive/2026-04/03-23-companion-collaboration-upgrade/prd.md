# 统一 Companion 信道：双向协作模型

## 背景

当前 companion 协作是单向的 dispatch→complete→resolve 链路，涉及三个独立工具：

- `companion_dispatch`：父 agent 向子 agent 派发任务
- `companion_complete`：子 agent 向父 agent 回传结果
- `resolve_hook_action`：父 agent 结案 pending action

问题：

1. **方向单一** — 只支持"父→子"派发，子 agent 无法主动向父 agent 提审
2. **人被排除在外** — 人只能旁观（hook runtime 面板）或做 tool 级审批（Ask），没有进入 companion 协议
3. **工具分裂** — `companion_complete` 和 `resolve_hook_action` 本质上都是"回应请求"，却是两套接口
4. **同步等待缺失** — `wait_for_completion=true` 被显式拒绝，没有 pause/resume 语义

## Goal

把 companion 从"agent 派发子任务的工具"升级为**双向协作信道**：

- Agent ↔ Agent（父子关系）
- Agent ↔ 人（提审/审批关系）

所有交互走统一的 request/respond 协议，复用同一条 hook 通道。

## 核心设计：两个工具 + payload 自由结构

设计原则：**工具参数只表达核心语义（谁、是否等待、回应哪个请求），所有场景特化的内容放进 `payload`。** payload 通过 `type` 鉴别器实现模块化校验、UI 渲染和回复约束。新增交互模式只需注册新的 payload type，工具 schema 不变。

### `companion_request`

向任意方发起请求。

```jsonc
{
  "target": "sub" | "parent" | "human",   // 核心：发给谁
  "wait": false,                           // 核心：是否暂停等回应
  "payload": {
    "type": "...",                          // 鉴别器：决定校验规则、UI 组件、回复约束
    ...                                     // type 特定字段
  }
}
```

返回值：`request_id`（后续 respond 时引用）。

### `companion_respond`

回应任意方的请求。

```jsonc
{
  "request_id": "req-abc-123",             // 核心：回应哪个请求
  "payload": {
    "type": "...",                          // 鉴别器：必须匹配 request 的 response_schema 约束
    ...                                     // type 特定字段
  }
}
```

### 迁移策略

现有 `companion_dispatch` / `companion_complete` / `resolve_hook_action` 三个工具**直接删除**，不保留别名。新工具完全替代旧工具：

| 删除的工具 | 替代为 |
|----------|-------------|
| `companion_dispatch` | `companion_request(target: sub, payload: {type: task, ...})` |
| `companion_complete` | `companion_respond(request_id, payload: {type: completion, final: true, ...})` |
| `resolve_hook_action` | `companion_respond(request_id, payload: {type: resolution, ...})` |

---

## Payload Type 系统

### 设计

每个 payload type 是一个注册模块，定义三件事：

1. **request_schema** — request payload 中该 type 需要哪些字段、格式约束
2. **response_schema** — 对应 respond payload 应满足的结构约束
3. **ui_hint** — 前端应使用哪种 UI 组件渲染

request 校验发生在 `companion_request` 执行时。response 校验发生在 `companion_respond` 执行时 — 系统从 pending action 上读取原始 request 的 type 和 response_schema，校验 respond payload 是否合规。

### 内置 Payload Types

#### `task` — 派发子任务

```jsonc
// request payload
{
  "type": "task",
  "prompt": "审阅代码安全性",            // 必填
  "label": "reviewer",                   // 可选，companion 实例标识
  "context_mode": "compact"              // 可选，compact | full | workflow_only | constraints_only
}
// response_schema 约束：respond payload.type 必须为 completion
// ui_hint: task_dispatch_card
```

#### `completion` — 任务完成回传

```jsonc
// respond payload（对应 task request）
{
  "type": "completion",
  "status": "completed" | "blocked" | "needs_follow_up",
  "summary": "认证模块已完成",            // 必填
  "final": true,                          // true=信道终结
  "findings": ["..."],                    // 可选
  "follow_ups": ["..."]                   // 可选
}
// ui_hint: completion_card
```

#### `review` — 提审/审阅

```jsonc
// request payload
{
  "type": "review",
  "prompt": "JWT 还是 session-cookie？推荐 JWT",   // 必填
  "context": "..."                                   // 可选，补充上下文
}
// response_schema 约束：respond payload.type 必须为 resolution
// ui_hint: review_card
```

#### `resolution` — 审阅结论

```jsonc
// respond payload（对应 review request）
{
  "type": "resolution",
  "status": "approved" | "rejected" | "needs_revision",   // 必填
  "summary": "用 JWT，加上 refresh token",                  // 必填
  "follow_ups": ["..."]                                     // 可选
}
// ui_hint: resolution_badge
```

#### `approval` — 二元审批

```jsonc
// request payload
{
  "type": "approval",
  "prompt": "要删 email 列，确认吗？",                      // 必填
  "options": ["确认删除", "保留该列"]                         // 可选，有则前端渲染为按钮
}
// response_schema 约束：respond payload.type 必须为 decision
// ui_hint: approval_card（有 options 时渲染按钮组）
```

#### `decision` — 审批决定

```jsonc
// respond payload（对应 approval request）
{
  "type": "decision",
  "choice": "保留该列",                                      // 必填（若有 options，必须命中其一）
  "note": "..."                                              // 可选
}
// ui_hint: decision_badge
```

#### `notification` — 单向通知（无需回复）

```jsonc
// request payload
{
  "type": "notification",
  "message": "migration 已生成，等你确认后再执行"             // 必填
}
// response_schema: null（不期望回复）
// ui_hint: notification_toast
```

### 校验流程

```
companion_request 执行时：
  1. 读 payload.type
  2. 查 PayloadTypeRegistry 拿到该 type 的 request_schema
  3. 校验 request payload → 失败则返回工具错误，告知缺了什么
  4. 生成 pending action，附带 response_schema 和 ui_hint

companion_respond 执行时：
  1. 查 pending action，读出原始 request 的 response_schema
  2. 读 respond payload.type
  3. 校验 respond payload 是否满足 response_schema → 失败则返回工具错误
  4. 校验通过 → 走正常 respond 流程
```

### Hook 尾部携带回复约束

pending action 创建时，response_schema 写入 action 元数据：

```rust
HookPendingAction {
    // ... 现有字段 ...
    payload_type: String,                          // "review" | "approval" | "task" | ...
    request_payload: serde_json::Value,            // 原始 request payload
    response_schema: Option<ResponseSchemaRef>,    // 回复约束引用
    ui_hint: String,                               // 前端 UI 组件提示
}
```

hook 注入消息在结尾附带回复要求，引导 agent 正确填写 respond payload：

```
[待处理 Companion 事项]
JWT 还是 session-cookie？推荐 JWT（type=review，status=pending）

事项 id: req-abc-123

请直接处理这项提审，并在完成后调用 companion_respond 结案。
回复要求：payload.type 必须为 resolution，必填 status 和 summary。
```

### PayloadTypeRegistry 归属

Registry 放在 `agentdash-application` 层，不下沉到 `agentdash-spi`。理由：payload type 是项目专属的业务约定，不需要跨 crate 共享，放应用层保持简单。

### 扩展方式

新增 payload type 只需：

1. 在 `agentdash-application` 中定义 request_schema + response_schema + ui_hint
2. 注册到 PayloadTypeRegistry
3. 前端实现对应 ui_hint 的渲染组件

工具 schema、hook 通道、pending action 结构均不变。

#### target + wait 组合语义

| target | wait | 语义 | 映射到现有机制 |
|--------|------|------|---------------|
| `sub` | `false` | 派发子任务，我继续干活 | 当前 `companion_dispatch` |
| `sub` | `true` | 派发子任务，等结果再继续 | 当前缺失的 `wait_for_completion` |
| `parent` | `true` | 向上提审，暂停等批复 | **新能力** |
| `parent` | `false` | 向上报告/通知，不暂停 | 当前 `companion_complete` 的异步变体 |
| `human` | `true` | 问用户，暂停等回复 | 当前 `Ask` 的泛化版 |
| `human` | `false` | 通知用户，不暂停 | **新能力** |

### `final` 语义

- respond payload 中无 `final` 或 `final: false` — 回应了某个具体问题，但 companion 关系还在（如：子 agent 中途提问 → 父 agent 回答 → 子 agent 继续）
- `final: true` — 整个任务完成，信道可以关闭（如：子 agent 做完 review → 父 agent 收到最终结果）
- `final` 只在 `completion` 类型的 respond payload 中有语义，其他 type 忽略

## 人的响应通道

人不调工具，人在 UI 上操作。人的 respond 走 API endpoint，payload 结构与 agent 的 `companion_respond` 一致，同样受 response_schema 校验：

```
POST /api/sessions/{id}/companion-requests/{request_id}/respond
{
  "payload": {
    "type": "decision",
    "choice": "保留该列",
    "note": "可以，注意加上日志"
  }
}
```

前端根据 pending action 的 `ui_hint` 渲染对应组件：
- `approval_card` → 按钮组（options 映射为按钮）
- `review_card` → 文本框 + status 下拉
- `notification_toast` → 仅展示，无需回复

这和当前 `POST /api/sessions/{id}/tool-approvals/{tool_call_id}/approve` 同构。长期可考虑合并 — tool approval 本质上是 `companion_request(target: human, payload: {type: approval}) → 人 companion_respond` 的特例。

## `wait` 阻塞模型

采用 **tool 级阻塞**（方案 B）：

```
Agent 调用 companion_request(wait: true)
    ↓
tool execute 内部持有 channel receiver，agent loop 停在此 tool call
    ↓
对方调用 companion_respond → sender 发送 → tool call 返回
    ↓
Agent loop 正常继续下一轮
```

理由：
- 复用现有 Ask/Approval 的 channel 阻塞模型，改动最小
- 等待期间 agent context 完整保留在 loop 内，恢复质量最好
- 和现有 `ToolCallDecision::Ask` 的前端展示/事件流天然兼容

资源代价：等待期间子 agent session 占用 executor slot。可接受 — 作为 V1，先跑通再优化。

## Hook 通道复用

所有 companion 交互仍走现有 hook 通道：

- `companion_request(target: sub)` → 触发 `BeforeSubagentDispatch` / `AfterSubagentDispatch`
- `companion_request(target: parent)` → 在父 session 生成 `HookPendingAction`
- `companion_request(target: human)` → 在当前 session 生成 pending approval event
- `companion_respond` → 触发 `SubagentResult`（对应子 agent 回应）或更新 pending action 状态（对应父/人回应）

pending action 的 `action_type` 直接对应 `wait` 语义：
- `wait: true` → `blocking_review`（阻塞 before_stop）
- `wait: false` → `follow_up_required` 或 `suggestion`

## 典型场景

### 场景 1：子 agent 中途向上提审

```
父 Agent: companion_request(target=sub, payload={prompt: "实现用户认证模块"})
子 Agent: [实现中，发现需要确认技术选型]
子 Agent: companion_request(target=parent, wait=true, payload={prompt: "JWT 还是 session-cookie？推荐 JWT"})
  → 子 agent loop 停在 tool call
父 Agent: [收到 pending action 注入]
父 Agent: companion_respond(request_id, payload={status: approved, summary: "用 JWT，加上 refresh token"})
  → 子 agent tool call 返回，拿到结论
子 Agent: [按 JWT 方案继续实现]
子 Agent: companion_respond(原始 request_id, payload={status: completed, final: true, summary: "认证模块已完成"})
```

### 场景 2：链式上抛到人

```
父 Agent: companion_request(target=sub, payload={prompt: "审阅数据库 migration"})
子 Agent: companion_request(target=parent, wait=true, payload={prompt: "发现要删 users 表的 email 列，需确认"})
父 Agent: [收到，自己也拿不准]
父 Agent: companion_request(target=human, wait=true, payload={prompt: "子 agent 发现要删 email 列，是否确认？", options: ["确认删除", "保留该列"]})
  → 父 agent loop 停在 tool call
人: [在 UI 上选择 "保留该列"]
  → POST /api/.../respond {payload: {status: rejected, summary: "保留该列"}}
  → 父 agent tool call 返回
父 Agent: companion_respond(子 agent 的 request_id, payload={status: rejected, summary: "保留 email 列，用户要求不删"})
  → 子 agent tool call 返回
子 Agent: [调整 migration 方案，继续工作]
```

### 场景 3：同步等待子任务结果

```
Agent: companion_request(target=sub, wait=true, payload={prompt: "帮我跑一遍集成测试"})
  → agent loop 停在 tool call
子 Agent: [跑测试]
子 Agent: companion_respond(request_id, payload={status: completed, final: true, summary: "3 passed, 1 failed: auth_test"})
  → 父 agent tool call 返回，直接拿到测试结果
Agent: [根据结果修代码]
```

## 非目标

- 不做 session 级 suspend/resume（留给后续优化）
- 不在本轮合并 tool approval endpoint（保持现有审批通道独立运作，后续可统一）
- 不改变 session binding 的 owner 模型（Project/Story/Task 的归属关系不变）
- 不做跨 owner / 跨 project 的 companion 调度

## Acceptance Criteria

### 工具与协议
- [ ] `companion_request` 工具实现，支持 target=sub/parent/human 三个方向
- [ ] `companion_respond` 工具实现，替代现有 `companion_complete` 和 `resolve_hook_action`
- [ ] `wait: true` 的 tool 级阻塞可工作，复用 Ask 审批的 channel 模型
- [ ] 现有 `companion_dispatch` / `companion_complete` / `resolve_hook_action` 直接删除，清理引用

### Payload Type 系统
- [ ] PayloadTypeRegistry 实现，支持注册 request_schema / response_schema / ui_hint
- [ ] 内置 payload types 注册：task、completion、review、resolution、approval、decision、notification
- [ ] companion_request 执行时按 type 校验 request payload
- [ ] companion_respond 执行时按 pending action 上的 response_schema 校验 respond payload
- [ ] hook 注入消息尾部携带回复约束提示（type + 必填字段）
- [ ] 未识别的 payload 字段静默忽略，保证向前兼容

### 前端与人机交互
- [ ] 人的 respond API endpoint 可工作，与 agent 的 companion_respond 共享 payload 校验
- [ ] 前端根据 ui_hint 渲染对应 UI 组件（approval_card / review_card / notification_toast 等）

### 端到端
- [ ] 链式上抛场景（子→父→人→父→子）端到端可跑通
- [ ] hook 通道（BeforeSubagentDispatch / SubagentResult / PendingAction）复用不破坏现有机制
- [ ] Task execution session 仍保持 dispatch 边界不被误放开
