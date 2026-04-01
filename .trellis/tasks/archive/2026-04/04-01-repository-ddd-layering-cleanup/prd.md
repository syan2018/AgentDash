# 仓储 DDD 分层优化与存储实现收敛

## 目标

为 AgentDash 当前基础设施层建立一条可执行的仓储治理路线，解决以下几类问题：

1. Repository port 职责过宽，混入跨聚合更新、事件日志写入和查询订阅等非单一聚合职责。
2. PostgreSQL 与 SQLite 维护了两套几乎相同的业务仓储实现，带来高重复、高漂移和命名混乱。
3. 聚合持久化与事务边界不清晰，部分实现无法保证 root 与子对象原子提交。
4. Postgres 实现仍残留 `Sqlite*Repository` 命名，增加理解成本并掩盖真实运行时依赖。

本任务当前阶段先做规划与任务收敛，后续在该 task 下逐步推进实现。

## 背景

当前代码库已经具备较完整的 DDD/整洁架构外形：

- `agentdash-domain` 定义实体与 repository trait
- `agentdash-infrastructure` 提供 sqlite / postgres 持久化实现
- `agentdash-application` / `agentdash-api` 通过 trait port 使用仓储

但实际实现中仍存在几个明显问题：

- `StoryRepository` 同时承担 Story CRUD 与 `state_changes` 事件日志职责
- `TaskRepository` 直接编码“创建/删除 Task 时同步更新 Story.task_count”的跨聚合语义
- `WorkspaceRepository` 虽然把 `bindings` 视为聚合一部分，但持久化未做到原子提交
- `sqlite/` 与 `postgres/` 下的仓储大量复制，差异主要只剩 SQL 占位符和少量 schema introspection
- 运行时主链路已经主要使用 PostgreSQL，但 sqlite 业务仓储仍以正式实现形态长期共存

## 需求

### 一. 明确并收窄 repository port 职责

- 每个 repository port 优先只表达单一聚合的持久化语义
- 跨聚合更新不直接固化在某一个 repository port 中
- 事件日志 / 状态变更流从业务聚合仓储中拆分出来，形成独立 port

### 二. 纠正事务边界

- 聚合 root 与其子对象的持久化需要同事务提交，或显式引入 unit of work 语义
- 删除依赖“先写 root、再独立写 children”的半原子流程
- 明确哪些跨聚合更新由 application service 负责编排

### 三. 删除 sqlite 业务仓储实现

- 若 sqlite 已不再承担正式运行时职责，则移除其业务仓储实现和对应导出
- 若仍存在少量测试用途，需改为测试辅助设施，而不是与 PostgreSQL 平级的正式实现
- 删除后保证主链路、测试链路和导出结构保持清晰

### 四. 统一 Postgres 命名与目录语义

- `postgres/` 目录下不再保留 `Sqlite*Repository` 这类历史残名
- 对外导出与内部 struct 命名保持一致
- 让阅读者能直接从命名判断真实后端类型与用途

### 五. 抽取 shared 行为，避免再次复制

- 对仍需保留的跨后端共享逻辑，抽到明确的 shared 层或公共辅助模块
- 共享内容优先包括：
  - 领域对象 <-> 行数据映射
  - payload / state change 构造
  - 通用事务步骤
  - 枚举/状态字符串转换
- 方言差异仅保留在最薄的一层

## 验收标准

- [x] `StoryRepository` 不再承担事件日志存取职责，相关能力迁移到独立 port
- [x] `TaskRepository` 不再直接暴露跨聚合更新 API，跨聚合一致性由 application service 或明确事务边界处理
- [x] `Workspace` 聚合的 root 与 bindings 持久化具备原子性
- [x] sqlite 正式业务仓储实现从主实现层移除，或被明确降级为测试专用设施
- [x] Postgres 仓储命名完成统一，不再出现误导性的 `Sqlite*Repository`
- [x] sqlite / postgres 之间不再保留大面积 copy-paste 业务逻辑
- [x] 主运行时装配路径保持正确，并有对应验证

## 完成说明

本轮已完成以下收敛：

- 从 `StoryRepository` 中拆出独立 `StateChangeRepository`
- 新增显式事务型 `TaskAggregateCommandRepository`，把跨 `Task` / `Story` / `StateChange` 的一致性边界从 `TaskRepository` 中移出
- `WorkspaceRepository` 改为在单事务中持久化 root + bindings
- 删除 sqlite 正式业务仓储，仅保留本机端 `SqliteSessionRepository`
- `postgres/` 目录统一为 `Postgres*Repository` 命名
- 抽取 `state_change_store.rs` 共享状态变更日志逻辑
- 运行 `cargo fmt`、分 crate `cargo check` 与全量 `cargo check` 验证通过

## 非目标

- 本任务不要求一次性重写所有基础设施模块
- 本任务不以“兼容旧接口”为优先目标，预研阶段可优先追求正确分层
- 本任务不强制引入复杂 ORM 或泛型过度抽象
- 本任务不顺手重构无关的 API / session / workflow 模块

## 建议推进顺序

### 阶段 1. 语义收敛

- 盘点现有 repository port 的聚合边界
- 标记哪些方法其实属于 event store / projection store / application workflow
- 形成新的 port 划分草案

### 阶段 2. 事务与职责拆分

- 拆出 `StateChangeStore` / `EventLogRepository` 一类独立 port
- 把跨聚合更新从 repository trait 中移出
- 在 application service 中补齐新的事务编排入口

### 阶段 3. 基础设施收敛

- 优先保留 PostgreSQL 业务实现
- 删除或降级 sqlite 正式业务仓储
- 修复 `postgres/` 下的误导命名

### 阶段 4. shared 抽取与清理

- 抽公共映射、payload 构造和通用事务步骤
- 收敛导出结构
- 更新相关规范文档与测试

## 风险与关注点

- 删 sqlite 时需要确认是否仍有隐含测试依赖
- 拆 port 后，API 与 application service 的依赖注入会有一轮调整
- 若同时修改事务边界和导出结构，变更面会比较大，建议分阶段提交
- 当前已有 review 发现可作为起始输入，但后续仍要重新核对全量仓储实现

## 参考文件

- `.trellis/spec/backend/repository-pattern.md`
- `crates/agentdash-domain/src/story/repository.rs`
- `crates/agentdash-domain/src/task/repository.rs`
- `crates/agentdash-domain/src/workspace/repository.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/routes/projects.rs`
- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-application/src/project/management.rs`
- `crates/agentdash-infrastructure/src/lib.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`
