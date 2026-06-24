# Agent Memory Discovery System Design

## Boundary

Memory 是运行时可发现的 VFS 资源、source inventory 和模型使用策略。系统边界保持五层：

- Host Integration 贡献 `MemoryDiscoveryProvider`。
- Frame construction 在 effective active VFS 上执行 memory discovery。
- Launch / turn preparation 将 memory inventory 转成 Agent 可见 context frame。
- VFS / inline_fs 处理真实文件读写和权限校验。
- `memory-manager` skill 指导 Agent 维护 memory 文件。

这条链路沿用现有 session startup pipeline 的单一事实源约束：进入 connector 的上下文、VFS、capability 必须来自 frame construction 产出的 launch surface。

## Default Source

默认 memory home 是 ProjectAgent Agent mount，runtime mount id 修订为 `agent`：

```text
agent://
  MEMORY.md
  topics/
    project-decisions.md
    workflows.md
    failure-modes.md
    user-feedback.md
    external-references.md
  archive/
```

底层 inline storage key 不变：

```text
owner_kind   = project_agent
owner_id     = project_agent.id
container_id = knowledge
```

`ProjectAgentKnowledge` VFS surface 继续作为 UI/API 浏览入口，内部 mount 切到 `agent`。这样 surface 描述产品入口，mount 描述 Agent 自己的文件空间。

## Discovery Contract

Memory discovery 与 Skill discovery 平行，但需要比 Skill discovery 多一个 mount-level 输入，因为 memory home 在 `MEMORY.md` 尚未创建时也应可发现。

```text
MemoryDiscoveryProvider
  provider_key()
  vfs_discovery_rules()
  discover_from_vfs(context, mounts, files)
  discover(context)
```

Host 传给 provider 的 mount 信息应是受控摘要，不包含本机绝对路径：

```text
MemoryDiscoveryMount
  mount_id
  provider
  display_name
  capabilities
  purpose
  owner_kind
  metadata_summary
```

Discovery output 只描述 source，不承载 topic 正文，也不表达独立权限：

```text
DiscoveredMemorySource
  provider_key
  source_key
  display_name
  source_uri
  index_uri
  mount_id
  scope: agent | project | user | external
  capabilities: derived from resolved mount
  format: agentdash
  index_status: missing | present | too_large | invalid
  trust_level
  summary
```

首期 first-party provider 识别 `agent` mount 并返回 `agent://` source；若 `agent://MEMORY.md` 存在且在大小上限内，provider 可附带 bounded index 摘要。未来外部 source 的 provider 也只能识别 active VFS 中已存在的 mount。

## Runtime Flow

生产路径应按以下顺序接入：

1. ProjectAgent owner bootstrap 构建 base VFS。
2. `knowledge_enabled=true` 时追加 `agent` mount。
3. 按 ProjectAgent preset 的 VFS grants 收敛项目级 mounts；Agent memory mount 保持自己的 mount capability。
4. Lifecycle / canvas / skill asset projection 继续叠加。
5. Capability resolver 产出 tool / MCP / companion 维度。
6. Skill baseline 从 effective VFS 派生。
7. Memory inventory 从同一份 effective VFS 派生。
8. Frame surface draft、context bundle 和 launch intent 写入同一份 handoff。

当前 `derive_runtime_capability_projection` 不是 ProjectAgent 生产主线的入口；实现应在 `OwnerBootstrapComposer` 或其直接调用的 helper 中显式接入 memory projection，避免只更新未消费的 helper。

## Context Injection

Memory 使用独立 `memory_context` frame 或等价 context slot，内容包括：

- memory usage policy；
- discovered source inventory；
- 默认 source：`agent://`；
- index pointer：`agent://MEMORY.md`；
- bounded index content（存在且未超限时）；
- topic read policy：正文按需通过 VFS 读取。

`MEMORY.md` 不应继续作为通用 project guideline 注入。`AGENTS.md` 保持 guideline 语义；`MEMORY.md` 迁移到 memory context，原因是它是长期经验索引，不是项目规则文件。

## Memory Manager Skill

`memory-manager` 是默认可见 skill，职责是指导 Agent 使用普通 VFS 工具维护 memory：

- 初始化 `agent://MEMORY.md` 和 `agent://topics/`；
- 判断信息是否会改变未来 Agent 行为；
- 优先更新已有 topic，再创建新 topic；
- 为 topic 写 frontmatter：`name`、`description`、`type`、`scope`；
- 写入共享 memory 前做 secret scan；
- 使用 memory 中涉及代码、配置、路径、外部事实的 claim 前先验证当前事实；
- 清理重复、过期、低置信内容。

该 skill 不定义专用工具。写入 `agent://` 仍由 VFS mount capability、edit capability 和 mutation dispatcher 决定。

## Reference Rules

Claude Code prompt 调研只作为文件式 memory 管理规则来源，详见 `research/claude-code-memory-prompts.md`。可复用的规则是短索引、topic frontmatter、高信号写入门槛、secret scan 和 stale claim 验证；这些规则进入 `memory-manager` skill，而不是成为本机路径 discovery provider。

## Validation Focus

- `agent` mount 在 active VFS 与 `CapabilityState.vfs.active` 中一致。
- `MEMORY.md` 只通过 memory context 注入，不通过 guidelines 重复注入。
- Provider 输出的 capabilities 与 resolved mount capabilities 一致。
- Provider 返回的 URI 必须是 controlled VFS URI，例如 `agent://MEMORY.md`。
- Integration registry 对 memory provider key 做空值和重复检测。
