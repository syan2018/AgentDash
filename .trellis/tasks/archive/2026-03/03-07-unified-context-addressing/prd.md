# 统一上下文引用与寻址空间设计

## Goal

为 Story / Task 的声明式上下文来源补齐一套可复用的“引用选择 + 统一寻址 + 运行时解析”设计：

1. 在创建 / 编辑上下文来源时，支持复用会话输入框现有的 `@文件引用` 交互，快捷选择工作空间下的文件；
2. 将当前仅面向文件系统的引用能力，演进为面向“统一寻址空间”的通用模型，为后续引用更多资源类型（如 MCP 资源、项目快照、知识条目、任务/Story 实体等）预留一致接口；
3. 明确不同运行环境下可用的寻址空间能力探测与暴露方式，避免前端硬编码“当前只能选文件”。

该任务当前先完成规划、建模与实施方案留档，不在本轮直接实现完整功能。

## What I already know

- 当前 Story / Task 已支持声明式上下文来源：`source_refs` 与 `context_sources`。
- 当前运行时注入由 `agentdash-injection` 负责解析，已支持 `manual_text` / `file` / `project_snapshot`。
- 当前会话输入框已有成熟的 `@文件引用` 交互：`frontend/src/features/file-reference/RichInput.tsx` 与 `frontend/src/features/file-reference/FilePickerPopup.tsx`。
- 当前后端已有仅面向工作空间文件的选择 API：`/api/workspace-files`、`/api/workspace-files/read`、`/api/workspace-files/batch-read`。
- 当前 Story 页面中对上下文文件引用的编辑是单独实现的，尚未复用会话输入框的引用选择体验。
- 当前上下文来源的 `locator` 语义仍较弱：对 `file` 类型来说本质是相对路径，但对未来其它资源类型还没有统一寻址协议。

## Assumptions (temporary)

- 第一阶段仍以“工作空间文件引用”作为最先打通的通用寻址类型。
- 前端不应直接知道所有资源类型的具体实现细节，而应读取后端暴露的“可用寻址空间能力”。
- 声明式上下文来源的数据模型会继续沿用 `ContextSourceRef` 作为领域入口，但其 `locator` 需要更清晰、可扩展的协议约束。
- 会话框引用文件逻辑可抽象成更通用的“引用选择器”能力，而不是只在 Session UI 内部复用。

## Open Questions

- 第一阶段是否需要支持“跨 Workspace / 非默认 Workspace”的地址解析，还是只覆盖当前 Task/Story 已绑定的工作空间。
- 统一寻址空间的协议名称是采用 URI 风格（如 `file://` / `mcp-resource://`），还是保留 `kind + locator` 结构并只约束 locator 格式。
- 通用能力探测接口是否应该直接暴露“provider + selector schema + read/list API”，还是只先暴露最小可用元数据。

## Requirements (evolving)

- 支持在 Story / Task 配置上下文来源时，快捷选择工作空间文件，而不是手工输入路径。
- 该交互尽量复用当前会话输入框的 `@文件引用` 检索、选择、展示能力，避免重复实现一套文件选择器。
- 上下文来源模型应支持从“文件引用”扩展到“统一寻址空间中的其它对象引用”。
- 统一寻址模型必须能表达：资源类型、地址、展示标签、可选读取策略、解析约束。
- 后端需要提供“当前环境可用的寻址空间能力”查询接口，前端据此决定可显示哪些引用入口。
- 不同环境下的寻址空间能力应可插拔，例如：
  - 有工作空间时支持 `workspace_file`
  - 有 MCP server 时支持 `mcp_resource`
  - 有项目上下文服务时支持 `project_snapshot` / `story_entity` / `task_entity`
- 运行时解析层需要和前端选择器解耦：前端只负责产出结构化引用，后端负责解释和解析。
- 新设计应兼容当前 `ContextSourceRef` 演进，不要求一次性推翻已有字段。

## Acceptance Criteria (evolving)

- [ ] 形成一份明确的统一寻址设计，说明领域模型、前后端接口、运行时解析职责边界。
- [ ] 明确 Story/Task 上下文引用与 Session 输入框引用的复用边界，避免重复建设两个选择器体系。
- [ ] 明确“能力探测接口”的输入输出结构，以及前端如何消费该接口。
- [ ] 明确第一阶段 MVP 范围、后续扩展路径与暂不实现项。
- [ ] 在任务文档中列出建议的落地拆分（PR/子任务粒度）。

## Definition of Done (team quality bar)

- 方案文档可直接指导实现，不需要再次大范围补需求。
- 涉及跨层的数据流、接口边界、责任归属均已说明。
- 复用点和抽象点明确，避免再次出现同类 UI / API 重复实现。
- 若开始进入实现阶段，可据此直接拆成前端、后端、解析层三个子任务。

## Out of Scope (explicit)

