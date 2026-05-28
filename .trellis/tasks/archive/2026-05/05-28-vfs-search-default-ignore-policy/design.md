# VFS 搜索工具默认忽略策略收束设计

## Architecture Boundary

VFS 工具入口仍保持 mount-relative 参数模型。Application 层负责解析 `mount_id://relative/path`、normalize path、校验 mount capability，然后把 list / grep 请求交给 provider。物理工作区扫描只发生在本机后端 `agentdash-local`，云端 `RelayFsMountProvider` 继续只负责协议转发。

本任务把默认忽略策略定义为本机文件发现语义，而不是业务数据库或 API DTO 语义。这样符合后端架构中“云端后端不直接访问本机文件系统，本机后端负责物理文件访问”的边界。

## Search Intent

内部引入文件发现意图，用于区分默认工作区扫描和显式 subtree 搜索：

```rust
enum FileDiscoveryIntent {
    ImplicitWorkspaceScan,
    ExplicitSubtreeScan,
}
```

推导规则：

- `path` 为空、`.` 或解析为 workspace root 时使用 `ImplicitWorkspaceScan`。
- `path` 明确指向 workspace root 下的子路径时使用 `ExplicitSubtreeScan`。

`ImplicitWorkspaceScan` 应用工作区 ignore、内置噪音目录和 VCS hard exclude。`ExplicitSubtreeScan` 将入口 subtree 视为用户目标，允许进入普通 ignored subtree；递归期间仍应用 VCS hard exclude 与 workspace 边界。

## Ignore Classes

### Workspace Ignore

来自工作区 ignore 文件的普通忽略语义，覆盖 `.gitignore` / `.ignore`。本机存在 ripgrep 时，grep 可利用 ripgrep 的 ignore 解析；本机 list / fallback search 应使用统一 helper 解析或模拟同一行为。

原因：默认工作区扫描的主要目标是源码与可维护文件，ignore 文件体现了项目对“搜索时低价值内容”的事实源。

### Builtin Noise Ignore

内置噪音目录用于覆盖常见依赖、构建和缓存目录：

```text
node_modules
target
dist
build
.next
.venv
__pycache__
```

原因：这些目录经常没有被所有项目完整写进 ignore 文件，但它们会显著污染 agent 搜索上下文。

### Hard Exclude

VCS 元数据目录保持强制排除：

```text
.git
.svn
.hg
.bzr
.jj
.sl
```

原因：这些目录是版本控制内部状态，不是 Agent 默认阅读或搜索的工作区内容；它们也容易产生大量无意义或高噪音结果。

## Local Backend Implementation Shape

`crates/agentdash-local/src/tool_executor.rs` 是主要落点：

- 抽出 `FileDiscoveryPolicy` 或等价 helper。
- `file_list` 根据 `path` 推导 `FileDiscoveryIntent`，目录递归时通过 policy 剪枝。
- fallback search 复用同一 policy，不再维护独立目录黑名单。
- ripgrep search 路径显式补齐 VCS hard exclude；显式 ignored subtree 时使用能进入目标 subtree 的 ripgrep 参数。

优先保持 relay 协议不扩展。`ToolFileListPayload.path` / `ToolSearchPayload.path` 已能表达显式 subtree 搜索，local backend 可以根据 path 自行推导 intent。

## Tool Description

`fs_glob` 与 `fs_grep` 描述需要表达：

- 默认从 mount root 扫描会应用 workspace ignore 和内置噪音目录策略。
- 需要检查依赖、构建产物或被普通 ignore 覆盖的目录时，显式传入 `path` 指向该 subtree。
- VCS 元数据目录默认不作为搜索目标。

## Trade-offs

不新增用户可见参数让工具 schema 保持简洁，也符合预研阶段“直接收束正确状态”的项目约束。代价是暂时没有一个显式 escape hatch 用于搜索 VCS 元数据目录；该能力若未来有真实调试需求，应单独设计为非常明确的工具能力，而不是借普通 path 误入。

使用 local backend 推导 intent 可以避免扩大 relay 协议面。代价是 future provider 若不通过 local backend，需要各 provider 对齐同一 provider contract；这应通过 VFS provider 文档或 trait helper 固化。

## Validation Strategy

测试集中在 `agentdash-local`，因为真实物理扫描在那里发生。工具层补充描述和必要的 handler 单测即可。验证命令优先使用窄范围 cargo test，最后按风险补 `cargo check -p agentdash-local` 或相关 crate check。
