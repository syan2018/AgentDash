# 统一 Address Space 与 Agent 访问层规划

## Goal

规划一套统一的 Address Space / Agent 访问层，让 Agent 的运行时工具访问、声明式上下文来源解析、多工作空间访问，以及非物理工作空间（例如云端 KM 的文件化视图）都建立在同一套抽象之上，而不是继续分散在多套协议和实现里。

本任务的输出目标不是直接完成全部实现，而是产出一版能指导后续落地的设计稿和 code-spec，并据此拆出分阶段实施任务。

## Background

近期云端后端与本机后端已经开始拆边界，并形成了几条相近但尚未统一的链路：

- 第三方 Agent Task 通过 relay 下发 `command.prompt` 到本机 backend
- 声明式 `File / ProjectSnapshot` 来源通过 relay 的 `command.workspace_files.*` 读取目标 workspace
- 本机 backend 已经具备 `command.tool.*` 对应的 ToolExecutor
- PiAgent / 原生 Agent 运行时仍主要依赖进程内 `BuiltinToolset`

这说明项目已经具备“统一访问层”的基础部件，但当前仍存在几个明显问题：

- 运行时工具访问和上下文注入不是同一套抽象
- 多 workspace 的 prompt 执行根目录语义尚未完全收口
- relay `command.tool.*` 已定义，但云端原生 Agent 尚未真正走这条链路
- 非物理 workspace（如 KM、快照、远程资源）缺少统一的“文件式访问”模型

如果继续按场景补丁式推进，后续很容易演变成：

- `workspace_files` 一套
- `tool.*` 一套
- PiAgent 内置工具一套
- context provider/source resolver 一套
- KM / 资源视图再长出第五套

因此需要先有一个统一规划任务，明确后续实现的底层抽象和迁移路径。

## Current State Summary

### 当前实际存在的四条访问链路

1. **第三方 Agent Prompt 链路**
   - 云端按 `Task.workspace_id -> Workspace.backend_id` 路由到目标本机
   - 通过 `command.prompt` 下发整段任务
   - 本机 ExecutorHub 启动 Claude Code / Codex / 其他第三方 Agent

2. **声明式上下文 workspace 读取链路**
   - `File / ProjectSnapshot` 通过 `command.workspace_files.read/list` 读取目标 workspace
   - 主要用于 Story / Task 上下文注入

3. **PiAgent tool call 链路（协议已定义，发送侧未闭环）**
   - relay `command.tool.*` 已定义
   - 本机 ToolExecutor 已实现
   - 但云端原生 Agent 仍未真正通过这条链路发起工具调用

4. **PiAgent 本地内置工具链路**
   - 运行时仍使用 `BuiltinToolset::for_workspace(...)`
   - 与 relay、本机 ToolExecutor、context source 并非同一套抽象

### 当前最关键的实现缺口

- 本机 `command.prompt` 接收到的 `workspace_root` 尚未真正传入本机执行器的工作目录解析逻辑
- 本机 `ToolExecutor` 与 `workspace_files` 在路径规范化和越界防护上还需要进一步收紧
- 云端原生 Agent 尚未切换到“provider + relay tool transport”的统一模型

### 与当前代码核对后的结论（2026-03-18）

以下结论已对照当前代码确认，不再只是设计推测：

- `workspace_root` 目前确实没有真正控制第三方 Agent 的执行根目录
  - cloud `relay_start_prompt(...)` 已把 `workspace.container_ref` 填入 `CommandPromptPayload.workspace_root`
  - local `command_handler` 收到后只记录日志，然后直接调用已有 `ExecutorHub`
  - `ExecutorHub` 仍基于构造时固定的 `self.workspace_root` 解析 `working_dir`
  - local 启动时 `ExecutorHub::new(...)` 仍使用 `accessible_roots.first()` 作为固定根目录

- 本机路径边界目前至少还有两个真实缺口
  - `workspace_files.read` 当前使用 `root.join(payload.path)` 后直接做 `starts_with(root)`，检查前没有 canonicalize，`..` 场景会被误判
  - `ToolExecutor::resolve_path` 对不存在的目标直接返回 `full.clone()`，只有目标已存在时才会做 canonical 越界校验，因此写入新文件时仍存在逃逸风险

- `Address Space` 在代码里已经有一个“轻量雏形”，但还不是本任务目标里的统一访问底座
  - `agentdash-injection::AddressSpaceProvider` 目前只负责返回 descriptor，供 `/api/address-spaces` 暴露可用空间
  - 它还不承担 `read / write / list / search / exec` 访问责任
  - 后续实现时需要明确这是“扩展现有 trait”还是“抽新一层 provider”，避免同名语义混淆

