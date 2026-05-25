# Design: Spec 文档体系收敛与迁移规划

## 1. 文档本体论

AgentDash 的 `.trellis/spec/` 应承载 architecture attractor 及其当前工程投影。它不是任务日志，也不是一次性验收清单。

spec 内容分三类：

| 类型 | 含义 | 维护规则 |
| --- | --- | --- |
| `Invariants` | 长期结构不变量，定义系统应向哪里收敛 | 默认不自动维护；重大架构思路变化时由用户明确确认 |
| `Current Baseline` | 当前代码如何投影不变量 | 可随实现事实维护；要求简短、事实化、可定位 |
| `Local Decisions` | 局部但稳定的设计选择 | 可维护，但只记录为什么，不记录历史错误过程 |

另有 `Contract Appendices`：协议、DTO、状态流、错误语义、序列化规则等可执行契约。它们可以比主文档更细，但仍不应收纳任务过程材料。

## 2. 文档载体边界

| 载体 | 职责 | 不应包含 |
| --- | --- | --- |
| `.trellis/spec/**/architecture.md` | 模块主文档，定义模块角色、不变量、当前基线和局部决策 | 任务验收清单、历史 changelog、临时 TODO |
| `.trellis/spec/**/<topic>.md` | 契约附录，记录协议、DTO、状态流、错误语义 | PR closure、Good/Base/Bad 模板、一次性测试命令 |
| `.trellis/tasks/**` | task harness，记录 plan、验证、audit、closure | 长期架构不变量的唯一来源 |
| `.trellis/workspace/**` | session memory，记录工作过程和开发者上下文 | 权威契约 |
| `AGENTS.md` 问题收纳 | 高频环境坑、agent 操作坑 | 模块架构设计 |

## 3. 目标目录模型

保留现有 layer 目录，但为主要模块建立 architecture 主文档：

```text
.trellis/spec/
├── index.md
├── project-overview.md
├── tech-stack.md
├── communication.md
├── shared/
│   └── index.md
├── guides/
│   ├── index.md
│   ├── cross-layer-thinking-guide.md
│   └── code-reuse-thinking-guide.md
├── backend/
│   ├── architecture.md
│   ├── index.md
│   ├── database-guidelines.md
│   ├── repository-pattern.md
│   ├── error-handling.md
│   ├── session/
│   │   ├── architecture.md
│   │   ├── startup-pipeline.md
│   │   ├── runtime-execution-state.md
│   │   ├── execution-context-frames.md
│   │   ├── bundle-main-datasource.md
│   │   └── streaming-protocol.md
│   ├── workflow/
│   │   ├── architecture.md
│   │   ├── activity-lifecycle.md
│   │   └── lifecycle-edge.md
│   ├── vfs/
│   │   ├── architecture.md
│   │   ├── vfs-access.md
│   │   └── vfs-materialization.md
│   ├── hooks/
│   │   ├── architecture.md
│   │   ├── execution-hook-runtime.md
│   │   └── hook-script-engine.md
│   └── capability/
│       ├── architecture.md
│       ├── tool-capability-pipeline.md
│       ├── capability-dimension-pipeline.md
│       ├── llm-model-config.md
│       └── plugin-api.md
├── frontend/
│   ├── architecture.md
│   ├── design-language.md
│   ├── type-safety.md
│   ├── state-management.md
│   └── workflow-activity-lifecycle.md
└── cross-layer/
    ├── architecture.md
    ├── backbone-protocol.md
    ├── desktop-local-runtime.md
    ├── shared-library-contract.md
    └── project-backend-workspace-routing.md
```

迁移可以先新增 architecture 主文档并更新索引，不强制立即重命名所有现有文件。

## 4. Architecture 主文档模板

