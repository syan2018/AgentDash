# 实施计划

## Phase 1：Native command admission

- 为 Dash Submit 建立可测试的 admission/execution 分界。
- Native Complete Agent 普通 Submit 返回 `Accepted`，owner task 推进执行并在 terminal 更新 effect。
- 保持 Steer、Interrupt、interaction、compaction 的明确语义，不用兼容分支混用旧阻塞行为。
- 更新 Complete Agent 与 Product facade 集成测试，覆盖 active read、重复 effect、interrupt 与 terminal inspect。

## Phase 2：Native durable usage

- 将实际 model context window 注入 Native provider。
- 扩展 provider/Core/Dash round completion usage 数据。
- 增加 history payload/folded usage state 与 canonical `TokenUsageUpdated` 投影。
- 覆盖多 provider round 累计、snapshot/changes/live 一致性和重连恢复。

## Phase 3：前端状态与 Task effect

- 用生产 shape 测试 `TurnStarted -> workspace refresh -> enabled cancel/steer`。
- 把 successful completed `task_write` 加入 control-plane effect planner，并接入 Task store refresh。
- 覆盖非 terminal、失败及非 task 工具不刷新。
- 验证 pending Submit 不再占据整个回合，Steer/Stop 使用当前 snapshot command。

## Phase 4：删除旧审计线

- 删除 `inspector://session` 前端入口、Context Audit service/UI、旧 Hook audit presentation。
- 删除 backend Context Audit bus、AppState wiring、frame producer 参数及对应测试。
- 调整 workspace tab/layout 与 context construction 测试，只保留当前权威 ContextFrame 路径。

## Phase 5：质量门

- 定向 Rust tests：Agent Core、Dash history/service、Native Complete Agent、AgentRun product facade。
- 定向 Web tests：managed runtime feed、session stream/commands、control-plane planner、workspace tabs、Task 状态栏。
- 运行相关 Rust 编译检查、Web typecheck/lint；按项目说明处理 Cargo lock 与定向 rustfmt。
- 用 `pnpm dev` 做一次 Native 会话人工验证：运行态、Steer、Stop、usage、task_write、右栏无旧审计 Tab。

## Phase 6：历史投影与动作状态收口

- 从 Managed Runtime canonical history 重建 `runtime/context/projection` 路由与无状态 projector。
- 覆盖用户/助手/工具明细、ContextFrame 分类、附件与 compaction 边界。
- 删除前端对投影 404 的静默降级，让产品合同断链直接暴露。
- Stop 可见性只读取权威 workspace execution status，不再读取 `isReceiving`。

## Phase 7：Native steering owner 收口

- 删除 `DashProvider::steer`，Provider 只保留单次模型 request/stream 职责。
- Dash Service 原子持久化 active turn 的 Steer 输入与 receipt，并维护 source-owned 消费游标。
- Agent Core 在工具结果后和 provider `Stop` 边界排空输入；有输入时继续同一 turn 的下一轮。
- 自动压缩 continuation 接管 steering owner 并保留游标，避免压缩窗口丢失已接纳输入。
- 用不实现专用 steering 的 production-shape provider 覆盖 Submit、Steer、后续 provider round
  与 Interrupt。

## 验证结果

- Native Complete Agent：29/29；Dash service：10/10；Agent Core loop：3/3；Dash Core：1/1。
- Agent library：26/26；Application：315/315；AgentRun application/facade library：135/135。
- Web全量测试：102 files、532/532；相关定向测试：4 files、24/24；typecheck通过。
- 本任务修改的TS/TSX定向ESLint通过；全量lint仍被33个未修改文件中的既有React Hooks错误阻断。
- 相关六个Rust crate的cargo check通过；旧审计标识负向扫描无匹配；`git diff --check`通过。
- 历史上下文 projector 2/2、Product route ledger、Runtime service 与 Stop 状态定向测试通过。
- 未启动`pnpm dev`做人工页面验收；未增加跨真实Product binding/resolver的专项ProductFacade组合夹具。