- declared source / context provider 目前仍未真正走统一 provider
  - `agentdash-injection::resolver` 仍直接使用 `std::fs` / `walkdir`
  - `task_agent_context` 与 `acp_sessions` 当前会先把 `File / ProjectSnapshot` 过滤掉，再以 `workspace_root = None` 调用通用 resolver
  - 这说明“声明式来源解析”和“workspace_files relay 读取”仍是分叉实现

- PiAgent runtime tools 尚未切到统一 provider 模型
  - `PiAgentConnector` 仍直接通过 `BuiltinToolset::for_workspace(context.working_directory.clone())` 注入本地内置工具
  - 这条链路还没有接到 relay `command.tool.*` / 本机 `ToolExecutor`

- 协议层的资源定位字段仍是分裂状态
  - `command.prompt` / `command.tool.*` 使用 `workspace_root`
  - `command.workspace_files.*` 使用 `root_path`
  - 说明项目还没有真正统一到 `mount + relative path` 的定位契约

## Requirements

- 定义统一的 Address Space 抽象，能够表达：
  - 本地物理 workspace
  - 多 workspace 同时挂载
  - 非物理 workspace，例如 KM / Snapshot / 只读资源空间
- 定义 Agent 运行时应使用的统一资源访问模型，避免“prompt 执行”和“tool call 访问”继续分裂
- 明确运行时工具最小集合，以及它们与底层访问抽象的关系
- 设计会话级 mount / workspace 绑定模型，让 Agent 可以显式访问多个挂载空间
- 明确哪些挂载支持 `read/write/list/search`，哪些支持 `exec`
- 规划与现有 `context provider`、`declared sources`、`source resolver` 的融合方式
- 规划与现有 relay 协议、本机 ToolExecutor、PiAgent 内置工具之间的迁移边界
- 给出非物理 workspace“文件化 warp”方案，但不强行承诺完整 POSIX 语义
- 明确安全边界：路径校验、写权限、跨 mount 隔离、执行权限约束

## Acceptance Criteria

- [x] 输出一版统一的 Address Space 领域模型与 provider 接口草案
- [x] 输出一版运行时工具模型，说明为何采用“小而稳定的标准工具集”而不是单一万能工具
- [x] 明确会话级 mount 方案，以及 Agent 如何访问多个 workspace / 非物理空间
- [x] 明确 context provider / declared source / runtime tool 三者如何共享同一套访问底座
- [x] 明确 relay、本机 backend、云端原生 Agent 各自的职责边界
- [x] 明确第一阶段最小可落地范围，以及后续阶段拆分建议
- [x] 明确需要先修复的当前阻塞项（例如 workspace_root 落地、路径安全校验）
- [x] 形成可继续拆分为实施任务的规划文档，而不是停留在概念层

## Design Principles

### 原则 1：不要先设计“万能工具”

推荐方向不是设计一个 `universal_tool(action=...)`，而是：

- 先建立统一 Address Space / Provider 抽象
- 再在其上暴露稳定的小工具集合，例如 `read / write / list / search / exec`

原因：

- 小工具更容易被模型稳定调用
- 参数模型更清晰，便于权限控制和错误语义约束
- 更容易扩展到多 mount / 非物理 workspace

### 原则 2：统一资源定位方式

推荐把资源定位统一成：

- `mount + relative path`

而不是继续让 Agent 感知：

- `backend_id + absolute path`
- 或不同工具各自定义的 `workspace_root` / `working_dir` / `root_path`

### 原则 3：relay 是 transport，不是新的领域模型

relay 应只负责：

- 把“对某个 mount 的访问请求”路由到对应本机
- 承担 transport / request-response / streaming 责任

不应让上层业务逻辑直接与 `RelayMessage` 深度耦合。

### 原则 4：context provider 与 runtime tool 必须共享底座

上下文注入和运行时工具访问本质上都在做“读取或操作某个资源空间”。  
两者不应长期维护两套实现。

### 原则 5：非物理 workspace 采用受限 VFS 语义

KM / Snapshot / 远程资源应优先承诺：

- `read`
- `write`
- `list`
- `search`
- `stat`

而不是伪装成完整 POSIX 文件系统。

## Target Architecture

### 1. 统一的核心对象