- 本轮不直接实现完整的统一寻址空间基础设施。
- 本轮不引入全文搜索、语义检索、嵌入索引等高级检索能力。
- 本轮不实现远程网页/PDF/第三方知识库抓取。
- 本轮不强行废弃当前 `/api/workspace-files/*` 接口；如需演进，优先做兼容迁移。

## Current State Analysis

### 1. 当前已有两套相邻但未统一的能力

#### A. Session 输入框文件引用链路

- 前端输入：`frontend/src/features/file-reference/RichInput.tsx`
- 选择浮层：`frontend/src/features/file-reference/FilePickerPopup.tsx`
- 读取服务：`frontend/src/services/workspaceFiles.ts`
- 后端接口：`crates/agentdash-api/src/routes/workspace_files.rs`
- 发送时构造 ACP block：`frontend/src/features/file-reference/buildPromptBlocks.ts`

这条链路的特点：

- 交互成熟，支持 `@` 触发、检索、选择、药丸展示；
- 资源模型偏“即时引用”，核心是把文件内容转成 prompt blocks；
- 地址模型是工作空间相对路径，尚未抽象为通用引用地址。

#### B. Story / Task 声明式上下文来源链路

- 领域模型：`crates/agentdash-domain/src/context_source.rs`
- Story/Task 持久化入口：`source_refs`、`context_sources`
- 解析执行：`crates/agentdash-injection/src/resolver.rs`
- 注入编排：`crates/agentdash-api/src/task_agent_context.rs`
- 页面编辑：`frontend/src/pages/StoryPage.tsx`

这条链路的特点：

- 面向“持久化声明”，而非一次性 prompt；
- 已有统一解析入口，但前端编辑体验还较原始；
- `locator` 可以承载地址，但尚未被定义为通用寻址协议。

### 2. 当前核心问题

- 文件引用选择能力被局限在 Session 输入框，没有沉淀为通用组件/协议。
- 工作空间文件 API 被硬编码为专用接口，不适合直接扩展到“统一寻址空间”。
- 前端不知道当前环境究竟有哪些可引用资源，因此无法做“按能力显示”的通用选择器。
- `ContextSourceRef.kind` 和 `locator` 之间缺少明确的协议约束，后续类型一多会变得难以维护。

## Proposed Design

### 决策原则

- **先统一地址与能力模型，再复用 UI。**
- **前端负责选择与展示，后端负责能力暴露与地址解析。**
- **保留现有 `ContextSourceRef` 入口，采用渐进演进，不做一次性重构。**
- **把“工作空间文件”视为统一寻址空间中的第一个 provider，而不是特例。**

### 设计总览

```text
AddressSpaceProvider(后端能力提供方)
    ├─ 列出可用空间 list_spaces()
    ├─ 搜索条目 search(space, query)
    ├─ 读取条目 resolve(address)
    └─ 返回 provider 元数据 / UI hint / 权限边界

Frontend Reference Picker
    ├─ 读取可用 spaces
    ├─ 选择 space
    ├─ 搜索 / 浏览 candidate
    └─ 产出统一的 ContextSourceRef 草稿

ContextSourceRef
    ├─ kind / locator（兼容现有）
    ├─ label / slot / priority / delivery
    └─ （后续可补）address_space / metadata

Injection Resolver
    └─ 根据 kind + locator 或 address provider 解析为上下文片段
```

### 方案一：在现有 `ContextSourceRef` 上渐进扩展（推荐）

#### 数据模型

第一阶段保持当前领域模型不破坏，仅收紧约束：

- `kind = file` 时，`locator` 必须是工作空间相对路径；
- `kind = project_snapshot` 时，`locator` 为逻辑范围（如 `.`）；
- 为未来扩展预留映射表：
  - `workspace_file` → `kind=file`
  - `workspace_snapshot` → `kind=project_snapshot`
  - `mcp_resource` → 新 kind
  - `story_entity` / `task_entity` → 新 kind

第二阶段可考虑显式拆出：

```ts
type AddressSpaceId = "workspace_file" | "workspace_snapshot" | "mcp_resource" | ...

interface AddressRef {
  space: AddressSpaceId;
  address: string;
  title?: string | null;
  metadata?: Record<string, unknown>;
}
```

再由 `ContextSourceRef` 组合 `AddressRef`。

#### 能力探测接口

建议新增统一接口：

`GET /api/address-spaces?project_id=&story_id=&task_id=&workspace_id=`

返回示意：

```json
{
  "spaces": [
    {
      "id": "workspace_file",
      "label": "工作空间文件",
      "kind": "file",
      "provider": "workspace",
      "searchMode": "prefix",
      "supports": ["search", "browse", "read"],
      "selector": {
        "trigger": "@",
        "placeholder": "输入文件名或路径",
        "resultItemType": "file"
      }
    }
  ]
}
```

设计要点：

