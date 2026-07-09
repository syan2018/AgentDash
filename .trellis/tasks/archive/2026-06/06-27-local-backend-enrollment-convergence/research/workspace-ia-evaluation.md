# Workspace 设置区 IA 评估（Phase 2.0 gate 产出）

> 目的：在动 UI 前，把现状概念负担说清，给出围绕用户心智的目标分组与文案，供确认后落地。本评估只定 IA 与文案方向，不含逐组件实现细节。

## 1. 现状（4 并列面板，术语负担过重）

`packages/app-web/src/pages/ProjectSettingsPage.tsx`（1482 行）「工作空间」tab 下并列 4 块：

| 面板 | 组件 | 现状文案/术语 | 暴露的概念 |
|---|---|---|---|
| Backend Access | ProjectSettingsPage.tsx 402-669 | 「Backend Access」「选择 backend」「绑定 Backend」「Inventory」「解绑」「priority {n}」「已启用/已暂停/已撤销」 | backend、access grant、inventory、priority、status |
| 本机 Workspace 发现 | LocalWorkspaceDiscoveryPanel.tsx (476) | 「本机 Workspace 发现」「选择本机 backend」「发现本机 Workspace」「可发现 identity」「候选」「一键绑定」 | discovery、identity、candidate、binding、confidence、root_ref |
| 工作空间 | WorkspaceList.tsx (268) | 「工作空间」「代码来源」「目录绑定」「目录解析：就绪/需注意/不可用」「Project 默认」 | workspace identity、binding、resolution、default |
| Workspace Modules | WorkspaceModulesPanel.tsx (108) | 「Workspace Module」「Extension/Canvas/Builtin」「operations」「UI entries」「诊断」 | module、operations、UI entries |

**问题**：用户被迫一次性理解 backend / access / inventory / priority / discovery / identity / candidate / binding / resolution / module 十余个术语；中英混排；「发现」与「绑定」「工作空间」分散在不同面板需来回跳转；Module 是只读诊断却占主线。

## 2. 用户真实心智（修正：平台不关注「代码」概念，关注「在哪台机器上跑」）

> 用户反馈关键修正：本平台**不以「代码/代码来源」为中心组织心智**；重点是**清楚地表达「在哪台机器上运行」**，并且必须能**一眼区分「用户自己的这台机器」与「服务器 runner」**。

普通用户的问题序列：

1. **这个项目能在哪台机器上跑？** 其中**哪台是我自己的电脑（这台设备）**，哪些是服务器 runner？怎么把一台新服务器接进来？
2. **工作空间落在哪台机器的哪个目录？**（落点，尽量自动）
3. （高级）模块/诊断信息。

「这台设备 = desktop local runtime（`registration_source=desktop_access_token`）」与「服务器 runner（`runner_registration_token`）」的区分，正好由本任务后端收束的 `registration_source` 提供事实源——UI 应直接据此打标，而非让用户从状态推断。

## 3. 目标 IA（现状 → 目标映射）

顶层三区：**运行环境 / 工作空间 / 高级**（沿用「工作空间」叫法，不改名为「代码空间」）。

| 用户问题 | 现状对应 | 目标区块 | 关键文案改写 | 复杂后台词去向 |
|---|---|---|---|---|
| 能在哪台机器跑 / 哪台是我的 | Backend Access 面板 | **「运行环境」**：机器列表 + 在线状态，**显式标识「本机（这台设备）」徽标** vs「服务器 runner」 | 「Backend Access」→「运行环境」；「绑定 Backend」→「添加机器/接入」 | grant/priority/inventory 收入条目展开区或「高级」 |
| 接入新服务器 | （前端缺失） | 运行环境区子块 **「接入新服务器」** | Runner token：建/列/轮换/撤销 + 一次性明文 + 复制 `agentdash-local setup` 命令 | token_prefix/secret 等仅在必要处 |
| 工作空间是哪个 | 工作空间列表 | **「工作空间」**（沿用现名） | 弱化「代码来源」主语，状态用人话；identity 标签（Git/P4/本地目录）保留为次要属性 | — |
| 落在哪台机器哪个目录 | 本机发现面板 + binding + resolution | 收进工作空间条目内 **「在某机器上定位」** + 「可用机器：本机 ✓ / 服务器 A 未定位」内联 | 去掉独立「发现」面板的来回跳转 | candidate/confidence/root_ref 进展开「目录详情」 |
| 高级/诊断 | Workspace Modules | **降级到「高级/诊断」折叠区** | 保持只读 | operations/UI entries 维持 |

## 4. 落地边界（硬约束）

- **不新增回退/断链**：`workspaceRouting` / `runtimeDiagnostics` 既有行为与测试不退化。
- 改动集中在：IA 分组、术语文案、「发现→绑定」就地化、token 子块融入。
- **不做**：多 project grant 的完整授权管理 UI（priority/policy/跨 owner/审计反向视图）—— 归独立任务 `06-27-runner-multi-project-access`。
- 与后端身份去 project 化解耦：UI 通过既有 `ProjectBackendAccess` / backend list 契约读取，de-projectization 让「可用机器」语义更干净但不阻塞前端开发。

## 5. 决策与待确认

- ✅ 顶层命名：**运行环境 / 工作空间 / 高级**（2026-06-27 确认）。
- ✅ 心智中心：**机器/运行落点**，非「代码」；运行环境区须显式标识「本机（这台设备）」vs「服务器 runner」。
- 默认执行（如无异议）：本次将独立「发现」面板就地化收进工作空间条目动作（保留弱化入口）；Workspace Modules 折叠进「高级」区，暂不移出工作空间 tab。落地中如遇成本过高的交互，降级为最小改动并记录，复杂部分归 `06-27-runner-multi-project-access` 或后续。