```rust
struct Mount {
    id: String,                 // main / repo-a / km / snapshot
    provider: String,           // relay_fs / km / snapshot / local_fs
    root_ref: String,           // workspace_id / uri / km_space_id / container_ref
    capabilities: CapSet,       // read / write / list / search / exec
    default_write: bool,
    display_name: String,
}

struct ResourceRef {
    mount_id: String,
    path: String,               // 相对 mount 根路径
}
```

### 2. Provider 抽象

```rust
#[async_trait]
trait AddressSpaceProvider {
    async fn read(&self, target: &ResourceRef, opts: ReadOpts) -> Result<ReadResult>;
    async fn write(&self, target: &ResourceRef, content: WriteContent) -> Result<WriteResult>;
    async fn list(&self, target: &ResourceRef, opts: ListOpts) -> Result<ListResult>;
    async fn search(&self, query: SearchQuery) -> Result<SearchResult>;
    async fn stat(&self, target: &ResourceRef) -> Result<StatResult>;
    async fn exec(&self, req: ExecRequest) -> Result<ExecResult>;
}
```

### 3. Session 级挂载表

每个 Task / Story / Session 在启动时都生成一份 mount table，例如：

- `main`：当前 Task 绑定 workspace，可读写可执行
- `spec`：共享规范仓，只读
- `km`：知识库空间，可读写不可执行
- `snapshot`：历史快照，只读

Agent 运行时只面对这些 mount，不再直接感知 `backend_id`。

## Runtime Tool Model

### 推荐的小工具集合

- `mounts.list`
- `fs.read`
- `fs.write`
- `fs.list`
- `fs.search`
- `shell.exec`

公共定位参数采用：

```json
{
  "mount": "main",
  "path": "crates/agentdash-api/src/app_state.rs"
}
```

执行命令采用：

```json
{
  "mount": "main",
  "cwd": ".",
  "command": "cargo test -p agentdash-api"
}
```

### 为什么不建议单工具多 action

- 模型需要先决定 action 再决定参数结构，稳定性更差
- 容易形成弱类型协议，错误语义模糊
- 权限控制会变得复杂

## Context Integration

### 统一后的职责划分

#### declared sources / context provider

- 只负责把声明式来源解析成 `ResourceRef` 或 `MountQuery`
- 不直接拼接 relay command

#### AddressSpaceProvider

- 负责真正访问目标资源
- 对物理 workspace 可走 relay / local fs
- 对 KM / Snapshot / 资源库可走对应 provider

#### runtime tools

- 对 Agent 暴露稳定的工具接口
- 工具内部调用同一套 provider

### 效果

同一份资源可以同时被：

- context 注入读取
- Agent 运行时访问
- 前端只读浏览

而不再需要三套平行实现。

## Non-Physical Workspace Warp

### 目标

让 KM / Snapshot / 资源空间在 Agent 侧呈现为“类文件空间”，但不要求完整文件系统语义。

### 推荐做法

- 每个非物理空间都实现自己的 provider
- 在 session mount table 中以 mount 形式挂入
- 对 Agent 暴露相同的 `fs.read/write/list/search` 接口

### 不承诺的语义

- symlink
- chmod
- watch
- file lock
- shell.exec
- 原子 rename

## Roles & Boundaries

### 云端

- 管理 mount 规划和 session 级访问视图
- 解析 `Task.workspace_id -> Workspace.backend_id`
- 选择 provider / transport
- 统一 context resolver 与 runtime tool 的入口

### 本机 backend

- 提供物理 workspace 的执行环境
- 负责本地路径合法性与安全边界
- 通过 ToolExecutor / 文件访问适配器执行 provider 请求

### relay

- 作为云端访问本机 provider 的 transport
- 不直接承载更上层的 mount 语义

### PiAgent / 原生 Agent

- 不直接依赖 `BuiltinToolset::for_workspace(...)`
- 应依赖 provider-backed tools

## Migration Strategy

### Phase 0: 先修当前阻塞项

- 修复本机 prompt 执行真正 honor `workspace_root`
- 收紧本机路径安全校验
- 明确第三方 Agent 和云端原生 Agent 各自当前真实链路

### Phase 1: 抽统一 Provider 接口

- 优先改造 context source / declared source
- 让 `workspace_files.read/list` 先走统一 provider 接口
- 暂不要求立即变更对外协议

### Phase 2: 引入 mount table

- 为 Task / Story / Session 生成 mount 列表
- 上下文注入与 runtime tool 都只消费 mount 视图

### Phase 3: runtime tool 切换

- PiAgent runtime tools 改成 provider-backed tools
- relay `command.tool.*` 成为 `relay_fs_provider` 的 transport 细节

