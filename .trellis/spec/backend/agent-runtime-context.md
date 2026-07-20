# Complete Agent Context 与 Compaction

## 1. Scope

本规范约束 Complete Agent context、initial context package、compaction 与 Managed Runtime
projection 的 owner 边界。

## 2. Ownership

- 完整 Agent 独占自己的 history、context、fork、compaction 与 resume authority。
- Dash Agent 以 ordered history 维护 `AgentSession`；context materialization 和
  compaction 都是 history-derived lifecycle。
- Codex 使用原生 ThreadStore、`thread/read`、`thread/compact` 与 history replacement。
- Managed Runtime 只拥有 command admission、operation、normalized snapshot/change、
  source evidence 与 availability，不保存可反向恢复外部 Agent 的 context head。
- Product 只编译 initial context contribution 和 Agent Surface requirement，不读取或改写
  Agent 内部 repository。

## 3. Initial context

Fresh create 可以原子携带 `InitialAgentContextPackage`。Package 必须包含 stable package
identity、schema version、mode、typed contributions、逐项 authority/revision/digest
provenance 与整体 digest。

- contribution 至少区分 compact summary、workflow context 与 constraint set；
- Workspace/VFS、Tool、Hook、credential 与 capability grant 继续通过
  `AgentSurfaceSnapshot -> BoundAgentSurface -> AppliedAgentSurface` 交付；
- receipt/inspect 必须返回 applied package digest 和真实
  `TypedNative | CanonicalRendered | Unsupported` fidelity；
- Runtime 在 applied evidence 到达前不激活 source；
- 派发任务作为 create 之后的首个普通 `SubmitInput`，不能代替 initial context 安装。

## 4. Compaction capability

Complete Agent 逐项声明：

```text
AgentOwnedNative | ExactContextRevision | ObservedOnly | Unsupported
```

required compaction 只有匹配的 exact/native 能力可以通过 admission。ObservedOnly 只允许
投影 Agent 自发 activity；Unsupported 在任何 side effect 前 typed reject。

## 5. Dash Agent semantics

Dash Agent compaction 以 history transform 表达：

```text
source history revision
  -> CompactionStarted
  -> summary + retained suffix + provenance
  -> CompactionApplied(new history revision)
  -> CompactionCompleted
```

command inbox、provider effect、retry 与 recovery ledger 位于 `AgentSession` 外。一次
`DashAgentCommit` 原子提交 effect settlement、history append/head CAS、derived change
与下一 continuation intent。

Manual compaction：

- normal Turn active 时 durable queued，但不创建伪 Turn/Item；
- compaction active 时新输入 deferred，不 steer 进 maintenance activity；
- terminal 后由独立 promotion 选择下一 command。

Automatic overflow 使用独立 A/B/C identity：

- A 为失败的 Agent Turn；
- B 为独立 compaction activity；
- C 为独立 continuation intent/Turn；
- B terminal 不隐式创建 C；
- clean failure exactly-once terminalize C，Lost 阻塞 promotion。

## 6. Codex 与其它 Agent

Codex adapter 发送 native compact command并映射可证明的
`ContextCompaction started/completed/failed/lost` 与 snapshot；Runtime 不安装 Dash
ContextRevision。其它 Agent 只按 descriptor 中声明的真实 capability 接入。

## 7. Presentation

Runtime committed projection/change 保存完整 typed compaction body、identity、source
revision、fidelity 与 terminal evidence。Canonical conversation projector保持同一 item
identity和顺序；前端从 item lifecycle渲染 running/succeeded/failed/lost，不固定解释为
completed。

## 8. Tests

- Fresh create package digest/fidelity、unknown outcome 与 first-input ordering。
- Dash history replay、manual queue、A/B/C、clean failure/Lost 与 atomic commit。
- Codex native compaction source mapping与 gap snapshot reconcile。
- Unsupported/Observed 不满足 required exact。
- Runtime reconnect只读 snapshot revision + durable change tail，不 replay presentation
  journal 或 Agent 内部 history。
