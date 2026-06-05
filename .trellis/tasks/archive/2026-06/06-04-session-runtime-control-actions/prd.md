# 修复 Session 全链路控制动作模型

## Goal

让 Agent Session 页面用准确的控制动作影响后端，并按真实运行态展示输入栏状态。

用户从 Agent 页进入 Draft 会话后，首条消息会 materialize RuntimeSession + LifecycleRun。进入真实 `/session/{id}` 后，如果当前 turn 正在执行，页面不能把“下一轮不可发送”误显示为“未连接 Agent dispatcher”；同时运行中输入应该支持 steer 当前 turn，取消应该作为独立控制动作保留。

## Confirmed Facts

- `SessionPage` 当前用 `runtimeControl?.can_send` 派生 `sessionSendReady`，并用它决定是否向 `SessionChatView` 传 `customSend`。
- 后端 `/sessions/{id}/runtime-control` 的 `can_send` 表示“当前是否能发送下一轮消息”，运行中会因为 `delivery_running` 返回 false，不代表 RuntimeSessionExecutionAnchor 或 Agent dispatcher 缺失。
- `LifecycleAgentMessageService.dispatch_user_message` 表达的是新一轮用户消息投递，会进入 session launch / prompt claim 流程，不适合作为运行中 steer。
- 底层 Agent runtime 已有 steering queue；`AgentConnector` 已有运行中注入通知的相近能力，但用户 steer 需要显式控制语义，不能复用 notification 文案或 String-only 载荷。
- relay 当前唯一实际通道是 codex；codex 侧只要接入对应控制协议即可运行，因此本任务必须把 relay/codex steering 一并贯通，不作为首版延期项。
- 现有 `SessionChatView` 输入区只有 `customSend?: fn` 和 `sendUnavailableReason?: string`，无法表达“控制面存在、下一轮不可发、可 steer、可 cancel”的组合状态。

## Requirements

1. Runtime control view 必须把“控制面连接状态”和“可执行动作”分开表达。
   - 控制面状态至少区分：unbound trace、anchored idle、anchored running、terminal、frame missing / unavailable。
   - 动作能力至少区分：send next turn、steer running turn、cancel running turn。
   - 每个动作都要有 enabled 与 unavailable_reason，前端不再从单个布尔值推断状态。

2. 后端必须提供显式 steer 用户消息入口。
   - steer 只面向已有 RuntimeSessionExecutionAnchor 的 lifecycle agent session。
   - steer 只能在当前 session running 且 connector 支持 steering 时可用。
   - steer 不创建新的 RuntimeSession、LifecycleRun、LifecycleAgent、AgentFrame，也不 claim 新 prompt/turn。
   - steer 请求使用与普通消息一致的 prompt block 语义，不降级成仅 String 文案。
   - relay/codex executor 必须贯通 steer，不允许仅返回 unsupported。

3. 普通下一轮消息与运行中 steer 必须走不同控制路径。
   - idle 时发送走现有 lifecycle agent message / launch 路径。
   - running 时输入栏主动作走 steer 路径。
   - cancel 是独立动作，运行中仍可用，不被 steer/send 状态挤掉。

4. Draft 首条启动链路必须保持当前目标。
   - 进入 Agent 页新会话时不预创建完整 session/lifecycle。
   - 首条 prompt 提交后才 materialize runtime/lifecycle 并导航到真实 session。
   - 首条启动失败时仍不留下空 lifecycle/session 数据。

5. 前端必须以 action model 渲染输入栏。
   - `SessionPage` 根据 Draft 参数或 runtime-control 生成明确的 action 状态。
   - `SessionChatView` / composer 不再用 `customSend` 是否存在判断 dispatcher 是否连接。
   - 文案必须准确：运行中显示“可 steer / 正在执行”，不可显示“未连接 dispatcher”；只读 trace 才显示无控制面。

6. 合同、生成类型、服务层、单元测试必须同步更新。
   - 不做兼容字段双读；预研阶段直接收敛到正确 contract。
   - 本任务预期不新增数据库 schema。若实现发现需要持久化 steering queue，必须新增 migration 文件，不能修改既有 migration。

## Acceptance Criteria

- [ ] 进入 Draft Agent Session 时，输入栏显示可开始首条消息；首条发送后导航到真实 session。
- [ ] 首条消息返回前或真实 session 正在 running 时，输入栏不再显示“当前 Session 未连接到 Agent dispatcher”。
- [ ] anchored idle session 显示“发送”动作，并调用下一轮 message endpoint。
- [ ] anchored running session 显示“steer”主动作和独立“取消”动作；Ctrl+Enter 触发 steer，不触发新 turn。
- [ ] terminal 或 frame missing session 禁用对应输入动作，并展示后端 action reason。
- [ ] unbound trace session 可以查看 trace，但输入栏明确显示只读原因。
- [ ] runtime-control 合同中不再把 `can_send` 当作唯一控制能力；前端使用 action set。
- [ ] steer endpoint 有后端测试覆盖：缺 anchor、非 running、connector 不支持、成功入队。
- [ ] relay/codex 通道有 steer 贯通测试或等价验证，证明运行中 steer 被转发到 codex 控制协议，而不是触发新 prompt。
- [ ] 前端测试覆盖：draft start、idle send、running steer、running cancel、readonly trace 状态。
- [ ] 类型检查、相关 Rust 测试、前端相关测试通过。

## Notes

- 本任务是上一个 Draft Session 启动任务的全链路修正，不另做兼容适配。
