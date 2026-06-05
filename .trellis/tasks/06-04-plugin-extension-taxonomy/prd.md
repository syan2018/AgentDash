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

## 已对齐的方向（讨论确认）

收束模型 = **两层 × 两种"作者 × 信任"姿态**，分别适用 codex 模型的不同子集：

### 第一层 · 企业能力接入（受信 / 编译期 / 深集成）
- 例子：Auth、企业 KM mount、内网 agent 后端 connector。
- 作者：核心团队 / 部署接入部门；信任来自**部署方权威**。
- 形态：**编译期绑定，明确不做动态加载**。
- 接口的真正价值：**让第三方仓用 upstream 方式跟主干，不必硬 fork**——下游维护依赖开源核心的私有集成 crate，只碰标准化扩展接口，永不 patch core。
- 当前载体：`AgentDashPlugin` trait（`builtin_plugins()` 静态装配）已是此接缝。
- 质量标准（= 可 upstream 目标的实现）：
  - 接口**完整**：私有 crate 能纯靠接口完成 Auth/mount/connector 等，不伸手进 core。
  - **闭环实验扩展点**：`source_resolvers` / `external_service_clients` / `routine_trigger_providers` 等"声明了却未接入宿主链路"的点必须真接通，否则下游被迫 patch core → 被逼 fork。
  - 接口 **semver 稳定**，upstream 升级不轻易破坏下游。
- 收束命名：**此层不叫"插件"**（它是宿主受信装配，非对外插件合同）。

### 第二层 · 拓展插件（零信任 / 数据驱动 / 浅内容）
- 例子：skill、命令、hook、UI 渲染、默认 prompt、runtime action。
- 作者：**任何人可开发贡献**；信任来源：**零信任（作者不可信）**。
- 对应 codex 的 `plugin + marketplace` 全套。
- 当前载体：`ExtensionTemplatePayload`（manifest v2）+ `ProjectExtensionInstallation` + `library_asset_seeds`。
- 现状（已查证，比预期成熟）：已支持 commands / flags / message_renderers / capability_directives / runtime_actions / protocol_channels / workspace_tabs / extension_dependencies / bundles / **permissions（含 `evaluate_action_permission` 裁决引擎、`capability_family` 分类、`requires_package_artifact()` 区分纯声明 vs. 带代码）**。
- **结构性缺口**：manifest 能扩"交互/UI/动作"面，但**不能贡献 agent 能力原语 skill / mcp-server / hook**；这些原语目前只能走第一层或 VFS mount / session 声明，未被"包"统一捆绑与归因。

## 目标与用户价值

产出一份可被后续重构反复引用的 **canonical taxonomy + 命名收口 + 边界/gap 取舍** 决策文档，消除"插件/扩展"跨层歧义。**本任务只产出决策，不实现、不归档**；实现另起 parent + children。

收束后的语言（按绑定作用域）：
- **Integration**（宿主/部署，受信原生，编译期，upstream 接缝）
- **Extension**（Project 全局，数据驱动，扩展工作台 UI 面，不挂 agent 上下文）
- **Capability Pack / 能力包**（单 Agent，数据驱动，引用清单式捆绑 skill/mcp/workflow + interface + permissions，= codex plugin 对应物）
- **Shared Library** = 分发/归属伞（marketplace 角色）；"Plugin" 一词退役。

## 需求

- R1 命名收口：确立上述四词定义与边界，"Plugin" 退役；第一层 `AgentDashPlugin` 语义改名为 Integration（符号级改名可分阶段）。
- R2 组织模型：采用 provenance 正交维度（模型 1），原语保持 canonical 注册表，包通过来源戳实现冲突/归因/卸载。
- R3 能力包：定义为新 `LibraryAssetType::CapabilityPack`，引用清单式（不内嵌），主挂 Agent 定义、session 覆盖。
- R4 分发信任：Curate 优先；路径/manifest 校验从第一天加；git 源/沙箱/签名留后续阶段。
- R5 一体两面：明确 package（分发面，export/install）与 provenance（运行时面）为同一身份两投影。
- R6 gap 与 rollout：沉淀结构性缺口清单 + 建议的 child 拆分与落地次序（路线草图，不在本任务执行）。

## 验收标准

- [ ] design.md 含完整四词 taxonomy + 与 codex 三层、与现有 7 概念的映射表。
- [ ] 命名错位诊断（Plugin/Extension/Shared Library 三者错位）有据可查、结论明确。
- [ ] Q2–Q7 全部决策落档，每条含理由与被否选项的代价。
- [ ] 结构性缺口清单 + 建议 child 拆分/rollout 次序成文。
- [ ] 用户过目确认终稿。
- [ ] 任务**不归档、不进入实现**；实现由后续 parent 承接。

## 不在范围内

- 动态加载 / 热插拔第一层原生能力（已明确否决）。
- 任意 git 源 / 运行时沙箱 / 签名 / 远程同步（留后续阶段）。
- 任何代码改动、符号级改名、child 任务创建（实现期事项）。

## 阻塞规划的开放问题

- Q1: 产出形态——纯概念收束文档 vs. 延伸实现规划（**待用户拍**，倾向先出决策文档）。
- Q2（已定）: 采用"模型 1 · provenance 正交"组织原语与包；原语保持 canonical，包是来源维度。存储分发形态选 (c) 混合（轻包 DB / 重包文件 bundle）。详见 design.md §2、§1.6。
- Q3（已定）: 命名按**绑定作用域**三分——**Integration**(宿主) / **Extension**(Project 全局 UI) / **Capability Pack 能力包**(Agent 级能力)；"Plugin" 退役；Shared Library 作分发伞。第一层 Plugin→Integration 已确认。详见 design.md §4。
- Q4（已定）: 能力包主挂 **Agent 定义/模板**（持久装备），session 级做覆盖。详见 design.md §4b。
- Q5（已定）: 方案 **A**——新增 `LibraryAssetType::CapabilityPack`，为"引用清单"manifest（引用 skill/mcp/workflow asset + interface + permissions），不内嵌。详见 design.md §4b。
- Q6（已定）: **(a) Curate 优先,分阶段**。信任来自人工 curation + 现有 permission 声明;路径/manifest 校验第一天就加;git 源/远程同步/运行时沙箱/签名留后续阶段。
- Q7（已定）: 收为**决策文档**；本任务**不归档、不进入实现**，留在原地持有 canonical taxonomy；实现另起 parent + children。
