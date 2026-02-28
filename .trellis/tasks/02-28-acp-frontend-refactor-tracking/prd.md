# ACP 前端重构 PRD（多 Agent 协同）

## 1. 背景与目标
当前 ACP 会话前端已可用，但存在“协议语义与渲染行为不完全一致”的问题，导致以下风险：
- 流恢复存在丢帧/重帧隐患（SSE/NDJSON 重连场景）。
- `tool_call/tool_call_update` 生命周期在乱序和增量更新场景下不稳定。
- `session_info_update/usage_update` 被弱化或丢弃，系统事件不可见。
- 关键路径自动化测试不足，重构风险高。

本次重构目标：
1. 建立稳定、可验证的 ACP 前端状态模型。
2. 完成流传输与会话渲染的语义对齐。
3. 通过多 agent 并行推进，并在统一进度板中可追踪。

## 2. 范围
包含范围：
- ACP 会话流接入与重连恢复（`streamTransport.ts`, `useAcpStream.ts`）。
- ACP 事件归并与展示模型（`useAcpSession.ts`, `types.ts`）。
- 会话渲染组件（`AcpSessionEntry.tsx`, `AcpToolCallCard.tsx`, `AcpMessageCard.tsx`）。
- ACP 相关测试（模型层 + 组件层）。

不包含范围：
- Story/Task 看板功能重构。
- 后端业务重写（仅允许最小接口补齐与契约对齐）。

## 3. 问题定义（重构输入）
1. `tool_call_update` 在缺少锚点时降级策略不明确，容易出现“静默错误”。
2. pending approval 状态在后续 update 中可能被错误覆盖。
3. 仅依据标题/文案猜测工具语义，导致跨模型行为不稳定。
4. `session_info_update/usage_update` 未形成完整展示链路。
5. 自动滚动策略与增量渲染耦合，长响应体验不稳定。

## 4. 需求规格
### 4.1 功能需求
- FR-1: 建立统一 reducer（或等价状态机）处理 ACP 更新归并。
- FR-2: `tool_call` 生命周期必须覆盖：
  - `pending`
  - `in_progress`
  - `completed`
  - `failed`
  - `canceled`
  - `rejected`
- FR-3: 对 `session_info_update` 和 `usage_update` 提供可视化渲染。
- FR-4: 重连恢复必须支持“按事件游标继续”，并确保幂等去重。
- FR-5: 取消执行要有即时用户反馈，不可长期停留在执行中假象。

### 4.2 非功能需求
- NFR-1: 可维护性：协议解析、状态归并、UI 渲染三层解耦。
- NFR-2: 可测性：关键状态流转必须有自动化测试。
- NFR-3: 可观测性：异常路径（解析失败、孤立 update）必须可见。

## 5. 重构方案（分阶段）
### Phase 1: 传输与状态基座
目标：
- 固化流恢复语义，建立 ACP reducer 主入口。

交付：
- `useAcpStream.ts` 重构为可测试的纯归并逻辑 + Hook 包装。
- `streamTransport.ts` 修复重连游标推进与重复消息处理。

验收：
- 断线重连后无丢帧、无重复、顺序稳定。

### Phase 2: 会话模型与生命周期对齐
目标：
- 统一 tool call 生命周期与聚合行为。

交付：
- `useAcpSession.ts` 去除“标题猜语义”逻辑，改为协议字段驱动。
- `types.ts` 收敛显示模型，明确异常/孤立事件类型。

验收：
- 乱序 `tool_call_update` 不导致状态倒退或丢失。

### Phase 3: UI 渲染补齐
目标：
- 事件“可见即可信”，杜绝关键协议事件静默丢弃。

交付：
- `AcpSessionEntry.tsx` 补齐 `session_info_update`/`usage_update` 分支。
- `AcpToolCallCard.tsx` 补齐 canceled/rejected 等终态展示。
- 优化滚动与流式指示策略。

验收：
- 系统事件、用量事件、工具终态均可被用户辨识。

### Phase 4: 测试与回归收口
目标：
- 将核心语义变更纳入自动化防回归。

交付：
- 新增 reducer 单测：chunk merge、乱序 update、孤立 update、重连去重。
- 组件测试：关键事件渲染与状态标记。

验收：
- ACP 关键路径测试通过，回归风险可控。

## 6. 多 Agent 协同规则
- 所有 agent 在推进前先认领 `progress.md` 对应行。
- 每次代码提交后必须更新：
  - 状态
  - 最后更新时间
  - 产物（commit/PR/测试结果）
- 出现阻塞时，必须在 `progress.md` 增加阻塞记录（含 owner 与 next step）。

## 7. 验收矩阵
1. 协议正确性：
- [ ] `tool_call/tool_call_update` 各状态可稳定复现
- [ ] `session_info_update/usage_update` 渲染可见

2. 流可靠性：
- [ ] 重连后事件连续
- [ ] 去重逻辑有效

3. 用户体验：
- [ ] 取消执行后状态反馈及时
- [ ] 长流式响应自动滚动稳定

4. 质量保障：
- [ ] 新增测试通过
- [ ] 无新增高优先级回归问题

## 8. 风险与回滚
- 风险 R1: reducer 重构导致现有渲染行为变化。
  - 缓解: 增量提交，每阶段保留回归测试。
- 风险 R2: 传输层改动引入连接抖动。
  - 缓解: 保留 SSE 回退路径，逐步启用 NDJSON 优化。
- 回滚策略:
  - 以 Phase 为单位回滚，不跨阶段混合提交。
