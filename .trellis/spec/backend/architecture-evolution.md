# 后端架构演进记录

> 记录后端架构的历次重大变更。每次变更的详细 plan 和 prd 参见 `.trellis/tasks/` 对应目录。

---

## 2026-02-26: 整洁架构重构

从混合分层迁移到整洁架构：

| 旧架构 | 新架构 | 状态 |
|--------|--------|------|
| `agentdash-state` | `agentdash-domain` + `agentdash-infrastructure` | ✅ 已迁移 |
| `agentdash-coordinator` | （已废弃、已移除） | ✅ 已清理 |
| `agentdash-api/executor/` | `agentdash-executor` | ✅ 已提取 |

关键变更：
1. 引入 `agentdash-domain` crate，包含实体和 Repository traits
2. 引入 `agentdash-infrastructure` crate，实现 Repository 接口
3. 使用 `Arc<dyn Repository>` 在 AppState 中进行依赖注入
4. 废弃 `agentdash-state` crate（删除 9 个文件，541 行）

---

## 2026-02-27: Project/Workspace/Story 领域模型重构

引入完整的 Project → Workspace → Story → Task 领域模型层次：

| 变更 | 说明 | 状态 |
|------|------|------|
| 新增 `project` 模块 | 项目容器，管理 Story/Workspace/Agent 预设 | ✅ |
| 新增 `workspace` 模块 | 物理工作空间，支持 GitWorktree/Static/Ephemeral | ✅ |
| 扩展 `Story` | 添加 `project_id`，`context` 改为结构化 `StoryContext` | ✅ |
| 扩展 `Task` | `workspace_path` → `workspace_id`，`agent_binding` 结构化 | ✅ |
| 扩展 Repository | Story/Task 支持完整 CRUD | ✅ |
| API 路由 | 新增 Project/Workspace 端点 | ✅ |

---

## 2026-03-20: API / Application 解耦重构

将 ~5640 行业务逻辑从 `agentdash-api` 迁移到 `agentdash-application`：

| Phase | 内容 | 状态 |
|-------|------|------|
| 1 | Session Plan + Context Composition → application | ✅ |
| 2 | Task Execution Gateway 纯逻辑 → application/task/ | ✅ |
| 3 | Address Space Access 三重职责拆分 | ✅ |
| 4 | Story Owner Session 编排 → application/story/ | ✅ |
| 5 | 引入 Response DTO / Assembler 层 (api/dto/) | ✅ |
| 6 | AppState 瘦身 → RepositorySet / ServiceSet / TaskRuntime / AppConfig | ✅ |

详见 `.trellis/tasks/03-19-decouple-api-domain-business-orchestration/plan.md`。

---

## 2026-03-27: API God Module Decomposition（深度解耦）

将 api 层残留的 God Module 逻辑进一步下沉到 application 层：

| Task | 内容 | API 层行数变化 |
|------|------|---------------|
| Task 1 | AgentTool SPI 下沉到 agentdash-spi + ThinkingLevel 统一 | - |
| Task 2 | execution_hooks 迁移到 application::hooks | ~2800 → 1 |
| Task 3 | Mount/AddressSpace 统一到 domain + service/tool 迁移 | ~1500 → ~6 |
| Task 4 | RepositorySet/runtime_bridge/workspace_resolution/gateway 核心下沉 | gateway 1493 → 360 |

架构改进：
- `RepositorySet` 定义从 api 下沉到 application
- `BackendAvailability` trait 解耦了 workspace resolution 对 AppState 的依赖
- Turn 监听/事件处理/artifact 持久化等核心逻辑已参数化并迁入 application
- API 层 relay dispatch 因依赖 api 独有的 BackendRegistry 暂时保留

详见 `.trellis/tasks/03-27-api-god-module-decomposition/prd.md`。

---

## 2026-03-29: Agent 核心类型抽取与 LLM Bridge 下沉

将 `agentdash-agent` 从混合依赖（rig-core + agentdash-spi）重构为纯净 Agent Loop 引擎：

| Phase | 内容 | 状态 |
|-------|------|------|
| 1 | 创建 `agentdash-agent-types` crate | ✅ |
| 2 | `agentdash-spi` 改为 re-export agent-types | ✅ |
| 3 | `agentdash-agent` 依赖瘦身：仅 agent-types + domain | ✅ |
| 4 | `RigBridge` + `convert.rs` 下沉到 executor | ✅ |
| 5 | 全量 check + clippy + test 通过 | ✅ |

关键设计决策：
- `AgentContext.tools` 改为 `Vec<ToolDefinition>`（仅 schema），Loop 通过独立 `tool_instances` 持有可执行实例
- `AgentTool` trait 和 `AgentRuntimeDelegate` trait 放在 agent-types
- `LlmBridge` trait 留在 agent crate（是 Agent Loop 的 port），`RigBridge` 是 adapter 放在 executor
- `BridgeRequest`/`BridgeResponse` 使用自有类型，不引用 rig 类型

详见 `.trellis/tasks/03-29-agent-types-extraction/prd.md`。

---

## 待办

- [x] 整合或废弃 `agentdash-coordinator` 遗留引用 — ✅ 已废弃，crate 已从 workspace 移除（2026-04-14 确认）
- [ ] 补充领域层单元测试
- [ ] Phase 6: SessionExecutor trait 解耦 application → executor（延伸目标）
