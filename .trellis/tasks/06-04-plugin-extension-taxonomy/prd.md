# PRD · 插件/扩展概念收束与分层模型

## 背景

项目内"插件 / 扩展（plugin / extension）"一词被多个抽象层共用，语义重叠，导致沟通与设计混乱。需要先收束命名与边界，再规划一个可持续扩展的分层模型。本任务**先做方向澄清与方案探讨，不立即实现**。

参照系：`references/codex` 的三层模型
- **能力原语**：skills / mcpServers / apps(connectors) / hooks，各自独立可用
- **plugin = 纯数据打包单元**：`plugin.json` 捆绑若干原语 + `interface` 展示元数据，本身不含编译进宿主的原生代码
- **marketplace = 分发与生命周期**：本地/curated/远端来源，install / upgrade / remove / 远程同步 / 版本固定 / 启停开关 / 能力归因(plugin_id)

## 确认事实（已查证）

本仓库"插件/扩展"当前跨 3 层、≥7 概念：

| 概念 | 位置 | 注册方式 | 所属层 |
|---|---|---|---|
| AgentDashPlugin | crates/agentdash-plugin-api/src/plugin.rs | 原生 Rust trait，编译期 bootstrap 汇总 | 宿主能力注入（受信） |
| Connector / Executor | crates/agentdash-spi/src/connector/mod.rs | list_executors()，内置或经 plugin | 能力原语 |
| Executor/LLM Bridge | agentdash-executor/.../bridges | DB llm_providers 表 | 能力原语（硬编码 bridge） |
| Workflow / Activity | crates/agentdash-contracts/src/workflow.rs | 领域实体，静态编排，不可插 | 编排 |
| Skill | crates/agentdash-spi/src/platform/skill.rs | VFS mount + extra_skill_dirs() | 能力原语 |
| MCP Server | crates/agentdash-mcp | session 级声明 | 能力原语 |
| Extension Package | crates/agentdash-application/src/extension_runtime.rs | manifest(JSON) + library_asset_seeds + DB 安装 | 数据驱动包 |

核心歧义：
- `AgentDashPlugin` = **编译期原生宿主扩展**（注册 connector/auth/mount/vfs/skill-dir/library-seed），其 trait 注释自承多数扩展点"仍处实验阶段，未接入稳定宿主链路"。
- `Extension Package` = **数据驱动可安装包**（commands/flags/renderers/tabs），才是真正对应 codex `plugin` 的层。
- 二者共用"插件/扩展"一词，是首要待掰开的歧义。

对照 codex 后识别的 gap：
1. 缺"打包单元"把原语捆成一个可安装物（现为 5 套独立注册路径）。
2. 缺 marketplace / 分发 / 版本生命周期（现为编译期 seed + DB 安装记录）。
3. 缺内容声明式（非原生）的生命周期 hook 面（现 hook_rules/workflow 为静态编排）。
4. 缺统一能力归因 + 跨原语冲突解决（仅 connector 有 DuplicateExecutorId）。
5. 缺展示元数据作为一等公民（原生 plugin 无 interface，对 UI 不可见）。
6. 缺 native(受信编译期) vs. content(数据驱动) 两层的明确分界与信任模型。
7. 缺可安装包的路径安全/manifest 校验（codex 强制 ./ 相对、拒绝 ..）。

## 目标与用户价值

（待澄清后填写）

## 需求

（待澄清后填写）

## 验收标准

（待澄清后填写 — 须可测试/可检验）

## 不在范围内

（待澄清后填写）

## 阻塞规划的开放问题

- Q1（进行中）: 本任务的产出形态——纯概念收束文档 vs. 延伸到落地/迁移实现规划。
