# Implementation Plan

## Phase 1: Terminal 收束

### 1.1 新建 AgentRunTerminalRegistry
- [ ] 创建 `crates/agentdash-application-agentrun/src/agent_run/terminal_registry.rs`
- [ ] 实现 `AgentRunTerminalRegistry` struct + 全部 API
- [ ] 在 `AppState` 中注册，替代 `terminal_cache` 字段
- [ ] 验证：编译通过

### 1.2 迁移 spawn 路径
- [ ] `routes/terminals.rs` `spawn_terminal_for_runtime_session`: register 到新 registry（传入 run_id + agent_id）
- [ ] `routes/lifecycle_agents.rs` spawn endpoint: 同上
- [ ] 验证：spawn terminal 功能正常

### 1.3 迁移 ws_handler 事件路由
- [ ] `EventTerminalOutput`: 从 `terminal_registry.get_terminal(id)` 取 run_id/agent_id → resolve active session → inject
- [ ] `EventTerminalStateChanged`: 同上路径 + `terminal_registry.update_state()`
- [ ] 验证：终端输出事件正确写入 journal

### 1.4 迁移 terminal API (input/resize/kill)
- [ ] 这些已通过 terminal_id 直接操作，只需确认权限校验不依赖 session_id
- [ ] 验证：交互式终端功能正常

### 1.5 新增终端输出回查 API
- [ ] 路由：`GET /agent-runs/:run_id/agents/:agent_id/runtime/terminals/:terminal_id/output`
- [ ] 实现：从 journal 过滤 terminal_output 事件，拼接返回
- [ ] 验证：API 返回正确输出内容

### 1.6 前端 useTerminalStore 扁平化
- [ ] `terminals: Map<string, TerminalInfo>` — 移除 sessionId 外层
- [ ] `registerTerminal` / `updateTerminalState` / `getTerminalsForSession` 接口调整
- [ ] `projectOutputEvent` / `projectStateEvent` 去重 key 改为 `event_seq` only
- [ ] 验证：类型检查 + 现有测试通过

### 1.7 前端 TerminalView 输出加载
- [ ] 新增 `fetchTerminalOutput(target, terminalId)` service 函数
- [ ] TerminalView: mount 时如果 store output 为空，调用 API 加载
- [ ] 验证：侧栏打开时显示历史输出

### 1.8 删除 SessionTerminalCache
- [ ] 删除 `crates/agentdash-application-runtime-session/src/session/terminal_cache.rs`
- [ ] 清理所有 import 和 AppState 中的旧字段
- [ ] 验证：编译通过，无死代码

---

## Phase 2: Title 收束

### 2.1 Workspace title 持久化
- [ ] `AgentRunWorkspace` 新增 `title: Option<String>` + `title_source: Option<String>` 字段
- [ ] 持久层 migration (如有 DB)
- [ ] 验证：字段可读写

### 2.2 Title 写入路径迁移
- [ ] `launch/commit.rs` auto_title: 写 workspace title
- [ ] `eventing.rs` project_source_session_title: 写 workspace title
- [ ] `title_service.rs` set_user_title: 写 workspace title
- [ ] 验证：title 更新后 workspace query 返回新 title

### 2.3 前端 title 消费路径
- [ ] `controlPlaneModel` 从 workspace state 读取 title（已通过 shell_model，只需确认不再读 session_meta）
- [ ] 移除 session_meta_updated title refresh 逻辑
- [ ] 验证：标题实时更新

### 2.4 清理 SessionMeta title 字段
- [ ] SessionMeta 中移除 title + title_source
- [ ] 清理 project_source_session_title 中写 SessionMeta 的逻辑
- [ ] 验证：编译通过

---

## Phase 3: Context Audit 收束

### 3.1 索引迁移
- [ ] `InMemoryContextAuditBus` inner key 从 `session_id` → `AgentRunKey(run_id, agent_id)`
- [ ] `emit()` 接口参数调整
- [ ] `query()` 接口参数调整
- [ ] 调用方（API handler）适配
- [ ] 验证：Context Inspector 功能正常

---

## Phase 4: Wait Activity + 清理

### 4.1 Wait Activity
- [ ] `sources/exec.rs`: `terminal_belongs_to_scope` 改用 AgentRun scope 判定
- [ ] `service.rs`: `collect_scope_exec_items` 从 registry.list_terminals(run_id, agent_id)
- [ ] 验证：wait 工具正确列出运行中终端

### 4.2 Hook Script Engine
- [ ] `script_engine.rs`: context 中移除 `session_id`，替换为 `run_id` + `agent_id`
- [ ] 验证：hook 脚本执行正常

### 4.3 Terminal Control Callback
- [ ] `agent_run_terminal_control.rs`: 入口参数从 session_id → (run_id, agent_id)
- [ ] 验证：turn boundary scheduling 正常

### 4.4 Legacy Canvas Endpoint
- [ ] 删除 session-scoped canvas invoke/snapshot endpoints
- [ ] 验证：AgentRun-scoped endpoints 正常

---

## Validation Commands

```bash
# 后端编译
cargo build --workspace

# 前端类型检查
cd packages/app-web && node ./node_modules/typescript/bin/tsc --noEmit --project tsconfig.app.json

# 前端测试
cd packages/app-web && node_modules/.bin/vitest run

# 集成验证（手动）
# - spawn terminal → 输出可见
# - session reconnect → terminal 仍可操作
# - 侧栏查看输出 → 内容显示
# - title 更新 → sidebar 实时刷新
# - context inspector → 审计事件连续
```