```markdown
# <Module> Architecture

## Role

模块在系统中的职责、边界和它服务的上层目标。

## Invariants

长期结构约束。默认不得由自动 spec update 改写。

## Current Baseline

当前代码对这些不变量的工程投影：主要 crate、入口类型、生产路径、权威 DTO 或状态源。

## Local Decisions

局部但稳定的设计选择。只记录为什么。

## Contract Appendices

链接到更细的契约文档。
```

## 5. 迁移分类规则

| 内容形态 | 处理方式 |
| --- | --- |
| 权威事实源、边界、状态流、协议字段 | 保留到 architecture 或 contract appendix |
| 当前 crate / endpoint / provider / action 列表 | 放入 Current Baseline，保持短小 |
| 局部技术选择及原因 | 放入 Local Decisions |
| `Tests Required` / `Command gate` / `Verification` | 迁出到 task harness 或删除 |
| `Good/Base/Bad Cases` / `Wrong vs Correct` | 默认迁出到 task；少量可转写为契约说明 |
| 日期型 changelog / 已迁移历史 | 迁出到 archived task 或删除 |
| 旧方案对照 / 兼容层过渡说明 | 若当前仍是事实，写入 Current Baseline；若只是历史，迁出或删除 |
| gotcha / 环境坑 | 放入 AGENTS 问题收纳或 workspace journal |

## 6. 存量文档树迁移矩阵

### 6.1 明确全局文档

这些文档描述整个项目或 Trellis/spec 体系本身，不应下沉到某个模块：

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `index.md` | 全局 spec 入口 | 保留；改写为 spec ontology + reading order；移除状态列 |
| `project-overview.md` | 全局 architecture overview | 保留；只保留项目定位、核心抽象、Cloud/Local 数据归属、顶层原则 |
| `tech-stack.md` | 全局 Current Baseline | 保留；强调它是当前技术基线，不是架构不变量 |
| `communication.md` | 全局协作规范 | 保留；可与 `shared/index.md` 去重 |
| `shared/index.md` | 全局跨语言 coding convention | 保留或并入根级 shared convention；避免重复 communication |
| `guides/index.md` | 全局 thinking harness 入口 | 保留；说明 guides 不是 architecture contract |
| `guides/cross-layer-thinking-guide.md` | 全局 thinking harness | 保留为短 checklist，指向 architecture / contract docs |
| `guides/code-reuse-thinking-guide.md` | 全局 thinking harness | 保留为短 checklist；命令示例改用项目偏好的 `rg` |

### 6.2 Layer 主入口

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/index.md` | backend layer index | 保留为索引；新增 `backend/architecture.md` 为主入口 |
| `frontend/index.md` | frontend layer index | 保留为索引；新增 `frontend/architecture.md` 为主入口 |
| `cross-layer/index.md` | cross-layer index | 保留为索引；新增 `cross-layer/architecture.md` 为主入口 |

### 6.3 Backend 全局与通用规范

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/directory-structure.md` | `backend/architecture.md` + appendix | 分层不变量上移到 architecture；crate 清单作为 Current Baseline |
| `backend/architecture-evolution.md` | task memory / archived history | 从 spec 主阅读路径移除；只抽取仍成立的分层原则 |
| `backend/database-guidelines.md` | backend persistence appendix | 保留；去掉 checklist 式任务语气，保留 migration 不变量 |
| `backend/repository-pattern.md` | backend architecture / persistence appendix | 聚合边界不变量上移；细节保留 appendix |
| `backend/error-handling.md` | backend error contract appendix | 保留；字段/错误映射属于 contract |
| `backend/domain-payload-typing.md` | backend domain appendix | 保留；Value 治理是长期边界 |
| `backend/quality-guidelines.md` | backend coding convention appendix | 瘦身；将 Session launch 字段归属移到 session architecture |
| `backend/logging-guidelines.md` | backend observability appendix | 保留；删除过细代码示例时保留字段原则 |
| `backend/embedded-skill-bundles.md` | backend/domain asset appendix | 保留；如后续 asset 模块增多可下沉到 `backend/assets/` |
| `backend/runtime-gateway.md` | backend/runtime architecture appendix | 保留；未来可下沉到 `backend/runtime/` 模块 |
| `backend/story-task-runtime.md` | backend/workflow 或 backend/session architecture | 核心 Story/Task/Session/Lifecycle 关系上移到相关 architecture；待演进迁出 |
| `backend/shared-library.md` | cross-layer shared-library + backend-specific appendix | 与 cross-layer 权威契约去重；只保留后端 seed/validator/事务语义 |

