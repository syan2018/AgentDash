# Workspace 管理页交互与文案优化

## Goal

修复「新建 Workspace / 工作空间管理」相关页面的交互不顺、状态刷新不及时、对外文案与命名晦涩等问题，提升面向用户（含非技术用户）的可用性与一致性。

## 涉及范围（文件）

- `packages/app-web/src/pages/ProjectSettingsPage.tsx`（父页面，工作空间 Tab、Backend Access 面板、刷新编排）
- `packages/app-web/src/features/workspace/workspace-list/WorkspaceList.tsx`（列表与抽屉宿主）
- `packages/app-web/src/features/workspace/workspace-list/WorkspaceListEditor.tsx`（1092 行：create/detail/detect/register/delete/badges）
- `packages/app-web/src/stores/workspaceStore.ts`（store：create/update/delete/detect）
- `packages/app-web/src/features/workspace/model/workspaceRouting.ts`（展示模型与摘要工具）

## Confirmed facts（已核实）

- 项目**无 i18n 库**（package.json 无 i18next/react-intl 等），文案均为内联硬编码。统一术语需直接改字符串或新增一个轻量术语常量模块。
- `projectStore.updateProjectConfig` 会把响应写回 `projects`（`projects.map`），因此「设为 Project 默认」切换后徽章能正常刷新。**刷新问题主要集中在 backend 健康/上下线驱动的可用性**，而非默认工作空间。
- `BackendAccessPanel` 已通过 `backendRuntimeSignature` 监听 backend 上下线重载；`WorkspaceList` 未做同样订阅，导致同页两块区域状态不一致。
- create 路径 `onCandidatesChanged()` 为 fire-and-forget（未 await 即 onClose）。
- `message` state 同时承载成功/信息/校验错误，统一以 `text-destructive`（红字）渲染 —— 正向反馈被显示为报错。
- detect+register UI 在编辑器内出现两份近乎相同的拷贝（create 的 local_detect 区块 与 Backend 路由的「登记新的可用目录」折叠区）。

## Requirements（草案，待确认优先级与范围）

### P0
- R1 修复成功/信息消息被渲染成错误红字：按 success/info/error 区分语气与颜色。
- R2 对外文案与命名体系统一：消除中英混杂的实现术语（逻辑 Workspace / 落点 / backend/root / binding / inventory / 候选项 / Facts / identity），裸 UUID 替换为可读名，detected_facts / identity JSON 等调试信息默认收纳到「高级」区。

### P1
- R3 `WorkspaceList` 跟随 backend 健康状态刷新（与 BackendAccessPanel 一致或提升到父页面统一加载）。
- R4 收敛三个创建入口为更清晰的主线；detect 成功后支持一步创建。

### P2
- R5 detect+register UI 抽成可复用组件，消除重复。
- R6 保存后先刷新再关闭；消除 props 派生 state 反模式（减少对 key remount 的依赖）。
- R7 `WorkspaceListEditor.tsx` 拆分（badges / detector / create / detail）。

## Acceptance Criteria

### 阶段 P0
- [ ] AC1 成功/信息类反馈（如「已加入运行位置，保存后生效」「已登记为可选目录」）以非错误语气（非 destructive 红字）呈现；仅校验失败/接口报错用红字。
- [ ] AC2 新增术语常量模块；列表与编辑器中不再出现「逻辑 Workspace / 落点 / binding / inventory / 候选项 / identity」等裸内部词，统一为约定中文表达。
- [ ] AC3 界面默认不再直出裸 UUID（workspace id、matched_workspace_ids）与 detected_facts/identity JSON；UUID 以可读名替代，调试信息收进「高级」折叠。
- [ ] AC4 `npm/pnpm` lint + type-check + 既有单测（含 workspaceRouting.test.ts）通过。

### 阶段 P1
- [ ] AC5 backend 上线/掉线后，工作空间列表的「可用数 / 运行解析徽章」与 Backend Access 面板保持一致刷新（不再需要手动刷新页面）。
- [ ] AC6 创建入口收敛为两条主线 + 本机识别次级动作；detect 成功后可一步创建工作空间，无需「登记→回候选区→选中→保存」多步。
- [ ] AC7 上述刷新与创建路径有对应单测或交互验证覆盖。

### 阶段 P2
- [ ] AC8 detect+register UI 抽为单一可复用组件，编辑器内不再有两份重复拷贝。
- [ ] AC9 保存（更新）成功后先刷新数据再决定关闭/驻留；移除依赖 key remount 重置的 props 派生 state 反模式。
- [ ] AC10 `WorkspaceListEditor.tsx` 拆分为多个聚焦文件（badges / detector / create / detail），单文件显著缩短且对外导出保持兼容。

## Out of scope（确认）

- 后端 API / 领域模型（agentdash-domain / agentdash-application / agentdash-api）改动。
- workspace 数据模型（identity_kind / binding / resolution_policy）语义变更。
- 引入完整 i18n 框架（仅做轻量术语常量模块）。

## Decisions（已确认）

- D1（范围）：**全做 P0–P2，在同一任务内分阶段提交**（P0 → P1 → P2，每阶段独立可验证、独立 commit）。
- D2（文案）：**中文为主 + 保留少量约定俗成专名（Workspace、Backend）**。新增一个轻量术语常量模块收口面向用户的文案；内部模型词（binding/落点→运行位置、inventory/候选项→可选目录、identity→代码来源、resolution→运行解析）统一翻译；Facts/detected_facts/identity JSON 等收进「高级」折叠。
- D3（入口收敛）：**两条主线**——「从可选目录创建」(默认) + 「先建空壳，之后补运行位置」(logical)；**本机目录识别降级**为可选目录区里的次级动作（「找不到？浏览本机目录添加」）；detect 成功后支持**一步创建**。

## 影响半径（已核实）

- 受影响的 Badge（WorkspaceStatusBadge/BindingStatusBadge/ResolutionBadge）与 WorkspaceEditorDrawer **仅在 `workspace-list/` 目录内被引用**，拆分自包含、无外部 import。
- `workspaceRouting.ts` 仅被 WorkspaceList、WorkspaceListEditor 及 `workspaceRouting.test.ts` 使用。改动需保持 `workspaceRouting.test.ts` 通过。