- 这是“能力发现接口”，不是解析接口；
- 它回答的是“当前环境可用哪些寻址空间”；
- 不同环境由后端动态决定返回哪些空间，而不是前端写死。

#### 搜索/浏览接口

第一阶段可保留现有 `/api/workspace-files`，但在统一层外再包一层：

- `GET /api/address-spaces/{space_id}/entries?query=...`
- `POST /api/address-spaces/resolve`

其中：

- `workspace_file` provider 内部可直接复用 `/api/workspace-files` 的实现；
- 后续新增 provider 时只需实现同一接口契约。

### 前端交互设计

#### 1. 抽象出通用 Reference Picker

从现有 Session `RichInput + FilePickerPopup` 拆出两层：

- `ReferencePickerCore`
  - 只关心触发、查询、结果列表、选择回调
  - 不绑定 `FileEntry`，改为通用 `AddressCandidate`
- `WorkspaceFileReferenceAdapter`
  - 负责把 `workspace_file` provider 返回结果适配成当前文件药丸样式

这样：

- Session 输入框仍可使用 `@` 文件引用；
- Story / Task 上下文来源编辑器可复用同一套选择浮层；
- 后续扩展新的 address space，只需要补 adapter，不必重写 UI 交互。

#### 2. Story / Task 页面交互

新增“添加引用来源”入口：

- 先选择 `Address Space`
- 再搜索/选择具体条目
- 最后补充 `slot / priority / required / delivery`

对于 `workspace_file`，可以提供“快速添加”路径：

- 直接复用当前 `@` 风格检索与结果列表
- 选择后自动生成 `ContextSourceRef { kind: "file", locator: relPath, ... }`

### 后端实现边界

#### Address Space Registry

建议新增一层注册表，例如：

- `AddressSpaceProvider` trait
- `AddressSpaceRegistry`

provider 最小接口：

- `descriptor(context) -> AddressSpaceDescriptor`
- `search(context, query) -> Vec<AddressCandidate>`
- `resolve(context, address) -> ResolvedAddressContent`

其中 context 至少包含：

- project/story/task/workspace 标识
- 当前执行环境能力（是否有 workspace、是否有 MCP 等）

#### 与 `agentdash-injection` 的关系

- `agentdash-injection` 继续作为“最终解析 + 片段生成”层；
- `AddressSpaceProvider` 更偏“资源寻址与读取”层；
- 第一阶段可由 `resolve_declared_sources` 内部直接调用 address-space resolver；
- 第二阶段再把 `file/project_snapshot/...` 全部迁到 provider 化实现。

## MVP Scope

### Phase 1（建议先做）

- 新建任务与方案留档
- 定义 `address-spaces` 能力探测接口
- 抽象前端通用引用选择器核心
- 让 Story / Task 上下文来源支持复用 `workspace_file` 选择器
- 保持运行时仍通过现有 `file` / `project_snapshot` resolver 解析

### Phase 2

- 把 `workspace-files` API 包装/迁移到统一的 address space 接口
- 新增至少一个非文件空间（建议 `project_snapshot`）
- 在前端支持按 provider 展示不同候选项样式

### Phase 3

- 支持 `mcp_resource`、`story_entity`、`task_entity` 等更多 provider
- 统一 Session prompt 引用与声明式上下文来源的底层寻址协议

## Recommended Task Split

- 子任务 A：统一寻址空间领域与 API 设计
- 子任务 B：前端引用选择器通用化改造
- 子任务 C：Story/Task 上下文来源编辑器接入通用选择器
- 子任务 D：后端 Address Space Registry 与 workspace_file provider
- 子任务 E：声明式来源解析层对统一寻址空间的接入

## Technical Notes

- 现有 Session 文件引用 UI：`frontend/src/features/file-reference/RichInput.tsx`
- 现有文件选择弹层：`frontend/src/features/file-reference/FilePickerPopup.tsx`
- 现有文件读取服务：`frontend/src/services/workspaceFiles.ts`
- 现有文件 API：`crates/agentdash-api/src/routes/workspace_files.rs`
- 现有上下文来源领域模型：`crates/agentdash-domain/src/context_source.rs`
- 现有声明式来源解析：`crates/agentdash-injection/src/resolver.rs`
- 现有 Task 注入装配：`crates/agentdash-api/src/task_agent_context.rs`
- 现有 Story 页面手工编辑逻辑：`frontend/src/pages/StoryPage.tsx`

## Recommendation

推荐采用“**先统一 capability / address / picker contract，再复用文件引用 UI**”的路线。

原因：

- 如果先直接把 Session 的文件选择器硬搬到 Story 页面，只能复用一次文件场景，后面扩展资源类型仍会再拆一次；
- 如果先补一个最小可用的 Address Space 契约，就能让 Session 与 Context Source 共享同一套底层模型；
- 现有 `ContextSourceRef` 已经是不错的持久化入口，当前更缺的是“地址协议”和“能力发现”。
