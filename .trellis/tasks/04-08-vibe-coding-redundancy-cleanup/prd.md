# Vibe Coding 冗余清理：消除多路径重复代码

## 背景

项目在 vibe coding 快速迭代期间积累了较多冗余：同一目的存在多条实现路径、工具函数重复定义、死代码残留、结构体字段雷同等。本 task 记录全面 review 的发现，供后续逐项清理。

## Goal

消除代码库中由 vibe coding 遗留的冗余代码和不一致模式，降低维护成本、提升可读性。

---

## 清理项一览

### P0 — 结构性冗余（优先处理）

#### 1. Context Builder 三路径统一

**现状**：三种不同路径构建会话上下文  
- `context/builder.rs` — Task 会话，使用 Contributor 注册表模式  
- `project/context_builder.rs` — Project 会话，直接 ContextComposer  
- `story/context_builder.rs` — Story 会话，直接 ContextComposer  

**重复点**：  
- `trim_or_dash()` 在 3 个文件重复定义（story、project、builtins）  
- workspace binding 摘要构建逻辑重复  
- `build_session_plan_fragments` 调用模式重复  
- resource block 构建 `json!({ "type": "resource", ... })` 重复  

**建议**：统一到 Contributor 模式，`context/builder.rs` 成为唯一入口。

#### 2. 前端统一 API 调用模式

**现状**：`api/client.ts` 已提供 `api.get/post/put/delete`，但 service 层分裂为两套风格  
- **Pattern A** (api.*): workflow.ts, canvas.ts, currentUser.ts, browseDirectory.ts, directory.ts  
- **Pattern B** (authenticatedFetch 手动): session.ts, executor.ts, filePicker.ts, addressSpaces.ts  

**建议**：Pattern B 的 service 迁移到 `api.*`；`authenticatedFetch` 仅保留给 SSE/streaming 场景。

---

### P1 — 代码坏味道（逐步清理）

#### 3. 删除 RemoteAcpConnector 死代码

- `connectors/remote_acp.rs` (106行) 所有方法返回 `Err("尚未实现")`
- 整个代码库无任何引用
- 直接删除，需要时从 git 恢复

#### 4. 合并 Start/Continue Task 重复结构体

`task/execution.rs` 中 `StartTaskResult` / `ContinueTaskResult` 字段完全一致；  
`StartTaskCommand` / `ContinueTaskCommand` 也几乎一致。  
合并为 `TaskExecutionResult` + `TaskExecutionCommand { phase, prompt }` + `ExecutionPhase` 枚举。

#### 5. 前端提取 Response 映射工具

- `canvas.ts` 和 `workflow.ts` 各有一份相同的 `asRecord` / `asRecordArray`
- `session.ts` 有自己的 `requireStringField` / `requireNumberField`
- 提取到 `api/mappers.ts` 或 `utils/response.ts`

#### 6. 清理向后兼容别名和 dead_code

- `context_container.rs` 的 `ContextContainerCapability = MountCapability` 别名 — 项目未上线无需兼容
- 全局 22 处 `#[allow(dead_code)]` 逐个排查后清理

---

### P2 — 中期改善（可分批处理）

#### 7. Session 计划/上下文文件合并

`session_context.rs`、`session_plan.rs`、`bootstrap_plan.rs` 散落在 application crate 根，  
且与 `session/` 模块目录并存。合并到 `session/plan/` 子模块，减少认知负担。

#### 8. 拆分超大文件

| 文件 | 行数 | 方向 |
|---|---|---|
| session/hub.rs | 2656 | 测试抽到独立文件；companion wait 抽为独立模块 |
| pi_agent/connector.rs | 2783 | 按职责拆分：tool handling / stream processing / agent loop |
| hook_delegate.rs | 930 | 可考虑按 trigger 类型拆分 |
| session_plan.rs | 819 | 合并到 session/plan/ 子模块 |

#### 9. 前端 types/index.ts 拆分

1181 行单体类型文件 → 按领域拆分为 `types/story.ts`、`types/workflow.ts`、`types/session.ts` 等，  
`types/index.ts` 仅做 re-export。

---

## 不纳入本次清理的项

- SQLite / PostgreSQL session repository 去重 — 属于持久化层架构决策，单独评估

## Acceptance Criteria

- [ ] P0 清理项全部完成
- [ ] P1 清理项全部完成
- [ ] P2 至少完成超大文件拆分
- [ ] 清理后 `pnpm run check` 全通过
