# Complete Agent Context 与 Compaction

## 1. Scope

本规范约束 Complete Agent context、initial context package、compaction 与 Agent Runtime
projection 的 owner 边界。

## 2. Ownership

- 完整 Agent 独占自己的 history、context、fork、compaction 与 resume authority。
- Dash Agent 以 ordered history 维护 `AgentSession`；context materialization 和
  compaction 都是 history-derived lifecycle。
- Codex 使用原生 ThreadStore、`thread/read`、`thread/compact` 与 history replacement。
- Agent Runtime 只拥有 command admission、normalized snapshot/change、source evidence 与
  availability；operation receipt直接引用Complete Agent public effect，不形成独立operation
  aggregate，也不保存可反向恢复外部Agent的context head。
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
- concrete Agent在安装时把每项contribution物化为accepted ContextFrame并随history保存；
  provider input与canonical history读取同一`rendered_text`，原因是initial context的authority、
  provenance与实际模型文本必须由同一接纳事实证明。

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
  -> CompactionApplied(new history revision + accepted CompactionSummary frame)
  -> CompactionCompleted
```

command inbox、public effect、retry与recovery事实位于同一个source repository。一次
`DashAgentCommit`原子提交command settlement、history append/head CAS与下一continuation
intent；change feed按history revision即时派生。

`CompactionApplied`保存最终CompactionSummary ContextFrame。后续provider round、overflow
continuation与canonical presentation直接使用该frame的`rendered_text`，使summary恢复与用户看到的
compaction evidence保持同一revision。canonical presentation在该摘要前重新发布当前已接纳的
AgentFrame ContextFrame，使时间线按“权威上下文刷新、压缩摘要”呈现这次事实切换。

Manual compaction：

- normal Turn active 时 durable queued，但不创建伪 Turn/Item；
- compaction active 时新输入 deferred，不 steer 进 maintenance activity；
- terminal 后由独立 promotion 选择下一 command。

Automatic overflow 使用A/B/C command chain：

- A 为原始SubmitInput command；
- B 为依赖A的compaction command；
- C 为依赖B的continuation command/Turn；
- B terminal 不隐式创建 C；
- clean failure exactly-once terminalize C，Lost 阻塞 promotion。

## 6. Codex 与其它 Agent

Codex adapter 发送 native compact command并映射可证明的
`ContextCompaction started/completed/failed/lost` 与 snapshot；Runtime 不安装 Dash
ContextRevision。其它 Agent 只按 descriptor 中声明的真实 capability 接入。

## 7. Presentation

Runtime committed projection/change保存typed compaction状态、source revision、fidelity与
terminal evidence，但不重复发布public effect/operation identity。Canonical conversation
projector保持同一item identity和顺序；前端从item lifecycle渲染
running/succeeded/failed/lost，不固定解释为completed。

## 8. Tests

- Fresh create package digest/fidelity、unknown outcome 与 first-input ordering。
- Dash history replay、manual queue、A/B/C、clean failure/Lost 与 atomic commit。
- Codex native compaction source mapping与 gap snapshot reconcile。
- Unsupported/Observed 不满足 required exact。
- Runtime reconnect只读 snapshot revision + durable change tail，不 replay presentation
  journal 或 Agent 内部 history。

## 9. Scenario: Minimal Compaction Applied Fact 与 Exact Recipe

### 9.1 Scope / Trigger

修改Dash compaction applied fact、summary frame、provider materializer或context query时适用。
Applied fact必须能从source history重放，因为成功压缩会改变下一轮模型输入。

### 9.2 Signatures

```rust
HistoryPayload::CompactionApplied {
    compaction_id: CompactionId,
    context_revision: ContextRevision,
    summary_frame: ContextFrame,
    retained_from: Option<HistoryEntryId>,
}
```

```rust
pub struct AgentContextSnapshot {
    pub source: AgentSourceCoordinate,
    pub recipe: AgentContextRecipe,
}

pub struct AgentContextRecipe {
    pub coordinate: AgentContextCoordinate,
    pub usage: AgentContextUsageAnalysis,
    pub contributions: Vec<AgentContextContribution>,
}
```

### 9.3 Contracts

- 只有`CompactionApplied + CompactionCompleted`共同出现的summary frame进入current recipe。
- `summary_frame.sections`使用`ContextFrameSection::CompactionSummary`，并保存identity、trigger、
  source range、first-kept coordinate、统计与真实created time。
- normal provider round、compaction input、post-compaction continuation和context query调用同一个
  history materializer；frame排序、retained boundary和tool pairing只有一份实现。
- current recipe按provider输入顺序包含typed ContextFrame、已接纳Tool definition与retained
  Message；CompactionSummary固定排在其它权威ContextFrame之后。
- `usage`从同一次物化得到的frames、tools与messages派生，不写入history；因此Inspector分类与
  实际recipe成员使用同一估算口径。
- tool call message使用call entry identity，tool result message使用result entry identity；
  retained membership由history boundary和materializer确定，不在Applied重复保存entry列表。
- failed/lost/cancelled只写terminal evidence，current context revision与recipe保持上一个成功值。
- `context_revision`由Started保存的source digest、canonical summary和retained boundary确定性生成；
  history fold校验该值。
- Runtime context projection直接返回完整`AgentContextRecipe`。浏览器因此同时看到coordinate、
  usage、typed ContextFrame、Tool definition、retained Message与Opaque evidence，不把
  compaction summary当成recipe本身。

### 9.4 Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| context revision与Started source/summary/retained boundary不一致 | history fold拒绝transition |
| 同一compaction重复Applied | history fold拒绝transition |
| Completed没有Applied | history fold拒绝transition |
| retained boundary命中tool pair | call/result都保留并按原顺序物化 |
| query要求旧snapshot revision | 返回conflict |

### 9.5 Good / Base / Bad Cases

- Good：Applied只保存canonical summary frame、context revision和retained boundary，reload后recipe
  digest稳定。
- Base：`retained_from=None`表示无历史suffix，summary frame仍是完整recipe contribution。
- Bad：同时保存summary字符串、frame、source digest、entry列表、tool pair与usage副本；这些字段
  会让同一history事实出现多个可漂移表示。

### 9.6 Tests Required

- history fold覆盖Applied transition、重复Applied、无Applied Completed与terminal不推进recipe。
- materializer fixture比较compactor输入、下一轮provider输入和context query。
- canonical projection验证同一summary frame进入timeline与current inspector。
- reload验证summary frame、context revision、retained materialization与recipe digest。

### 9.7 Wrong vs Correct

```rust
// Wrong：把可从history/summary frame派生的信息再做一份checkpoint mirror。
CompactionApplied { compaction_id, checkpoint }

// Correct：一次Applied提交最小、可重放的canonical事实。
CompactionApplied { compaction_id, context_revision, summary_frame, retained_from }
```
