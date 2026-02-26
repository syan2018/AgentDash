# 前端实现 Agent 连接与 ACP 会话绘制（Session 视图 MVP）

## 背景与动机

我们已经完成了两项关键工作：

- **执行集成层（后端）**：提供了 `POST /api/sessions/{id}/prompt` 与 `GET /api/acp/sessions/{id}/stream`，并以 ACP `SessionNotification` 作为统一流式数据结构（可回放历史）。  
- **ACP 前端绘制组件（前端）**：已具备 `frontend/src/features/acp-session/` 的渲染能力（`AcpSessionList` / `AcpSessionEntry` / `useAcpStream` 等）。

但目前 AgentDashboard 前端仍缺少一个“正式的”交互入口来：

1) 创建/管理会话（session）  
2) 发送 prompt（启动执行）  
3) 连接 ACP WebSocket 并将消息流稳定绘制出来  

本任务目标是参考：

- vibe-kanban 的连接/重连实现（WS 管理、finished 处理、指数退避等）
- ABCCraft 的 Session（对话创建/连接器选择/消息流交互）交互范式

在 AgentDashboard 前端落地一个可用的 **Session 视图 MVP**，实现“连接 + 执行 + 绘制”的闭环。

---

## 目标（Goal）

在 AgentDashboard 前端新增一个 Session 视图（或等价的工作流入口），让用户可以：

- 创建/选择一个 session
- 发送 prompt 启动执行
- 通过 ACP WebSocket 接收 `SessionNotification`，并使用既有 ACP 组件渲染
- 支持基础的连接状态展示、错误提示、取消执行与重新开始

---

## 范围（Scope）

### In Scope（本任务内）

- 前端新增 Session 入口（导航/按钮/页面结构）与基础 UI
- 会话 ID（sessionId）生成、切换、复制与历史回放体验
- 发送 prompt：复用 `frontend/src/services/executor.ts::promptSession`
- 流式绘制：复用 `frontend/src/features/acp-session/` 组件
- WebSocket 连接健壮性改进：参考 vibe-kanban（重连、资源释放、错误/close 处理、finished 终止语义）
- 以“创建项目/创建 Story 辅助 Agent”为主的**预设 prompt 模板**（先做到体验入口与复用，不强依赖后端 tool 扩展）

### Out of Scope（明确不做/后续任务）

- 真正的多轮对话 Session（同一个 sessionId 反复 prompt）  
  - 当前后端 `ExecutorHub::start_prompt` 对同一 `session_id` 只会启动一次（重复调用直接返回 OK，不再产生新流）。  
  - 多轮对话需要后端调整 session runtime 语义，建议拆分后续任务处理。
- 复杂的“结构化创建项目”工具链（让 Agent 直接调用后端创建 Story/Task 的 tool call），此为后续增强项
- 后端按 `backendId` 路由不同执行器/远程后端（如要跨后端执行，需要明确新的 API 契约）

---

## 关键契约与约束（Contract & Constraints）

### 1) HTTP：启动执行

- `POST /api/sessions/{sessionId}/prompt`
- Body（camelCase）：
  - `prompt: string`（必填）
  - `workingDir?: string`
  - `env?: Record<string, string>`
  - `executorConfig?: { executor: string; variant?: string; modelId?: string; ... }`

### 2) WebSocket：接收 ACP 流（含历史回放）

- `GET /api/acp/sessions/{sessionId}/stream`
- 服务端会先发送 history（从 jsonl 读出），再持续推送新的 `SessionNotification`
- 客户端可选发送控制消息：
  - `{"type":"execute", ...PromptSessionRequest}`（snake_case 的 tag + camelCase 的字段）
  - `{"type":"cancel"}`

### 3) sessionId 的语义（当前实现的“单次执行会话”）

- 后端会对同一 `sessionId` 只允许 `start_prompt` 成功触发一次（后续 prompt 不再启动新流）
- 因此前端 MVP 需要：
  - **每次点击“发送/执行”时生成新的 sessionId**（或显式“新会话执行”）
  - 提供“重新开始”行为：生成新 sessionId 并重新连接