### 6.4 Backend Session 模块

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/session/session-startup-pipeline.md` | `backend/session/architecture.md` + startup appendix | 主线不变量上移；scenario 验收内容迁出 |
| `backend/session/runtime-execution-state.md` | session runtime appendix | 保留；边界表可进入 architecture Current Baseline |
| `backend/session/execution-context-frames.md` | session connector projection appendix | 保留；ExecutionContext 不是事实源这一点上移为 invariant |
| `backend/session/bundle-main-datasource.md` | session context appendix | 保留；Bundle 主数据面不变量上移 |
| `backend/session/streaming-protocol.md` | cross-layer 或 session protocol appendix | NDJSON wire contract 可移到 `cross-layer/`；session-specific 字段可留 appendix |
| `backend/session/pi-agent-streaming.md` | session / connector appendix | 保留为 PiAgent connector 映射契约；如建 connector 模块可下沉 |

### 6.5 Backend Workflow 模块

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/workflow/activity-lifecycle.md` | `backend/workflow/architecture.md` + appendix | ActivityEvent/LifecycleEngine 权威上移；测试清单迁出 |
| `backend/workflow/lifecycle-edge.md` | workflow edge appendix | 保留 edge kind 和校验契约；migration/未来扩展迁出 |

### 6.6 Backend VFS 模块

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/vfs/vfs-access.md` | `backend/vfs/architecture.md` + access appendix | 地址模型、provider 边界上移；scenario 测试清单迁出 |
| `backend/vfs/vfs-materialization.md` | VFS materialization appendix | 保留；尾部创建/精简脚注删除 |

### 6.7 Backend Hooks 模块

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/hooks/execution-hook-runtime.md` | `backend/hooks/architecture.md` + runtime appendix | loop 外取信息、loop 边界控制等不变量上移 |
| `backend/hooks/hook-script-engine.md` | hook script appendix | 保留 Rhai 决策原因；当前 preset 清单改为 Current Baseline 或移到代码引用 |

### 6.8 Backend Capability / Plugin 模块

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `backend/capability/tool-capability-pipeline.md` | `backend/capability/architecture.md` + tool appendix | capability resolver 不变量上移；矩阵作为 Current Baseline |
| `backend/capability/capability-dimension-pipeline.md` | capability dimension appendix | 保留；scenario 验收内容迁出 |
| `backend/capability/llm-model-config.md` | capability / model config appendix | 保留；provider 列表标注为 Current Baseline |
| `backend/capability/plugin-api.md` | capability / plugin architecture appendix | 保留；stable/experimental/internal 分层可进入 architecture |

### 6.9 Frontend 文档

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `frontend/directory-structure.md` | `frontend/architecture.md` + appendix | FSD/module baseline 上移；逐文件目录树压缩 |
| `frontend/component-guidelines.md` | frontend UI convention appendix | 保留或并入 design-language，避免重复样式规则 |
| `frontend/design-language.md` | frontend design-system appendix | 保留；primitive list 是 Current Baseline |
| `frontend/hook-guidelines.md` | frontend session/stream appendix | 保留；NDJSON hook 契约可与 streaming protocol 互链 |
| `frontend/state-management.md` | frontend state appendix | 保留；store 清单标注为 Current Baseline |
| `frontend/type-safety.md` | frontend data contract appendix | 保留；snake_case 直接映射是 invariant |
| `frontend/quality-guidelines.md` | frontend coding convention appendix | 瘦身；测试清单迁出 |
| `frontend/workflow-activity-lifecycle.md` | frontend workflow appendix | Activity on-wire/UI 模型契约保留；编辑器 layout 任务细节迁出 |

