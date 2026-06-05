# Design · 插件/扩展概念收束与分层模型

> 本文是讨论沉淀 + 技术设计草图。任务定位为"先收束概念、出决策底座"，不立即实现。

## 1. 讨论梳理（决策轨迹）

按对话推进，已对齐的判断：

1. **codex 没有单一"plugin 系统"**，而是刻意三层：能力原语 → plugin(纯数据打包单元) → marketplace(分发/生命周期)。原语各自独立，plugin 只负责"捆绑 + 展示 + 归因"，marketplace 只负责"分发 + 生命周期 + 启停"。
2. **本项目"插件/扩展"跨 3 层、≥7 概念**，根因是把两个正交轴塞进一个层级。
3. **两层 × 两种"作者×信任"姿态**已锁定：
   - 第一层（企业能力接入）：受信 / 编译期 / 深集成；作者=核心团队 or 部署接入部门；**明确不做动态加载**；接口价值=让第三方仓用 upstream 跟主干、不必硬 fork。
   - 第二层（拓展插件）：零信任 / 数据驱动 / 浅内容；**任何人可贡献**；对应 codex 的 plugin+marketplace 全套。
4. **组织关系采用"模型 1：provenance 正交"**（非容器/聚合）：原语保持 canonical 注册表不动，"包"是正交的来源维度，装载时把贡献展开进各注册表并盖来源戳。
5. **plugin 是一体两面**：分发时是可 export/install 的真实包（manifest + payload）；运行时是盖在原语上的 provenance 投影。export/install 操作"包"，冲突/归因/卸载靠 provenance 戳。
6. **存储/分发形态选 (c) 混合**：纯声明轻包走 DB(Shared Library)，带 artifact 重包走文件 bundle（呼应已有 `requires_package_artifact()`）。
7. **关键事实更正（查证后）**：真正的打包/分发层早已存在 = **Shared Library**；"Extension"只是其六种 asset type 之一，并非打包伞。详见 §3。
8. **决定性轴：绑定作用域（binding scope）。** Extension 不挂 agent 上下文 → 天然 Project 级全局（VS Code 式工作台 UI 扩展，codex 无对应物）；能力包挂 agent 上下文 → 天然 Agent 级（codex plugin 的真正对应物）。二者作用域不同、扩展表面不同，**不能合并为一个名字**（否决"只用 Extension 一个词"的方向）。
9. **三概念按作用域定名（见 §4）**：Integration(宿主) / Extension(Project) / Capability Pack 能力包(Agent)；"Plugin" 退役。
10. **第一层 Plugin → Integration 已确认**（用户拍板）。

## 2. 目标分层模型（收束后）

```
轴 A · runtime kind（agent 真正消费）
   connector · skill · mcp-server · workflow(≈codex hook) · vfs-mount
        ▲                                              ▲
        │ 原生贡献(受信/编译期)                          │ 数据贡献(零信任/可安装)
   [第一层] Host Provider                          [第二层] Shared Library Asset
   (现 AgentDashPlugin)                            (现 Shared Library + Extension)
        └──────────────── 轴 B · provenance/scope ───────────────┘
              (现 LibraryAssetScope: Builtin/System/Org/User)
```

- 原语注册表原地不动；每个实例携带 `provenance { source_kind, source_id, version }`。
- "包" = 枚举若干原语贡献 + interface + permissions 的 manifest；装载=展开+注册+盖戳；卸载=按戳清扫。
- provenance 一处解锁四事：冲突裁决（按 source rank）、生命周期（按戳清扫）、归因/UI/@mention、信任门（按 source_kind）。

## 3. 命名错位诊断（本轮核心产出）

代码实测，两层命名：
- 第一层（native/编译期/受信）→ **Plugin**：`agentdash-plugin-api`、`agentdash-first-party-plugins`、`AgentDashPlugin`。
- 第二层（data/可安装/零信任）→ **Extension**：`extension_runtime/_management/_package`、`ExtensionTemplatePayload`、`project_extensions`、`extension_package_artifacts`。
- 真正的打包/分发伞 → **Shared Library**：`LibraryAssetType{Agent,McpServer,Workflow,Skill,VfsMount,Extension}Template` + `LibraryAssetScope{Builtin,System,Org,User}`。

