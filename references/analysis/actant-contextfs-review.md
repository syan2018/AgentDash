# Actant (ContextFS) 仓库深度 Review

**项目：** https://github.com/blackplume233/Actant  
**分析日期：** 2026-03-27  
**分析重点：** 核心架构设计、工程治理、可借鉴模式

---

## 1. 项目定位

Actant 是一个**面向 AI Agent 的上下文文件系统（ContextFS）**。核心理念：

> 把 Agent 可感知的一切上下文——静态文件、运行时状态、进程输出、控制指令——统一建模为可寻址、可操作、有生命周期的文件系统对象。

技术栈：TypeScript monorepo (pnpm workspace)，18 个包，Node.js ≥ 22。

---

## 2. 核心架构

### 2.1 四层分栈

```
Contracts Layer  (@actant/shared)           — 共享类型、错误、RPC 基础设施
       ↑
VFS Stack        (@actant/vfs)              — 唯一文件系统内核
       ↑
AgentRuntime     (agent-runtime, domain-context, acp, pi, channel-*, tui)
       ↑
Surface Stack    (api, cli, rest-api, dashboard, mcp-server, actant)
```

依赖方向严格单向，通过 `stack-boundaries.mjs` + CI 测试自动化执行。

### 2.2 VFS 内核三问

| 问题 | 解决方案 |
|------|---------|
| 路径如何被解释？ | `mount namespace` — 规范化、最长前缀匹配 |
| 子树如何被接入？ | `mount type` (root/direct) + `filesystem type` (hostfs/runtimefs) |
| 对象最终是什么？ | `node type` — directory / regular / control / stream |

### 2.3 请求流

```
Caller → Mount Namespace (canonicalize) → Mount Table (resolve) → Middleware (permission) → Node Layer → Backend
```

---

## 3. 核心设计亮点

### 亮点 1：Linux VFS 隐喻的彻底贯彻

只有 4 种 node type：
- `directory` — 可枚举子节点
- `regular` — 稳定快照内容
- `control` — 写入触发副作用（类 `/proc` 控制文件）
- `stream` — 有序 chunk 流（stdout/stderr）

运行时不是独立平台宇宙，而是 VFS 下的一棵 `runtimefs` 伪文件系统子树：
- `/agents/<name>/status.json` → regular node
- `/agents/<name>/control/request.json` → control node（写入 = 发指令）
- `/agents/<name>/streams/stdout` → stream node

### 亮点 2：Consumer Interpretation 与内核严格分离

VFS 内核永远不决定文件用途。同一个 `.md` 文件可以同时被当作 Markdown、Skill、SQL 模板、配置消费。这避免了内核承担业务语义的常见陷阱。

### 亮点 3：自动化边界门禁

`stack-boundary-gate.test.ts` 在 CI 中：
1. 验证每个包都被分类到某个 stack
2. 验证 package.json 依赖不违反 stack 规则
3. 检测跨 stack 循环依赖
4. 扫描源码 import 确保不越界
5. 验证 bridge 包不直接组装 kernel 内部对象

`contextfs-terminology-gate.test.ts` 用 grep 门禁禁止旧术语回流。

### 亮点 4：MountFS 插件化

每种 filesystem type 是独立包 (`@actant/mountfs-*`)，通过 `FilesystemTypeDefinition<TConfig>` 接口注册：
- `create(config, mountPoint, lifecycle)` → `VfsMountRegistration`
- `validate(config)` → 配置校验
- `defaultFeatures` → 特性声明

新增 filesystem type 不需要修改 VFS 内核。

### 亮点 5：Middleware Chain

VFS Kernel 的请求分发采用 middleware chain，Permission 是一个 middleware，可以在不修改内核的情况下插入审计、限流、日志等横切关注点。

### 亮点 6：精细生命周期管理

6 种 lifecycle 类型：daemon / agent / session / process / ttl / manual。不同来源的挂载有不同生存策略。

### 亮点 7：Channel Protocol (ACP-EX)

自有 Agent 通信协议，核心原则：
- 可选性：只有 `prompt` 是 required
- 对称性：Host 和 Backend 都可主动发起操作
- 后端能力释放：`adapterOptions` / `backendOptions` 透传
- ACP 兼容：Core Profile 与 ACP 语义一致，扩展用 `x_` 前缀