---

## 用户体验（UX）设计要点

### Session 视图的最小交互闭环

- 顶部：Session 标题（可选）、sessionId 展示（可复制）、连接状态（connected/loading/error）
- 中部：ACP 流渲染区域（`AcpSessionList`）
- 底部：输入框 + 发送按钮 + 取消按钮 + 新会话按钮
- 可选：执行配置（executor/profile、permissionPolicy 等）以折叠面板呈现

### “创建项目/Story 辅助 Agent”入口（MVP）

- 提供 1~2 个预设模板按钮：
  - “创建项目/Story：需求澄清 + 产出标题/描述/下一步”
  - “生成初始执行计划（plan）”
- 模板只负责生成 prompt（不做强制结构化解析）；后续可升级为结构化 JSON 输出 + 一键创建 Story

---

## 需求（Requirements）

### R1：新增 Session 入口与页面/视图

- [ ] 在当前 UI 中提供进入 Session 视图的入口（不要求复杂路由，允许先用简单页面/面板）
- [ ] 能在 Session 视图中展示 ACP 流（复用 `AcpSessionList`）

### R2：会话生命周期管理

- [ ] 自动生成 `sessionId`（短 UUID/时间戳方案均可，但必须足够不冲突）
- [ ] 支持复制 `sessionId`
- [ ] 支持“新会话/重新开始”（生成新 sessionId 并清空前端条目）

### R3：启动执行（Prompt）

- [ ] 发送 prompt 前做基础校验（空输入不发送）
- [ ] 发送请求可选走两种模式（二选一即可先落地）：
  - A) HTTP：调用 `promptSession(sessionId, req)`，然后连接 WS 获取流
  - B) WS：连接后发送 `{"type":"execute", ...req}`
- [ ] 提供 Cancel 按钮（调用 WS cancel 或新增后端 cancel API；若仅能 WS cancel，需确保连接存在）

### R4：WebSocket 连接健壮性（参考 vibe-kanban）

- [ ] 断线重连策略（指数退避 + 上限）
- [ ] `close` / `error` 后正确更新 UI 状态
- [ ] 资源释放：组件卸载/切换 sessionId 时移除事件监听并关闭 WS
- [ ] 明确 finished/terminal 语义（若后端提供 finished 消息则不重连；当前后端未实现 finished 消息，可作为兼容逻辑预留）

### R5：验收可观测性

- [ ] 在 UI 层给出明确错误信息（连接失败、解析失败、请求失败）
- [ ] 能复现：刷新页面后仍可回放 session history（后端 jsonl 读取）

---

## 验收标准（Acceptance Criteria）

- [ ] 前端可以在一个明确的“Session 视图”里输入 prompt 并启动执行
- [ ] 能稳定连接 `GET /api/acp/sessions/{id}/stream` 并渲染消息流（含 tool_call/plan/content）
- [ ] 页面刷新后，使用同一 `sessionId` 能回放历史（至少能看到之前的条目）
- [ ] “重新开始”会生成新的 sessionId 并开始新的执行流（不受后端“同 sessionId 只能启动一次”的限制）
- [ ] 断开网络/服务端重启时，UI 不会卡死，能给出可理解的连接状态与重连入口

---

## 技术方案草案（Technical Notes）

### 建议的实现路径（分阶段）

1) **MVP 打通**：新增 Session 视图 + 生成 sessionId + prompt + `AcpSessionList` 展示  
2) **连接增强**：将 `useAcpStream` 的连接管理对齐 vibe-kanban：重连、finished、清理  
3) **体验增强**：增加预设 prompt 模板、executorConfig 折叠配置、复制 sessionId、快捷重新开始

### 风险与注意事项

- 后端 sessionId 的“单次启动”语义很容易让用户困惑；前端必须显式“新会话执行”。
- 当前系统存在“后端连接（backendId）”概念，但 session 执行接口暂未按 backendId 区分；若要做“对指定后端执行”，需要补充跨层契约并另起任务。

