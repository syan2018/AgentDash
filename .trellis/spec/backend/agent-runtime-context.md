# Complete Agent Context 与 Compaction

## 1. Scope

本规范约束 Complete Agent context、initial context package、compaction 与 Agent Runtime
projection 的 owner 边界。

## 2. Ownership

- 完整 Agent 独占自己的 history、context、fork、compaction 与 resume authority。
- Dash Agent 以 ordered history 维护 `AgentSession`；context materialization 和
  compaction 都是 history-derived lifecycle。
- Codex 使用原生 ThreadStore、`thread/read`、`thread/compact` 与 history replacement。
- Agent Runtime 只拥有 command admission、operation、normalized snapshot/change、
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

command inbox、provider effect、retry 与 recovery ledger 位于 `AgentSession` 外。一次
`DashAgentCommit` 原子提交 effect settlement、history append/head CAS、derived change
与下一 continuation intent。

`CompactionApplied`保存最终CompactionSummary ContextFrame。后续provider round、overflow
continuation与canonical presentation直接使用该frame的`rendered_text`，使summary恢复与用户看到的
compaction evidence保持同一revision。

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

## 9. Scenario: Typed Compaction Checkpoint 与 Exact Recipe

### 9.1 Scope / Trigger

修改Dash compaction applied fact、summary frame、provider materializer或context query时适用。
checkpoint必须能从source history重放，因为成功压缩会改变下一轮模型输入。

### 9.2 Signatures

```rust
HistoryPayload::CompactionApplied {
    compaction_id: CompactionId,
    checkpoint: CompactionCheckpoint,
}

pub struct CompactionCheckpoint {
    pub operation_id: EffectId,
    pub context_revision: ContextRevision,
    pub base_history_revision: u64,
    pub applied_history_revision: u64,
    pub source_head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub summary: String,
    pub summary_frame: ContextFrame,
    pub compacted_entry_ids: Vec<HistoryEntryId>,
    pub retained_from: Option<HistoryEntryId>,
    pub retained_entry_ids: Vec<HistoryEntryId>,
    pub tool_pairs: Vec<CompactionToolPairMembership>,
    pub checkpoint_digest: String,
    pub usage: Option<CompactionUsageEvidence>,
    pub created_at_ms: u64,
}
```

```rust
pub struct AgentContextSnapshot {
    pub source: AgentSourceCoordinate,
    pub snapshot_revision: AgentSnapshotRevision,
    pub context_revision: Option<String>,
    pub recipe_digest: AgentPayloadDigest,
    pub authority: AgentContextAuthority,
    pub fidelity: AgentContextFidelity,
    pub contributions: Vec<AgentContextContribution>,
}
```

### 9.3 Contracts

- 只有`CompactionApplied + CompactionCompleted`共同出现的checkpoint进入current recipe。
- `summary_frame.sections`使用`ContextFrameSection::CompactionSummary`，并保存identity、trigger、
  source range、first-kept coordinate、统计、usage evidence与真实created time。
- normal provider round、compaction input、post-compaction continuation和context query调用同一个
  history materializer；frame排序、retained boundary和tool pairing只有一份实现。
- tool call message使用call entry identity，tool result message使用result entry identity；
  checkpoint另存typed pair membership。
- failed/lost/cancelled只写terminal evidence，current context revision与recipe保持上一个成功值。

### 9.4 Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| checkpoint operation/source digest与active compaction不一致 | history fold拒绝transition |
| provider side effect尚未started | Applied被拒绝 |
| 同一compaction重复Applied | history fold拒绝transition |
| Completed没有checkpoint | history fold拒绝transition |
| retained boundary命中tool pair | call/result都保留并按原顺序物化 |
| query要求旧snapshot revision | 返回conflict |

### 9.5 Good / Base / Bad Cases

- Good：checkpoint保存typed frame、成员坐标和tool pairs，reload后recipe digest稳定。
- Base：`retained_from=None`表示无历史suffix，summary frame仍是完整recipe contribution。
- Bad：只保存summary字符串和timeline event位置；reload无法证明真实retained membership。

### 9.6 Tests Required

- history fold覆盖Applied transition、重复Applied、无Applied Completed与terminal不推进recipe。
- materializer fixture比较compactor输入、下一轮provider输入和context query。
- canonical projection验证同一summary frame进入timeline与current inspector。
- reload验证checkpoint identity、真实时间、usage和tool pair membership。

### 9.7 Wrong vs Correct

```rust
// Wrong：summary字符串不足以证明当前模型输入。
CompactionApplied { summary, retained_from }

// Correct：一次Applied提交完整、可重放的typed checkpoint。
CompactionApplied { compaction_id, checkpoint }
```
