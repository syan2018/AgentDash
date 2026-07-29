# Canvas Interaction Runtime

## Runtime 模型

展示 Canvas definition 时，Canvas provider 创建或复用 `InteractionInstance`，并 attach 当前 actor。
instance 拥有 canonical shared state 与单调递增的 state revision。renderer lease、Workspace tab
与 attachment 具有不同生命周期。

仅在当前 Workspace Module surface 返回 `interaction:{instance_id}` 时使用它。把 descriptor 中的
Agent state projection 作为只读上下文。

## Command

修改状态前：

1. describe 当前 `interaction:{instance_id}` module。
2. 选择 `agent_and_panel` 且 `ready` 的 command Operation。
3. 复制完整 OperationRef，并遵守 input schema。
4. 携带 descriptor 要求的准确 expected state revision。
5. 成功或发生 revision conflict 后重新 describe。

command 拥有确定性 state transition。不要把 projected state、component props 或 presentation
payload 当作 write authority。

## Component 与 event

Canvas component binding 引用显式声明的 Extension component 与固定 component ABI。component
event 通过声明的 event binding 进入 trusted host。binding 可以指向：

- versioned platform command；
- 一个准确 Operation；
- 有界的 ephemeral OperationScript。

iframe 不接收 credential、backend ID、placement fact、authorization revision 或 host filesystem
path。host 解析 authority，并重新准入每次 Operation。

OperationScript 只用于有界即时组合。durable retry、recovery、human gate 或跨 session 编排使用
Workflow。

## Resource slot

通过声明的 resource slot 与当前 attachment authority 绑定数据。区分 definition-level、
shared-instance 与 attachment-local binding。binding 不是 Canvas source，也不会隐式修改 canonical
Interaction state。
