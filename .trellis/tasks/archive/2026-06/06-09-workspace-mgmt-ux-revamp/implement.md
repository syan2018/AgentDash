# 执行计划 — Workspace 管理页交互与文案优化

三阶段（P0→P1→P2），每阶段结束跑校验并独立 commit。

## 验证命令（pnpm@10，cwd = packages/app-web）
- 类型：`pnpm -C packages/app-web typecheck`
- Lint：`pnpm -C packages/app-web lint`
- 单测：`pnpm -C packages/app-web test`
- 一键：`pnpm -C packages/app-web check`
- 手动联调（按需）：`pnpm -C packages/app-web dev`

## 阶段 P0 — 反馈语气 + 文案/术语 + 去调试信息
1. [ ] 先读 `features/workspace/model/workspaceRouting.test.ts`，确认对中文 label 的断言，决定保值还是同步更新。
2. [ ] 新建 `features/workspace/model/workspaceTerms.ts`：迁入 `IDENTITY_KIND_LABELS`，定义用户词表（运行位置/可选目录/代码来源/运行解析）与保留专名说明。
3. [ ] `workspaceRouting.ts`：`identityKindLabels` 迁移/re-export，保证既有 import 与测试不破。
4. [ ] `WorkspaceListEditor.tsx`：引入 `feedback {tone,text}` 取代裸 `message`；归类所有 `setMessage` 调用的 tone；渲染按 tone 着色（替换 L1036-1038 的统一 destructive）。
5. [ ] 替换面向用户文案：列表头（WorkspaceList:83-108）、空状态、徽章 label、各 DetailSection 标题/描述、placeholder（去字段名、给示例）。
6. [ ] 去调试信息：列表 `Facts:`（WorkspaceList:148-152）下沉/移除；详情副标题裸 UUID（Editor:649）处理；`matched_workspace_ids`（Editor:766）映射为可读名；`identity JSON` summary 改「高级（开发者）」。
7. [ ] 校验：`check` 通过。Commit：`fix(workspace): 反馈语气分级 + 面向用户文案统一，收纳调试信息`。

## 阶段 P1 — 刷新一致性 + 创建入口收敛
8. [ ] `WorkspaceList.tsx`：新增 `backendRuntimeSignature` useMemo + useEffect（首帧跳过），变化时重载 `loadRoutingInputs()`，与 BackendAccessPanel 对齐（AC5）。
9. [ ] create 路径：`onCandidatesChanged()` 改 `await` 后再 `onClose()`（Editor:580-581）。
10. [ ] 创建入口收敛（D3）：`CreateMode` 收敛为 `from_directory | logical`；顶部改两条主线；候选区底部加「找不到？浏览本机目录添加」次级动作（`canManageBindings` gated）。
11. [ ] detector detect 成功后新增主按钮「用这个目录创建 Workspace」，内部一步完成（构造 binding/必要时登记 → createWorkspace），省去多步舞步（AC6）。
12. [ ] 补/调单测覆盖刷新与新建路径（AC7）；手动走查创建两条主线 + 本机识别。
13. [ ] 校验通过。Commit：`feat(workspace): 列表跟随 backend 健康刷新 + 创建入口收敛为两条主线`。

## 阶段 P2 — 组件拆分 + 反模式清理
14. [ ] 抽 `badges.tsx`（3 个 Badge + config），更新 WorkspaceList / 编辑器 import。
15. [ ] 抽 `DirectoryDetector.tsx`，prop `mode: 'fill-binding' | 'register-inventory'`，消除两份重复 detect+register UI（AC8）。
16. [ ] 拆 create / detail 为各自组件；`WorkspaceEditorDrawer` 作薄壳分发或由 WorkspaceList 直接选用；保持对外导出兼容。
17. [ ] detail 表单改 `useEffect([workspace.id])` 显式同步，弱化对 key remount 的依赖；保存成功先 `await` 刷新再决定关闭/驻留并刷新运行解析预览（AC9）。
18. [ ] 校验通过；人工回归 create/detail/detect/register/delete 全流程。Commit：`refactor(workspace): 拆分 WorkspaceListEditor 并清理 props 派生 state`。

## 风险点 / 回滚
- 风险文件：`WorkspaceListEditor.tsx`（最大改面）、`workspaceRouting.ts`（被测试依赖）。
- 每阶段独立 commit，可单独 revert。
- 拆分阶段务必保持 barrel/兼容导出，避免 `workspace-list.tsx` 等外部入口破裂。

## task.py start 前 checklist
- [ ] prd.md 三阶段 AC 已就绪且可测。
- [ ] design.md 边界、刷新方案、拆分形态明确。
- [ ] 用户已 review 规划并同意进入实现。
