# PRD · 外部市场来源接入规划

## 背景

项目已经形成 `Integration` / `Shared Library` / `Marketplace` / `Project Asset` 的分层：

- `Integration` 是宿主/部署级受信扩展点，用于企业接入 Auth、mount、connector、内嵌 Shared Library asset 等能力。
- `Shared Library` 是公共配置资产的存储、权限、版本和安装事实源。
- `Marketplace` 是用户浏览、发现、导入和安装资产的产品界面。
- `Project Asset` 是运行时真正消费的资源，Session / AgentFrame 不直接读取市场 payload。

本任务要规划一个新的 **Marketplace Source / 市场来源发现层**：允许 Host Integration 通过源码级接入对接企业分发服务，例如企业 Skill 分发、企业 MCP Registry、内部工具目录。外部来源负责发现和拉取候选资产；资产进入运行路径前仍需转换成类型化 `LibraryAsset`，再安装为 Project 资源。

## 用户价值

- 部署方可以通过企业版源码接入把内部 Skill / MCP 分发服务接入 AgentDash，不需要维护 fork 或改核心 Marketplace 页面。
- 用户能在同一个资源市场中浏览内置资产、用户发布资产和外部来源候选资产。
- 外部资产导入后复用现有安装、版本提示、来源状态、Project 资源编辑和审计模型。
- Skill / MCP 市场先落地，后续可自然扩展到 Capability Pack。

## 确认事实

- `crates/agentdash-integration-api` 已是企业仓与开源核心之间的唯一受信契约面。
- `AgentDashIntegration::library_asset_seeds()` 已能让 Integration 在启动期贡献内嵌 Shared Library 资产。
- `Shared Library` 已规定运行路径只读取安装后的 Project 资源，不直接消费 `LibraryAsset.payload`。
- Project Marketplace 页面已支持按 asset type 浏览、安装、更新覆写和 source-status 展示。
- Skill 已有 `RemoteSkillSource::fetch(url)` 端口和 `HttpRemoteSkillSource` 实现，支持 GitHub / ClawHub / skills.sh 的单 Skill URL 导入。
- MCP Preset 已有类型化 DTO、transport config、probe 和 Marketplace 发布/安装安全约束。
- 现有远端 Skill 能力是“按 URL 导入单项资源”，缺少“外部市场目录 list/search/detail”的发现层。
- 本轮确认：首期来源治理收敛为源码级 Host Integration 接企业分发服务，来源集合跟随企业版发布节奏审查、部署和回滚。

## 目标

R1. 定义 Marketplace Source 作为外部市场发现抽象，支持 Host Integration 声明一个或多个企业分发来源。

R2. 外部来源以标准 listing/detail 暴露候选资产，首批覆盖 `skill_template` 与 `mcp_server_template`。

R3. 外部资产导入必须经过后端 typed validator，写入 `LibraryAsset(source = remote_imported)` 或等价远端来源标识，再复用现有 install API 安装到 Project。

R4. Marketplace 前端支持从外部来源浏览、搜索、查看详情、导入并安装资产。

R5. Skill 市场来源复用现有远端 Skill fetch / file typing / size limit / `SKILL.md` 校验能力，新增 catalog discovery 职责。

R6. MCP 市场来源只提供无密钥模板和参数 schema；用户安装时填写连接参数，后端继续拒绝 credential、header、env、本机路径、localhost 或私网 URL 等不适合发布/导入的连接材料。

R7. 外部来源刷新只影响候选目录和可更新提示，不静默修改 Project 资源。

R8. 后续 Capability Pack 以同一 Source / Listing / Import 管线扩展，本任务先把接口形状留出资产类型扩展空间。

## MVP 范围

- Source registry：聚合来自 Host Integration 的企业市场来源描述。
- External marketplace API：列出来源、搜索候选资产、读取候选详情、导入候选资产。
- Skill catalog：外部来源返回 Skill listing，导入时拉取文件并创建 `skill_template` LibraryAsset。
- MCP catalog：外部来源返回 MCP template listing，导入时创建 `mcp_server_template` LibraryAsset。
- Marketplace UI：外部来源筛选、详情抽屉、导入/安装主流程。
- 验证：provider key 冲突、非法 payload 拒绝、Skill 文件限制、MCP 连接材料安全约束、导入后 install/source-status 闭环。

## 后续阶段

- 外部市场自动同步 Project 资源。
- 任意未审来源的动态执行代码。
- 签名、沙箱、远程自动升级。
- Capability Pack 实装。
- Native Integration 动态加载。
- User-scope source 管理：需要单独设计用户级来源权限、可见性和导入安全边界。
- 配置式 HTTP catalog：需要单独定义配置发布、密钥管理和来源审计模型。

这些能力依赖 Source / Import 基线稳定后再独立推进，原因是它们分别引入同步、执行、信任治理、用户级来源管理和 Agent 级能力组合的新边界。首期把来源治理收在源码级 Integration，能让企业分发服务跟随企业版发布节奏审查、部署和回滚。

## 验收标准

- [ ] `design.md` 明确 Marketplace Source 与 Integration、Shared Library、Project Asset、runtime surface 的边界。
- [ ] `design.md` 给出 Skill 与 MCP 两类外部来源的数据流、关键 DTO / trait 草图和安全校验点。
- [ ] `implement.md` 拆分出可独立验收的 child tasks，并标明依赖顺序。
- [ ] 规划明确外部来源导入后继续走 Shared Library install/source-status，不形成第二套安装状态。
- [ ] 规划明确外部 MCP 导入的连接材料处理规则，避免市场 payload 持有用户私密或本机绑定配置。
- [ ] 规划包含最小验证矩阵，覆盖后端 contract、application validation、前端 Marketplace flow。
- [ ] 用户确认 MVP 的来源注册策略后，任务可进入 child 创建或实现准备。

## 已定决策

Q1. MVP 的来源注册策略：只支持源码级 Host Integration 声明系统级/企业级市场来源。

理由：这个能力主要服务企业分发服务接入，变更本身应跟随企业版源码和部署发布节奏；来源集合属于平台治理面，首期由 Host Integration 装配面统一治理。

## 开放问题

Q2. 企业分发服务在 AgentDash 内的版本关系应采用哪种模型：导入后只把资产复制成本地 Shared Library asset，还是保留企业分发服务为可查询的上游版本源，用于 Marketplace 显示“远端有新版本”？

推荐答案：首期保留企业分发服务为上游版本源，只做显式刷新/检查，不自动更新 Project 资源。原因是企业分发服务的价值不只是首次导入，还包括统一发布和版本发现；但 Project 运行事实仍应由本地安装版本控制，避免远端变化静默影响运行。