### 亮点 8：Permission 模型

7 种 principal（owner/self/agent/archetype/parent/any/public），支持路径模式（含 `${self}` 变量展开）和优先级。

### 亮点 9：RuntimefsProviderContribution SPI

泛型 SPI 接口 `RuntimefsProviderContribution<TRecord, TStreamName, TWatchEvent>`，让任何运行时以统一方式向 VFS 贡献数据。

### 亮点 10：工程治理成熟度

- GitNexus 代码知识图谱（9324 符号、17983 关系）
- Trellis 工作流系统
- 20+ QA 场景库（JSON 格式）
- Endurance Testing 耐久性测试
- dependency-cruiser 依赖分析
- Terminology Gate 术语门禁

---

## 4. 包结构一览

| 层级 | 包 | 说明 |
|------|---|------|
| Contracts | `@actant/shared` | 共享类型、错误、RPC |
| VFS | `@actant/vfs` | 唯一内核 |
| MountFS | `mountfs-workspace`, `mountfs-process`, `mountfs-runtime-agents`, `mountfs-runtime-mcp` | 各类 filesystem type 实现 |
| Runtime | `agent-runtime`, `domain-context`, `acp`, `pi`, `channel-claude`, `tui` | 执行、协议、UI |
| Surface | `api`, `cli`, `rest-api`, `dashboard`, `mcp-server`, `actant` | 对外入口 |

---

## 5. 关键类型

```typescript
// 挂载注册 — 完整描述一个挂载实例
interface VfsMountRegistration {
  name: string;
  mountPoint: string;
  label: string;
  features: ReadonlySet<VfsFeature>;
  lifecycle: VfsLifecycle;
  metadata: VfsMountMetadata;
  fileSchema: VfsFileSchemaMap;
  handlers: VfsHandlerMap;
}

// 生命周期 — 6 种类型
type VfsLifecycle =
  | { type: "daemon" }
  | { type: "agent"; agentName: string }
  | { type: "session"; agentName: string; sessionId: string }
  | { type: "process"; pid: number; retainSeconds?: number }
  | { type: "ttl"; expiresAt: number }
  | { type: "manual" };

// 节点类型 — 4 + 1 保留
type VfsNodeType = "directory" | "regular" | "control" | "stream" | "symlink";

// 文件系统类型
type VfsFilesystemType = "hostfs" | "runtimefs" | (string & Record<never, never>);
```

---

## 6. 设计哲学总结

1. **Everything is a file** — Linux 哲学在 Agent 上下文领域的彻底应用
2. **Kernel stays minimal** — 内核只做路径/挂载/节点/操作，业务语义全部外推
3. **Boundaries are enforced, not documented** — 架构边界通过自动化测试守护
4. **Protocol is optional-first** — 通信协议以能力协商为核心
5. **Freeze before extend** — 先冻结基线，再在冻结基础上扩展

---

## 7. 对 AgentDashboard 主仓库的借鉴分析

以下基于 Actant 的设计亮点，对照 AgentDashboard 当前实现，识别出值得借鉴的具体方向。

### 借鉴 1：Address Space 应升级为真正的 VFS 内核

**Actant 做法：** VFS Kernel 是唯一的路径解析 + 操作分发中心。`mount namespace → mount table → middleware → node → backend` 是一条完整的请求链路。所有操作（read/write/list/stat/watch/stream）都经过同一个 `dispatch()` 入口。

**AgentDashboard 现状：** `RuntimeAddressSpace` 和 `RuntimeMount` 已经有了 mount 的概念，但它们只是**数据结构**（DTO），不是执行内核。实际的 read/write/list/exec 操作分散在 `execution_hooks.rs`、`address_space_access.rs`、`runtime_bridge.rs` 等多个地方，没有统一的分发入口。`AddressSpaceProvider` 只负责 descriptor/discovery，不负责实际 I/O。

**建议：** 考虑引入一个 `AddressSpaceKernel`（或 `MountDispatcher`），把 `RuntimeMount` 从纯数据结构升级为可执行的挂载实例，统一 read/write/list/search/exec 的分发路径。这样 `relay_fs` 和 `inline_fs` 就变成了两种 "filesystem type"，而不是散落在各处的 if-else 分支。

### 借鉴 2：引入 Middleware Chain 做横切关注点

