# 上下文注入机制（Injection 模块）

## Goal

建立一个可扩展的上下文注入能力，将“上下文声明”“上下文解析”“上下文组装”“上下文投递”拆开，支撑 Story / Task / Session 三类注入点，并为后续 `@引用文件`、项目快照、远端文档等能力提供统一底座。

这次实现不再从零设计一套完全独立的新框架，而是基于项目现有的 Task 上下文构建链路继续演进：

- 已有 `ContextContributor + ContextComposer + prompt_blocks` 雏形
- 已有 Story / Task 上下文字段与 MCP 工具读写入口
- 已有执行器对结构化 `prompt_blocks` 与 `mcp_servers` 的支持

目标是在不推翻现有链路的前提下，把它正式收敛为 Injection 模块。

## Requirements

### R1. 注入模型拆为四层

实现应明确区分以下四层职责：

1. **声明层（Declaration）**
   - 用结构化 `ContextSourceRef` 声明“要注入什么”
   - 声明应可挂在 Story 与 Task 侧
   - 声明只描述来源，不直接保存解析结果

2. **解析层（Resolution）**
   - 不同来源由不同 `SourceResolver` 负责解析
   - 解析结果必须统一为标准化片段，而不是各自拼字符串
   - 解析应支持失败处理、字符预算、相对路径解析

3. **组装层（Composition）**
   - 基于 `slot + order + merge strategy` 合并上下文片段
   - 区分普通上下文与最终指令
   - 保留来源摘要，便于调试与预览

4. **投递层（Delivery）**
   - 最终产物不是单一字符串，而是可直接投递给执行器的结构化上下文包
   - 至少包含：`prompt_blocks`、`source_summary`、`mcp_servers`、`working_dir`

### R2. 引入结构化 Source Ref

需要新增统一的来源声明类型：

```rust
pub struct ContextSourceRef {
    pub kind: ContextSourceKind,
    pub locator: String,
    pub label: Option<String>,
    pub slot: ContextSlot,
    pub priority: i32,
    pub required: bool,
    pub max_chars: Option<usize>,
    pub delivery: ContextDelivery,
}
```

首批支持的 `kind`：

- `manual_text`
- `file`
- `project_snapshot`

首批支持的 `slot`：

- `requirements`
- `constraints`
- `codebase`
- `references`
- `instruction_append`

### R3. 保留现有文本字段，但新增声明式来源字段

为了平滑落地，第一阶段同时保留已有字段与新字段：

- Story 继续保留：`prd_doc`、`spec_refs`、`resource_list`
- Task / AgentBinding 继续保留：`prompt_template`、`initial_context`
- 同时新增声明式 `source_refs`

第一阶段要求：

- 旧字段仍可正常参与上下文构建
- 新字段一旦存在，应纳入统一注入流程
- 不要求立刻废弃旧字段，但要明确它们未来会逐步被 `source_refs` 替代

### R4. 首批实现 3 类 Resolver

#### 4.1 `ManualTextResolver`

- 将 `locator` 视为正文内容
- 适合 PRD 摘要、人工补充约束、临时执行提示等

#### 4.2 `FileResolver`

- 支持从工作区解析相对路径
- 首批支持：`.md` / `.txt` / `.json` / `.yaml` / `.yml`
- 结构化内容需转换为可读 Markdown
- 对大文件设置默认大小上限，超限时截断或报错

#### 4.3 `ProjectSnapshotResolver`

- 基于工作区自动生成项目结构摘要
- 至少包含：目录树摘要、技术栈线索、关键入口文件
- 默认排除：`node_modules`、`target`、`.git` 等噪音目录

### R5. 继续复用片段式组装模型

不采用“整份内容整体合并”的粗粒度模型，统一使用片段式模型：

```rust
pub struct ContextFragment {
    pub slot: String,
    pub label: String,
    pub order: i32,
    pub strategy: MergeStrategy,
    pub content: String,
}
```

要求：

- 保留 `Append` / `Override`
- 同 slot 内容按顺序拼接
- `instruction` slot 单独组装，避免被普通上下文污染

### R6. 与现有 Task 执行链路集成

第一阶段直接集成到现有 Task 执行上下文构建流程，不单独新起平行链路。

集成目标：

- `build_task_agent_context(...)` 继续作为任务执行前的总入口
- 内部新增“声明式来源贡献者”参与上下文构建
- 执行网关继续把结果投递到执行器的 `prompt_blocks` / `mcp_servers`

### R7. 提供预览与统计能力

需要能在不真正启动执行器的情况下预览最终注入结果，至少包括：

- 最终 Markdown 上下文
- 指令部分
- 来源摘要
- 字符统计

第一阶段可以先提供内部函数级能力，不强制要求完整 HTTP UI。

## Acceptance Criteria

