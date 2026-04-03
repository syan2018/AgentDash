# PRD：远程执行器 discovery 与 session prompt 路由未对齐

## 背景

当前前端会话页会通过 discovery 接口看到在线本机 backend 上报的远程 executors，例如：

- `CODEX`
- `CLAUDE_CODE`
- `GEMINI`
- `OPENCODE`

但真实手工验证表明，在 session 页手动选择这些 executor 后发送 prompt，云端主路由仍可能直接报错：

```text
未知执行器 'CODEX'，无法路由到任何连接器
```

这说明“前端可见的 executor 集合”和“session prompt 实际可路由的 executor 集合”当前并不一致。

## 当前现象

### 前端面

- `SessionChatView` 会展示 discovery 返回的 executors 作为可选项。
- 用户从下拉中可以选中 `Codex` 等远程 executor。

### 后端面

- `/api/discovery` 会合并：
  - 云端 `CompositeConnector.list_executors()`
  - `BackendRegistry` 里在线 backend 上报的 executors
- 但云端 `AppState.services.connector` 当前并未直接把这些远程 executors 挂进 session prompt 主路由。
- `CompositeConnector.prompt()` 仍要求 executor id 能在其内部 routing 表中命中本地 connector。

## 影响

- 用户会看到“可选”的远程 executor，但真实发送时直接 400。
- 它容易被误判成 session lifecycle 回归，实际是 discovery contract 和 prompt routing contract 脱节。
- 这条能力是否应该存在，本身也需要决策：系统到底要不要支持在当前会话页直接切远程 executor。

## 决策选项

### 方案 A：修补

目标：

- 保留前端展示远程 executor 的能力。
- 让 session prompt 主路径能够真正路由这些 executor。

可能要求：

- 明确远程 executor 该通过哪条路径进入：
  - 直接扩展 `CompositeConnector`
  - 或在 session prompt 路由侧显式走 relay / backend routing
- 明确 remote executor 的 discover / prompt / cancel / approval 能力边界
- 保证前端“可见 = 可发送”

### 方案 B：删除

目标：

- 如果当前产品并不打算支持“在通用会话页手动切远程 executor”，就应直接收窄 discovery / UI 暴露面。

可能要求：

- discovery 不再把当前 session prompt 无法直路由的 executors 暴露给会话页
- 或前端只在明确支持的场景显示这些 executor
- 保证前端“能选到的东西”一定可执行

## 判断标准

无论最终选 A 还是 B，都应满足：

1. 前端可见的 executor 集合与真实 prompt 能力一致。
2. 用户不会再遇到“下拉可选但发送 400”的行为。
3. 文档与 `AGENTS.md` 不再需要靠“这是已知误导点”来解释该现象。

## 相关文件

- `frontend/src/features/acp-session/ui/SessionChatView.tsx`
- `frontend/src/services/executor.ts`
- `crates/agentdash-api/src/routes/discovery.rs`
- `crates/agentdash-api/src/bootstrap/turn_dispatcher.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-executor/src/connectors/composite.rs`
- `crates/agentdash-executor/src/connectors/remote_acp.rs`

## 当前结论

这个 task 只是把问题独立记录出来，便于你后续决定：

- 要不要修补这条能力
- 如果不修，是否直接删除这条前端暴露面