### 6.10 Cross-layer 文档

| 当前文件 | 目标归属 | 迁移动作 |
| --- | --- | --- |
| `cross-layer/backbone-protocol.md` | cross-layer protocol appendix | 保留 Backbone 权威；历史 ACP 对照/过渡期说明迁出或转为 Current Baseline |
| `cross-layer/desktop-local-runtime.md` | cross-layer desktop architecture appendix | 保留；Dashboard/API/Tauri 边界是 invariant |
| `cross-layer/project-backend-workspace-routing.md` | cross-layer workspace routing appendix | 保留契约；scenario 验收内容迁出 |
| `cross-layer/shared-library-contract.md` | cross-layer shared-library 权威契约 | 保留为主契约；与 backend/shared-library 去重 |

## 7. 高风险文件处置建议

### `backend/architecture-evolution.md`

该文件是 changelog，不是 architecture 主文档。迁移时应：

- 抽取仍成立的后端分层不变量到 `backend/architecture.md`。
- 历史迁移记录留在相关 archived task，不在 spec 主体保留。
- 从 backend index 中移除“通用开发规范”入口，或改为历史参考。

### `backend/session/session-startup-pipeline.md`

保留为 contract appendix，但需瘦身：

- 主线 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext` 进入 `session/architecture.md` 的 Invariants。
- Stage responsibilities、construction/launch 边界保留。
- 多个 `Scenario` 的测试清单、Wrong/Correct、Verification 迁出或删除。

### `cross-layer/shared-library-contract.md` 与 `backend/shared-library.md`

需要明确主从：

- `cross-layer/shared-library-contract.md` 作为跨层权威契约。
- `backend/shared-library.md` 只保留后端 seed、validator、安装事务、plugin embedded 相关基线，或并入 `backend/capability/architecture.md` / `cross-layer/shared-library-contract.md`。

### `frontend/workflow-activity-lifecycle.md`

拆分心智：

- Activity 作为 wire/editor/run 唯一模型进入 `frontend/architecture.md` 或 `workflow/architecture.md`。
- 编辑器具体 layout、测试命令、out-of-scope 迁回 task harness。
- API mapper 与 `activity_state` 读取契约可留作 appendix。

## 8. `trellis-update-spec` 调整方案

现有 `trellis-update-spec` 强调 code-spec 和强制 7 段模板，会鼓励把任务验收材料写进 spec。应改为：

### Spec Maintenance Goal

`.trellis/spec/` 的目标是维护项目 architecture attractor 及其当前工程投影，而不是记录任务过程。

### 自动维护边界

- 允许自动维护：
  - Current Baseline 中的事实性更新。
  - Contract Appendix 中的签名、字段、错误语义、状态流更新。
  - Local Decisions 中已经由任务明确确认的局部设计理由。
- 禁止自动维护：
  - Invariants 的新增、删除、改写。
  - 重大模块边界重定义。
  - 用历史错误反例污染主文档。
  - 将任务验证步骤写入 spec 主体。

### 不变量变更门槛

涉及 Invariants 时，update-spec 应停止自动编辑，改为在任务中记录提案，并要求用户确认。

### 7 段模板降级

原 `Scope / Signatures / Contracts / Validation / Cases / Tests / Wrong vs Correct` 模板只作为 contract appendix 的可选采集辅助，不作为 mandatory output。

## 9. 索引规则

每个 layer index 应标记：

- Architecture entry：本 layer 或模块的主入口。
- Contract appendices：协议/DTO/状态流文档。
- Guides：只放思考触发，不放实现契约。

index 不再维护“✅ 已更新/已创建”状态列。