**Actant 做法：** VFS Kernel 的 middleware chain 让 permission、audit、rate-limit 等横切关注点可以无侵入地插入。

**AgentDashboard 现状：** `execution_hooks.rs` 是一个 2900+ 行的巨型文件，把 workflow 推进、权限检查、上下文注入、诊断收集等逻辑全部混在一起。`AppExecutionHookProvider` 实现了 `ExecutionHookProvider` trait 的所有方法，每个方法内部都有大量的 resolve → check → build 逻辑。

**建议：** 考虑把 `ExecutionHookProvider` 的职责拆分为 middleware chain 模式。例如：
- `WorkflowAdvanceMiddleware` — 负责 lifecycle step 推进
- `PermissionMiddleware` — 负责权限检查
- `ContextInjectionMiddleware` — 负责上下文注入
- `DiagnosticMiddleware` — 负责诊断信息收集

这样每个关注点独立可测试，`execution_hooks.rs` 也不会继续膨胀。

### 借鉴 4：Node Type 概念引入 Mount 体系

**Actant 做法：** 4 种 node type（directory/regular/control/stream）让 VFS 能统一表达静态文件和运行时控制面。Agent 的 `control/request.json` 写入 = 发送指令，`streams/stdout` 订阅 = 获取输出流。

**AgentDashboard 现状：** `MountCapability` 只有 5 种（Read/Write/List/Search/Exec），没有区分"读取静态内容"和"订阅实时流"，也没有"写入即触发副作用"的 control 语义。Task 的执行控制和状态读取走的是完全不同的 API 路径（REST endpoint vs WebSocket），没有统一到 mount 体系中。

**建议：** 考虑在 `MountCapability` 中增加 `Stream` 和 `Control` 能力，让 Task 的执行输出（stdout/stderr）和控制指令（start/stop/cancel）也能通过 mount 体系表达。这样前端可以用统一的方式访问"文件内容"和"执行输出"。

### 借鉴 6：Filesystem Type Registry 模式

**Actant 做法：** `FilesystemTypeRegistry` 让新增 filesystem type 只需实现一个 `FilesystemTypeDefinition<TConfig>` 接口并注册，不需要修改内核。

**AgentDashboard 现状：** `ContextContainerProvider` 是一个 enum（`InlineFiles` / `ExternalService`），新增 provider 类型需要修改 enum 定义和所有 match 分支。这是一个封闭的扩展模型。

**建议：** 考虑把 `ContextContainerProvider` 从 enum 改为 trait object 注册模式（类似 `AgentDashPlugin` 的 `agent_connectors()` 模式）。这样企业插件可以注册自定义的 context provider（如企业 KM、文档中心），而不需要修改核心 enum。

### 借鉴 7：Protocol 的 Capability 协商模式

**Actant 做法：** Channel Protocol 的每个 Backend 通过 `ChannelCapabilities` 声明自己支持的能力，Host 在调用前检查。所有 optional 方法都有对应的 capability flag。

**AgentDashboard 现状：** `ConnectorCapabilities` 已经有了类似的设计（`supports_cancel`、`supports_discovery` 等），但粒度较粗。`AgentConnector` trait 的方法没有和 capability 严格对应——有些方法在 trait 上是 required 的，但实际上某些 connector 不支持。

**建议：** 这一点 AgentDashboard 已经在正确的方向上。可以进一步细化，让 `AgentConnector` 的每个 optional 操作都有对应的 capability flag，并在调用前做运行时检查，而不是让不支持的 connector 返回 `Err`。


---

## 8. 总结：优先级排序

| 优先级 | 借鉴项 | 预期收益 | 实施难度 |
|--------|--------|---------|---------|
| P0 | 自动化架构边界门禁 | 防止依赖腐化，成本极低 | 低 |
| P0 | Middleware Chain 拆分 execution_hooks | 解决 2900 行巨型文件，提升可维护性 | 中 |
| P1 | Address Space 升级为执行内核 | 统一 I/O 分发，消除散落的 if-else | 高 |
| P2 | Node Type 引入 | 统一静态/动态内容访问模型 | 高 |
| P2 | Filesystem Type Registry | 开放式 provider 扩展 | 中 |

---

*文档基于代码分析生成，反映 Actant 仓库截至 2026-03-27 的实际实现*