**错位结论（按严重度）：**

1. **"Plugin" 给了最不像插件的东西（主要错位）。** 生态共识里 plugin ≈ "可安装 add-on"；这里却指"编译期原生、不可安装"的宿主代码。听到 plugin 的人预期正好相反。对照 codex：codex 的 `plugin` = 可安装数据包（≈本项目第二层），codex 的原生 crate 本体**根本不叫 plugin**。→ 本项目把"plugin"贴在了 codex 不会贴的那一层。
2. **"Extension" 味道对、但 scope 错。** ≈ VS Code "Extension"（可安装内容包），方向没错；问题是它被摆成"六种 LibraryAssetType 之一"的窄类型，读者误以为它是"扩展机制总称(伞)"，而真正的伞是 Shared Library。
3. **真正的打包/分发/归属层（Shared Library）既不叫 plugin 也不叫 extension**，导致全系统三个名字横跨两层，且"伞"是最不显眼的那个。`LibraryAssetScope` 已经是一种现成的 provenance 维度，强力印证模型 1 可落地（基础设施已部分存在）。

**对照映射：**

| codex | 本项目现状 | 收束后建议 |
|---|---|---|
| 能力原语 skill/mcp/hook/app | skill/mcp/workflow/connector/vfs（散在各注册表） | 不变，canonical 原语 + provenance 戳 |
| plugin（可安装数据包） | 部分 = Extension(窄)；缺"异构捆绑包" | 第二层"可安装包"=广义 Library bundle |
| marketplace（分发/生命周期） | **Shared Library**（已存在！） | 保留 Shared Library 作分发伞 |
| codex 的 crate 本体（不叫 plugin） | **AgentDashPlugin**（误叫 plugin） | 第一层正名，去掉 "plugin" |

## 4. 命名收束（按绑定作用域，已基本对齐）

三概念，作用域 host → project → agent 一一对应，无近义冲突：

| 概念 | 作用域 | 定义 | 现载体 |
|---|---|---|---|
| **Integration** | 宿主/部署 | 受信原生能力提供者（Auth/mount/connector/vfs）；编译期；upstream 接缝 | 现 `AgentDashPlugin`（待改名） |
| **Extension** | Project 全局 | 数据驱动、可安装；扩展工作台 UI 面（命令/渲染器/tab/channel/runtime action）；不挂 agent 上下文 | 现 `ExtensionTemplatePayload` 等（语义收窄确认，名保留） |
| **Capability Pack / 能力包** | 单个 Agent | 数据驱动、可安装；捆绑 agent 能力原语（skill/mcp/workflow）+ interface + permissions + provenance；= codex plugin 对应物 | **新增**（复用 Shared Library 分发） |

- **"Plugin" 退役**：第一层让出该词后不再复用，避免与 Extension 再造近义混用。
- **Shared Library 留作分发/归属伞**（= marketplace 角色），Extension 与 Capability Pack 均为其 asset type。
- **第一层改名 `Integration`**（用户确认）；符号级改名可分阶段（先 spec/文档对齐语义，再改 crate/类型名）。

## 4b. 能力包的承载与绑定（Q4/Q5 已定）

查证：`AgentTemplateConfig` 现含 `executor/provider/model/system_prompt/permission_policy` + `capability_directives` + **`mcp_slots`（AgentMcpSlotTemplate: key/description/required，引用式晚绑定）**；skill/workflow 当前未在 agent 定义中被引用。各原语（skill/mcp/workflow）已各自是可安装的单类型 LibraryAssetType。

