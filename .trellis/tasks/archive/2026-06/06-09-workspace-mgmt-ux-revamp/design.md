# 技术设计 — Workspace 管理页交互与文案优化

仅前端（`packages/app-web`）改动；不触碰后端契约与数据模型。改动按 P0/P1/P2 三阶段，每阶段保持可独立编译与提交。

## 1. 术语与文案（P0 · D2）

### 1.1 新增术语常量模块
- 新文件：`packages/app-web/src/features/workspace/model/workspaceTerms.ts`
- 收口面向用户的中文文案与映射，作为单一事实源。示例（最终以实现为准）：
  - `IDENTITY_KIND_LABELS`：`git_repo→"Git 仓库" / p4_workspace→"P4 工作空间" / local_dir→"本地目录"`（沿用现有 `identityKindLabels`，迁移到此并去掉散落定义）。
  - 用户词表：`运行位置`(binding/落点)、`可选目录`(inventory/candidate)、`代码来源`(identity)、`运行解析`(resolution)。
  - 保留专名：`Workspace`、`Backend`（与 Backend Access 面板、团队口语一致）。
- `workspaceRouting.ts` 现有 `identityKindLabels` 改为 re-export 或迁移，保证 `workspaceRouting.test.ts` 仍可 import（若测试断言具体中文串，保持值不变或同步更新测试——实现时先看测试）。

### 1.2 调试信息收纳
- `Facts: {factSummary}`（WorkspaceList:148-152）：从列表行默认显示移除，或改为仅在详情「高级」区展示。
- `detected_facts` / `identity JSON` textarea（Editor:822-844）：维持在 `<details>` 折叠，summary 文案改为「高级（开发者）」。
- 裸 UUID：
  - 详情副标题 `ID: {workspace.id}`（Editor:649）→ 改为不显示或显示可读摘要。
  - `matched_workspace_ids.join(", ")`（Editor:766）→ 用 workspaces 列表映射成 `name`，找不到再退化为短 id。

### 1.3 反馈语气分级（P0 · AC1）
- 现状：单一 `message` state + `text-destructive`（Editor:1036-1038）混用成功/信息/错误。
- 方案：引入 `feedback: { tone: 'success' | 'info' | 'error'; text: string } | null` 取代裸 `message`，渲染按 tone 着色（success→success 色，info→muted，error→destructive）。store 的 `error` 仍视为 error tone。
- 影响点：所有 `setMessage(...)` 调用归类 tone（创建/登记/加入落点为 success/info；校验失败为 error）。

## 2. 状态刷新一致性（P1 · D 无新决策，技术对齐）

### 2.1 backend 健康驱动刷新（AC5）
- 现状：`WorkspaceList` 仅 mount 时 `loadRoutingInputs()`；`BackendAccessPanel` 用 `backendRuntimeSignature`（ProjectSettingsPage:448-468）监听重载。
- 方案（择一，倾向 A）：
  - **A（推荐）**：在 `WorkspaceList` 内复用同样的 `backendRuntimeSignature` 派生 + effect，变化时重新 `loadRoutingInputs()`。改动局部、低风险。
  - B：把 `accesses/candidates/backends` 的加载提升到 `ProjectSettingsPage`，与 BackendAccessPanel 共用一份并下传。更彻底但牵动更多 props。
- 选 A：保持 `WorkspaceList` 自治，新增一个 `useMemo` signature + `useEffect`（首帧跳过，参照现有 `hasObservedBackendRuntimeRef` 模式）。

### 2.2 创建/保存后刷新时序
- create 路径 `onCandidatesChanged()` 改为 `await` 后再 `onClose()`（Editor:580-581）。
- 详见 P2 的保存驻留（AC9），P1 阶段先保证「刷新发生在关闭前」。

## 3. 创建入口收敛（P1 · D3）

### 3.1 目标信息架构
- 顶部不再是三个并列 mode 按钮。改为两条主线：
  1. **从可选目录创建**（默认）：展示候选 `CandidateList`，选中即生成 Workspace 身份 + 初始运行位置（沿用 `handleSelectCandidate`/`candidateToDraft`）。
     - 候选区底部次级动作：「找不到？浏览本机目录添加」→ 打开目录浏览/识别（仅 `canManageBindings` 可见，复用 detector 组件）。
     - detector detect 成功后提供主按钮「用这个目录创建 Workspace」：内部先登记/构造 binding 再走创建，免去「登记→回候选区→选中→保存」。
  2. **先建空壳，之后补运行位置**（logical）：仅填身份，bindings 为空，提示之后由可用目录匹配。
- `CreateMode` 类型从三值收敛为二值（`from_directory` | `logical`）；`local_detect` 不再是顶层 mode，其能力并入「from_directory」的次级 detector。

### 3.2 兼容与权限
- `visibleCreateModes` 逻辑简化：两条主线对所有人可见；本机识别次级动作受 `canManageBindings` 控制（与现状一致）。
- detail 模式不受入口收敛影响（detail 无 create mode）。

## 4. 组件拆分与反模式清理（P2）

### 4.1 文件拆分（AC10）
将 `WorkspaceListEditor.tsx`（1092 行）拆为：
- `badges.tsx`：`WorkspaceStatusBadge` / `BindingStatusBadge` / `ResolutionBadge` + 其 status/label config。
- `DirectoryDetector.tsx`：选 backend + 路径输入 + 浏览 + detect + 识别结果展示；prop `mode: 'fill-binding' | 'register-inventory'` 控制 detect 成功后的行为，消除两份重复（AC8）。
- `CreateWorkspaceForm`（或 `WorkspaceCreateDrawer`）：create 模式专用。
- `WorkspaceDetailDrawer`：detail 模式专用（含删除确认、路由预览、落点管理）。
- 保留 `WorkspaceEditorDrawer` 作为薄壳按 `mode` 分发，或由 `WorkspaceList` 直接选用对应 drawer。对外导出保持兼容（WorkspaceList 的 import 不破）。
- `IdentityFields`、`CandidateList` 可移入各自子文件或公共子目录。

### 4.2 props 派生 state 反模式（AC9）
- 现状：表单字段 `useState(workspace?.xxx)` + 依赖 `key` remount 重置（WorkspaceList:203/218）。
- 方案：拆分后 create/detail 各自组件，初始值职责更清晰；detail 用一个 `useEffect([workspace.id])` 显式同步表单，而非依赖外层 key。保存成功后：先 `await` 刷新 store，再根据场景决定关闭或驻留并刷新「运行解析」预览。
- key remount 可保留为兜底，但不再是唯一重置手段。

## 5. 风险与回滚
- 每阶段独立 commit；P0 改文案/语气风险最低，P1 涉及刷新时序与交互结构，P2 是结构重构。
- 主要风险点：
  - `workspaceRouting.test.ts` 对中文 label 的断言 → 改前先读测试，同步更新或保值。
  - 入口收敛改变用户既有操作路径 → 需人工过一遍 create/detail/detect/register/delete 全流程。
  - 拆分文件后 import 路径 → 保持 barrel/兼容导出，避免外部破坏。
- 回滚：按阶段 commit，任一阶段出问题可单独 revert 该阶段 commit。

## 6. 验证
- `pnpm -C packages/app-web lint`、`typecheck`、`test`（具体命令见 implement.md，按仓库实际脚本）。
- 手动：在工作空间 Tab 走查创建（两条主线 + 本机识别次级）、详情编辑、backend 上下线后列表刷新、各类反馈语气。
