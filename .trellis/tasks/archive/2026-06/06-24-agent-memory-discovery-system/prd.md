# Agent Memory Discovery System

## Goal

为 AgentDash 建立一套通用的 Agent Memory 发现、识别、注入和管理体系。Memory 是运行时可发现的文件系统资源与提示词约定，不是新的业务实体；读写继续使用现有 VFS / inline_fs 工具和 mount capability。

默认 memory home 是当前 ProjectAgent 自己的 Agent mount，目标 URI 为 `agent://`。同一项目内多个用户使用同一个 ProjectAgent 时共享这份 memory；不同 ProjectAgent 默认拥有各自独立的 memory。

## Confirmed Facts

- `ProjectAgent.knowledge_enabled` 已存在，领域语义是 Agent 跨 session 知识库，按 ProjectAgent 隔离。
- Agent knowledge 的 inline storage key 是 `owner_kind = project_agent`、`owner_id = project_agent.id`、`container_id = knowledge`。
- 当前 mount id 是 `agent-knowledge`；后端 builder、mount purpose detection、前端知识库浏览入口都引用该字符串。
- `ProjectAgentKnowledge` VFS surface 已存在，适合作为 UI/API 的知识库浏览入口；surface source name 描述的是产品入口，不必跟 runtime mount id 同名。
- 当前 ProjectAgent 正式运行路径不会自动调用 `append_agent_knowledge_mounts`，`knowledge_enabled=true` 的 Agent mount 尚未进入 active runtime VFS。
- `inline_fs` mutation 已通过 VFS capability、edit capability 与 inline storage key 分发，适合直接承载 markdown memory 文件。
- Host Integration API 是项目标准扩展入口；Skill discovery 已通过 `AgentDashIntegration::skill_discovery_providers()`、VFS discovery rules 和 runtime skill baseline 接入运行时。
- 新 frame construction 主线当前只调用 `derive_runtime_skill_baseline`，没有调用完整 `derive_runtime_capability_projection`；Memory projection 需要接入生产 owner bootstrap 路径。
- 当前通用 guideline 发现规则会扫描 `MEMORY.md`。正式 memory 系统落地后，`MEMORY.md` 应归入 memory discovery/context frame，避免同一文件同时作为 guideline 与 memory index 注入。
- Claude Code 仅作为 prompt 与文件布局参考；本任务的默认 source 聚焦 ProjectAgent `agent://`，原因是本机目录 discovery 会引入额外宿主路径授权面，当前没有对应产品需求。

## Product Decisions

- 运行时 mount id 从 `agent-knowledge` 改为 `agent`，保持底层 `container_id = knowledge`。
- `ProjectAgentKnowledge` surface source name 保持不变，只把该 surface 内可见 mount 从 `agent-knowledge` 切到 `agent`。
- 首期 memory source 是 ProjectAgent Agent mount；integration SPI 保留未来 provider 接入点，但默认体验不依赖外部 provider。
- Memory discovery 只描述 source inventory、index pointer、格式和置信信息；写权限由源 mount 的 VFS capability 派生。
- Agent 维护 memory 的能力通过 `memory-manager` skill 表达，skill 指导 Agent 使用普通 VFS 工具读写 `agent://` 文件。

## Requirements

- Memory discovery 通过 Host Integration 装配，与 Skill discovery 使用同类 provider 收集、冲突检测和运行时 projection 模式。
- 发现模型必须支持 mount 级 source 识别，即使 `agent://MEMORY.md` 尚不存在，也能把可写 memory home 暴露给 Agent 创建索引。
- 默认文件布局为 `agent://MEMORY.md` 加 `agent://topics/*.md`；`MEMORY.md` 是短索引，topic 文件承载正文。
- Runtime prompt 注入 memory policy、source inventory、默认 index pointer 和可选的 bounded index 内容；topic 正文由 Agent 按需读取。
- 运行时可见的 `agent://` mount 必须随 `knowledge_enabled` 开关出现或消失。
- `memory-manager` skill 需要覆盖保存价值判断、topic 整理、索引更新、secret scan、stale fact 验证和复用边界。
- 文件大小、frontmatter、重复 source、空 index、过期 claim 等问题应以 diagnostic 或 skill 指引表达，不改变 FS 权限模型。

## Acceptance Criteria

- [ ] `knowledge_enabled=true` 的 ProjectAgent runtime active VFS 包含 `agent://` mount，且 mount 具备 read/write/list/search。
- [ ] `knowledge_enabled=false` 的 ProjectAgent runtime active VFS 不包含 `agent://` mount。
- [ ] ProjectAgent 知识库浏览入口使用 `agent` mount，后端、前端、测试中不存在陈旧 `agent-knowledge` 字符串。
- [ ] Memory discovery provider 通过 Host Integration 注册与收集，provider key 冲突 fail fast。
- [ ] Memory source inventory 可从 active VFS 的 `agent` mount 派生，且不需要预先存在 `MEMORY.md`。
- [ ] Memory context frame 或等价注入路径只注入 policy、inventory、index pointer 和 bounded index，不默认注入 topic body。
- [ ] `MEMORY.md` 从通用 guideline 注入语义迁移到 memory 语义，避免重复注入。
- [ ] `memory-manager` skill 可被默认发现/注入，并且只依赖普通 VFS 工具。
- [ ] 权限测试证明 memory discovery 不能让不可写 mount 获得写入能力。

## Scope Boundaries

- 首期以 ProjectAgent 内部长期记忆为 MVP，因为它已经有云端 inline_fs owner 坐标和项目内跨用户复用价值。
- 外部 memory source 只保留 SPI 形状，原因是目前没有真实外部来源需要默认接入；未来接入时也应先成为普通 VFS mount，再由 provider 识别。
- Claude Code 参考只沉淀为 `memory-manager` 的提示词和文件组织规则，避免为了读取本机 `~/.claude` 引入宿主根目录授权面。

## Review Point

正式实现前需要确认本计划按上述默认决策推进，特别是：`ProjectAgentKnowledge` surface source name 保持不变，仅 rename runtime mount id / URI 为 `agent`。