- **绑定目标（Q4）**：能力包主挂 **Agent 定义/模板**（持久装备，复用语义）；session 级只做覆盖/临时增减。
- **承载方式（Q5）= 方案 A**：新增 `LibraryAssetType::CapabilityPack`，本质是**"引用清单"manifest**——引用若干已有 skill/mcp/workflow asset + interface + permissions + provenance，**不重新内嵌原语**。
  - 对齐 codex plugin（manifest 指向 skill 目录/mcp 配置）。
  - 复用现有单类型 asset 作原子；包是上层捆绑信封。
  - 契合现有 slot 晚绑定文化：AgentTemplate 引用能力包，与引用 mcp_slot 同构。
  - 定义：**能力包 = "agent 模板的能力半边"，去掉 model/executor 绑定，做成可复用可安装独立物**；AgentTemplate = 引用若干能力包 + 补 model 配置。

## 5. 结构性缺口（gap，已精确定位）

1. Shared Library 能分发单一 typed 原语模板（skill/mcp/workflow/vfs）+ UI 复合(extension)，但**缺"一个可安装单元捆绑多个异构原语实例 + 共享 interface/provenance"**的概念（codex plugin 的核心）。
2. install 缺"展开原语贡献 → 注册进各表 → 盖 provenance 戳"链路，及配套冲突/卸载清扫。
3. 零信任来源的 workflow/mcp 贡献缺**按 activity executor 危险面分级的权限门**（声明式低危先开，Function/bash 执行后开）。
4. 第一层接口的实验扩展点（`source_resolvers`/`external_service_clients`/`routine_trigger_providers`）未闭环 → 下游被迫 patch core。
5. 接口 semver 稳定性约束未确立（影响 upstream 可跟随性）。

## 6. 建议的 child 拆分 + rollout 次序（路线草图，本任务不执行）

实现期建议起一个 parent，下挂以下可独立验收的 child；按风险面/依赖排序：

1. **命名与语义对齐（先文档后符号）**：在 .trellis/spec 固化四词 taxonomy；第一层 `AgentDashPlugin`→`Integration` 符号级改名（crate/类型/日志）。低风险、解锁后续表述。无运行时行为变化。
   - 验收：spec 落地；符号改名后全绿编译；无残留 "Plugin" 指代第一层。
2. **provenance 维度落地**：给原语实例（skill/mcp/workflow 注册）加 `provenance{source_kind,source_id,version}`；冲突裁决按 source rank（builtin>integration>mount>pack，pack 不得覆盖 builtin）。
   - 验收：同名跨源有确定性裁决 + 单测；可按 source 查询/分组。
3. **CapabilityPack 资产类型**：新增 `LibraryAssetType::CapabilityPack`（引用清单 manifest：refs + interface + permissions）；manifest/路径校验（`./` 相对、拒 `..`）。
   - 验收：可定义/校验一个引用 skill+mcp+workflow 的包；非法路径被拒。
4. **install 展开链路**：装包 = 解析 refs → 注册进各原语表 → 盖 provenance 戳；卸载 = 按戳清扫；升级 = 重展开。
   - 验收：装/卸/升级后原语集合与归因正确；幂等。
5. **AgentTemplate 引用能力包**：`AgentTemplateConfig` 增 `capability_pack_refs`（与 mcp_slots 同构）；session 级覆盖。
   - 验收：agent 解析时能力包展开为 effective 能力；session 覆盖生效。
6. **第一层扩展点闭环**：把 `source_resolvers`/`external_service_clients`/`routine_trigger_providers` 等实验点真接入宿主链路，或显式标注未支持；确立 SPI/plugin-api 的 semver 约束。
   - 验收：私有集成 crate 能纯靠接口完成既定能力，无需 patch core；接口稿评审。

跨 child 依赖：2 依赖 1 的语义定稿；3/4 依赖 2；5 依赖 3/4；6 相对独立可并行。依赖写进各 child artifact，不靠树位置隐含。

## 7. 待后续阶段（明确缓做）

- 开放分发：任意 git 源、远程同步、签名、运行时沙箱（Q6 选 curate 优先后顺延）。
- Extension 与 CapabilityPack 是否共享更多 manifest 公共段（interface/permissions 复用）——实现期再看。
