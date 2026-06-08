# Implement Plan · Agent Workspace Module Registry 与 Canvas Extension 协作面预研

> 本 parent 保持 planning 状态；实际实现由 3 个 child task 承接。本文件是 parent 的拆分与集成视图，不代表已允许在 parent 上改运行代码。

## 1. 规划产物

- [x] 创建 Trellis parent task。
- [x] 写入 PRD：目标、确认事实、需求、验收标准、已决策（D1–D6）。
- [x] 写入 Design：核心概念、数据流、Extension/Canvas 映射、Workspace Module Tools 形态、AgentFrame 锚点修正、3-child 拆分。
- [x] 用户 review 并确认按 3 child 拆分实现，UI 落在 Child 3。
- [x] 创建并挂接 3 个 child task。

## 2. 已锁定决策（驱动拆分）

| 编号 | 决策 | 落点 |
|---|---|---|
| D1 | 首个 child = 只读路径，零执行副作用 | Child 1 |
| D2 | Canvas 首轮只映射已有 canvas，invoke 包现有 service，不另起 authoring | Child 2 |
| D3 | protocol channel 作为 provider module operations，不独立成 module | Child 1 契约 |
| D4 | AgentFrame 预留裁切字段，可见性解析走 Capability 通道 | Child 1 契约 |
| D5 | 项目设置页合并管理 = 同一 projection 另一消费端，契约定在 Child 1、UI 在 Child 3 | Child 1 + Child 3 |
| D6 | 收口 `extension_runtime` 死字段，不留半截投影 | Child 1 |

## 3. Child 任务树

- `06-08-workspace-module-read-contract` —— 读路径与单一契约
- `06-08-workspace-module-operate` —— invoke + present 操作面
- `06-08-workspace-module-integration-ui` —— 集成 review + 项目层管理 UI + 收尾

依赖顺序：Child 1 → Child 2 → Child 3。Child 1 的 projection DTO 与可见性契约是 Child 2/3 的前置；契约未定死前不开 Child 2/3 的运行代码。各 child 的候选修改面与验证命令见各自 `prd.md`（必要时活动前补 `design.md` / `implement.md`）。

## 4. 关键风险（贯穿三个 child）

- Workspace Module Registry 不能变成新的业务数据库；它始终是 AgentFrame/Session/Capability runtime projection。
- 元工具不能退化成无 schema 的万能 JSON 调用；`workspace_module_describe` 与服务端 schema 校验必须成对出现。
- Canvas 与 Extension 统一协作协议，但保留各自事实源与生命周期；invoke 的 canvas 分支必须复用现有 canvas service，**禁止第二条 authoring 路径**。
- 可见性必须走 Capability 通道，AgentFrame 不得形成与 capability 并行的第二套裁切规则。
- 死字段 `extension_runtime` 必须在 Child 1 被接管或删除，不允许新旧投影并存。
- Workspace Module 不能与既有 Workspace 域实体混淆（workspace root / WorkspaceBinding）；DTO 与文档需明确它是工作台协作模块。

## 5. 开始实现前检查（每个 child 激活前复用）

- [ ] 读取 `.trellis/spec/backend/runtime-gateway.md`。
- [ ] 读取 `.trellis/spec/cross-layer/shared-library-contract.md`。
- [ ] 读取 `.trellis/spec/guides/cross-layer-thinking-guide.md`。
- [ ] 复查当前 dirty worktree，避免把无关改动混入。
- [ ] 对应 child 经 `task.py start` 进入 `in_progress` 后再改运行代码。
- [ ] 复杂 child（至少 Child 1、Child 2）在 `start` 前补 `design.md` + `implement.md`。