- [ ] Domain 中存在统一的 `ContextSourceRef` 模型，可被 Story / Task 持有
- [ ] 至少实现 `manual_text`、`file`、`project_snapshot` 三类来源解析
- [ ] 旧字段与新 `source_refs` 能同时参与 Task 执行上下文构建
- [ ] 注入结果通过现有 `prompt_blocks` 链路进入执行器，而不是退化为单一纯文本接口
- [ ] `instruction` 与普通 `context` 明确分离
- [ ] 对来源解析失败、超大文件、缺失工作区等情况有明确处理
- [ ] 具备单元测试覆盖核心解析与组装逻辑

## Definition of Done

- [ ] 更新 PRD，使设计与现有代码状态一致
- [ ] 落地第一阶段代码骨架
- [ ] Task 执行前可以消费声明式来源
- [ ] 代码通过相关 crate 的测试/检查

## Technical Approach

### 总体方向

采用“**保留现有执行入口，抽出独立 Injection 能力层**”的实现方式：

1. 在 Domain 定义 `ContextSourceRef`
2. 新增独立 `agentdash-injection` crate，承载：
   - source ref 解析
   - 上下文片段组装
   - 项目快照生成
3. `agentdash-api::task_agent_context` 不再自己定义一整套独有上下文模型，而是复用 Injection crate 的公共抽象
4. `TaskExecutionGateway` 继续只做装配与投递，不负责解析来源

### 模块落位

```text
crates/
├── agentdash-domain/
│   └── src/
│       └── context_source.rs      # ContextSourceRef 等声明类型
├── agentdash-injection/
│   └── src/
│       ├── lib.rs
│       ├── composer.rs            # ContextFragment / ContextComposer
│       ├── error.rs
│       ├── resolver.rs            # SourceResolver trait / dispatch
│       └── resolvers/
│           ├── manual_text.rs
│           ├── file.rs
│           └── project_snapshot.rs
└── agentdash-api/
    └── src/
        └── task_agent_context.rs  # 组装 Task 侧 contributor，并调用 injection crate
```

### 第一阶段数据来源

Task 执行前的来源按以下顺序参与构建：

1. Task / Story / Project / Workspace 的核心结构化信息
2. Story 旧字段：`prd_doc` / `spec_refs` / `resource_list`
3. Task 旧字段：`initial_context`
4. Story 的 `source_refs`
5. Task 的 `context_sources`
6. MCP 能力注入
7. 最终指令模板与 continue 附加指令

### 错误策略

- `required = true` 的来源解析失败：本次构建失败
- `required = false` 的来源解析失败：记录到来源摘要或 warning，跳过继续
- 无工作区但请求 `file` / `project_snapshot`：按 required 策略处理
- 超出 `max_chars`：先截断，再在内容尾部标明已截断

## Decision (ADR-lite)

**Context**

项目已经存在一条 Task 执行上下文链路，但抽象分散在 API 层，且缺少统一的来源声明与解析能力。PRD 原版倾向从零定义 `Injector trait + CompositeInjector`，但这会与现有可工作的片段式上下文构建模型形成重复建设。

**Decision**

采用“保留现有执行入口，新增 `ContextSourceRef + SourceResolver + ContextComposer`”的方式演进为 Injection 模块；不再优先实现一个完全独立、与现有执行入口平行的 `InjectorRegistry + CompositeInjector` 体系。

**Consequences**

- 优点：更贴近现有代码，落地成本低，能尽快接入真实执行路径
- 优点：后续 `@引用文件`、项目快照、远端文档都可复用同一声明与解析模型
- 代价：第一阶段会存在“旧字段 + 新字段”并存
- 代价：`task_agent_context` 仍有一部分装配逻辑留在 API 层，后续仍需继续下沉

## Implementation Plan

- PR1: 新增 `ContextSourceRef` 与 `agentdash-injection` 基础 crate
- PR2: 接入 Task 上下文构建，支持 `manual_text` / `file` / `project_snapshot`
- PR3: 为 Story MCP / Task API 增加 source refs 读写入口与预览能力

## Out of Scope

- 不实现 PDF / Word / 网页抓取等复杂解析
- 不实现智能相关性排序
- 不实现缓存层
- 不实现完整 UI 预览页面
- 不在第一阶段废弃 `prd_doc` / `initial_context` 等旧字段

## Technical Notes

- 现有可复用入口：`crates/agentdash-api/src/task_agent_context.rs`
- 现有执行投递结构：`crates/agentdash-executor/src/hub.rs`
- 现有 Story 上下文 MCP 工具：`crates/agentdash-mcp/src/servers/story.rs`
- 现有 Task 侧上下文读取工具：`crates/agentdash-mcp/src/servers/task.rs`
- 第一阶段优先验证“声明式来源真的进入执行上下文”，而不是一次性做完整产品面