### Phase 4: 非物理 workspace 接入

- 接入 KM provider
- 接入 Snapshot provider
- 让它们共享同一套 mount / tool / context 模型

## Gap List（设计目标 vs 当前实现）

| 主题 | 当前实现 | 与目标架构的差距 | 建议阶段 |
|------|----------|------------------|----------|
| prompt 执行根目录 | `workspace_root` 已透过 relay 下发，但 local ExecutorHub 仍固定根目录 | 第三方 Agent 无法真正按 Task 绑定 workspace 执行 | Phase 0 |
| 本机路径安全 | `workspace_files` 与 `ToolExecutor` 各自做校验，且都存在越界空隙 | 缺少统一的 mount/path 校验与能力边界 | Phase 0 |
| Address Space provider | 现有 trait 只做 descriptor 暴露 | 缺少统一的读写/搜索/执行 provider 接口 | Phase 1 |
| declared sources | 仍是本地 `std::fs` / `walkdir` 解析 | 与 relay workspace 读取、runtime tool 不能共享底座 | Phase 1 |
| session mount | 还没有 mount table，运行时仍以单 workspace 绝对路径为主 | 无法显式表达多 workspace / 非物理空间 | Phase 2 |
| PiAgent runtime tool | 仍依赖 `BuiltinToolset` | 没接到 provider-backed tools / relay transport | Phase 3 |
| 非物理 workspace warp | 仅停留在设计层 | KM / Snapshot 尚无统一文件化访问模型 | Phase 4 |

## First Implementation Slice

第一阶段最小可实现范围建议为：

1. 修复 `workspace_root` 落地
2. 补齐本机路径安全校验
3. 定义 `Mount / ResourceRef / AddressSpaceProvider`
4. 让 declared source 的 workspace 读取改走 provider 接口
5. 暂不改动前端协议和 KM warp

## Deliverables

- 一份可执行的架构设计说明
- 一份推荐的数据模型 / trait 草案
- 一份迁移顺序与任务拆分建议
- 一份当前实现与目标架构之间的 gap list

## Recommended Task Breakdown

建议把后续落地拆成以下六个任务，并保持依赖顺序清晰：

1. `fix-local-prompt-workspace-root-binding`
   - 目标：让 relay 下发的 `workspace_root` 真正成为本机第三方 Agent 执行根目录
   - 完成标准：`command.prompt.workspace_root` 不再只是日志字段，`working_dir` 基于该根目录解析

2. `harden-local-path-boundary-validation`
   - 目标：统一本机 `workspace_files` 与 `ToolExecutor` 的路径规范化、越界防护与错误语义
   - 完成标准：绝对路径、`..`、不存在父目录写入等场景都不能逃逸出 mount 根目录

3. `extract-address-space-provider-core`
   - 目标：抽出真正的 `Mount / ResourceRef / AddressSpaceProvider` 核心模型
   - 完成标准：provider 不再只是 descriptor，而是具备可复用的访问接口与能力矩阵

4. `migrate-declared-sources-to-provider`
   - 目标：让 `File / ProjectSnapshot` 与相关 declared source 读取统一改走 provider
   - 完成标准：context provider 与 relay workspace 访问共享同一套底座

5. `replace-pi-agent-builtin-tools-with-provider-tools`
   - 目标：把 PiAgent runtime tools 从 `BuiltinToolset` 切换到 provider-backed tools
   - 完成标准：runtime tool 的资源定位统一为 `mount + relative path`

6. `design-km-provider-warp`
   - 目标：为 KM / Snapshot 等非物理空间定义受限 VFS warp
   - 完成标准：非物理空间能以 mount 形式接入，但不伪装成完整 POSIX 文件系统

依赖关系建议为：

- `fix-local-prompt-workspace-root-binding` + `harden-local-path-boundary-validation`
  - 先完成这两个 Phase 0 阻塞项，再推进抽象层统一
- `extract-address-space-provider-core`
  - 是 `migrate-declared-sources-to-provider` 与 `replace-pi-agent-builtin-tools-with-provider-tools` 的前置
- `design-km-provider-warp`
  - 放在 provider / mount 模型稳定之后推进

## Follow-up Task Candidates

- `fix-local-prompt-workspace-root-binding`
- `harden-local-path-boundary-validation`
- `extract-address-space-provider-core`
- `migrate-declared-sources-to-provider`
- `replace-pi-agent-builtin-tools-with-provider-tools`
- `design-km-provider-warp`
