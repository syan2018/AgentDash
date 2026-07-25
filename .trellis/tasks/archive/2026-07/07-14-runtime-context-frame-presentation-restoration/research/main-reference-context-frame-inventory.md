# main-reference ContextFrame 搬运清单

## 固定参考源

```text
repository: D:/Projects/AgentDash-main-reference
branch:     main
commit:     957fa9d60ea3d67efa1bb278fe5b376cf0c34598
policy:     read-only behavior oracle
```

所有实现与 review 必须用 `git -C D:/Projects/AgentDash-main-reference` 确认该 commit；若参考目录 HEAD 改变，先记录并重新审核 golden 来源，不能静默跟随。

## 核心类型与公共规则

| 规则 | 参考文件 |
| --- | --- |
| ContextFrame/delivery/section 类型 | `crates/agentdash-spi/src/hooks/mod.rs` |
| 公共 builder、timestamp/ID、notice enqueue | `crates/agentdash-application-runtime-session/src/session/context_frame.rs` |
| turn 内排序、去重、delivery plan/record | `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs` |
| durable event wrapper/persistence | `crates/agentdash-application-runtime-session/src/session/eventing.rs` |

## Frame family builders

| Family | 参考文件 | 新归属 |
| --- | --- | --- |
| identity | `session/identity_context_frame.rs` | Runtime context_projection + Application identity facts |
| user_context | `session/user_context_frame.rs` | Runtime context_projection + Application user facts |
| environment | `session/environment_context_frame.rs` | Runtime context_projection + workspace facts |
| system_guidelines | `session/guidelines_context_frame.rs` | Runtime context_projection + guideline facts |
| memory_context | `session/memory_context_frame.rs` | Runtime context_projection + memory facts |
| assignment_context | `session/assignment_context_frame.rs` | Runtime context_projection + workflow/hook injection facts |
| capability/tool/skill/VFS/MCP/companion delta | `session/hub/runtime_context_transition.rs` 与 `session/dimension/*` | Runtime normalized surface delta |
| pending_action | `session/pending_action_context_frame.rs` | Runtime HookRun/mailbox projection |
| auto_resume/system notice | hook notice builders + launch preparation | Runtime HookRun/mailbox projection |
| compaction_summary | `session/compaction_context_frame.rs` 与 `session/eventing.rs` | Runtime context activation projection |

## 触发路径

| 场景 | 参考路径 | 新 canonical operation |
| --- | --- | --- |
| 初始启动 | `session/launch/preparation.rs` → `launch/commit.rs` | ThreadStart |
| live surface transition | `session/hub/runtime_context_transition.rs` | SurfaceAdopt |
| queued hook/pending frame | `session/launch/preparation.rs` | HookRun effect / next TurnStart |
| compaction | `session/eventing.rs` | Context activation/head commit |

## 搬运规则

1. 先为参考 builder 建立 fixture/golden，再写新 projector。
2. payload 字段、section 内容与顺序保持一致；wrapper 可改为 typed PlatformEvent。
3. 不搬运 `SessionRuntimeInner`、live connector、进程内 queue 或 repository side effects。
4. builder 内 `Utc::now()` 改为显式 operation clock；magic string 改为 typed enum，序列化值保持一致。
5. 多点 delta 计算收敛为一次 normalized `ContextSurfaceDelta`。
6. 每个 golden 记录参考文件、输入 fixture 与允许 normalization 的动态字段。

## WI-01 固定行为证据

以下证据均取自固定 commit `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`，路径相对于
`D:/Projects/AgentDash-main-reference`。动态 normalization 只允许替换 `id`、
`created_at_ms` 以及 canonical wrapper coordinate；`kind`、delivery metadata、sections、
rendered text 与序列顺序不可 normalization。

| Family | Builder / rule evidence | Trigger / order evidence |
| --- | --- | --- |
| identity | `session/identity_context_frame.rs`; kind=`identity` | `session/launch/preparation.rs`; stable-system order 10 |
| user_context | `session/user_context_frame.rs`; kind=`user_context` | `session/launch/preparation.rs`; stable-system order 12 |
| environment | `session/environment_context_frame.rs`; kind=`environment` | `session/launch/preparation.rs`; session-policy order 15 |
| system_guidelines | `session/guidelines_context_frame.rs`; kind=`system_guidelines` | `session/launch/preparation.rs`; session-policy order 20 |
| compaction_summary | `session/compaction_context_frame.rs` | `session/eventing.rs`; run-state order 30 |
| assignment_context | `session/assignment_context_frame.rs` | `session/launch/preparation.rs`; assignment order 40 |
| capability/tool/skill/VFS/MCP/companion delta | `session/hub/runtime_context_transition.rs` | initial selection in `session/launch/preparation.rs`, live emission in `session/hub/runtime_context_transition.rs`; discovered-inventory order 50 |
| memory_context | `session/memory_context_frame.rs` | `session/launch/preparation.rs`; discovered-inventory order 60 |
| pending_action | `session/pending_action_context_frame.rs` | pending action collection in `session/launch/preparation.rs`; turn-runtime order 70 |
| auto_resume/system notice | `session/launch/preparation.rs` notice conversion | queued notice collection in the same file; turn-runtime order 80 |

### 公共 payload 与 wrapper oracle

- `session/context_frame.rs::build_context_frame` 是所有 family 的公共 payload 构造点；原实现
  在 builder 内取时间并以该时间生成 ID。新 projector 必须显式接收 operation time/identity，
  相同 operation 重放产生相同 ID 和 digest。
- `agentdash-spi/src/hooks/mod.rs` 定义旧 JSON vocabulary；其中
  `ContextDeliveryMetadata::for_frame` 固定 phase/order/cache/channel/frontend label。
- `session/eventing.rs::emit_context_frame` 将完整 frame 放入
  `SessionMetaUpdate { key: "context_frame", value }`。新 wrapper 为 typed
  `PlatformEvent::ContextFrameChanged { frame }`，内部 `frame` JSON 必须逐字段相等。
- `session/launch/preparation.rs` 先按 delivery phase/order 规划，再按 frame ID 去重；stream
  golden 必须保留该顺序与相邻 frame 聚合边界。

### 已落地的 executable oracle

- `agentdash-agent-protocol::tests::context_frame_changed_preserves_the_main_reference_payload_shape`
  固定验证 owned payload 与 typed wrapper，覆盖旧 JSON 的 null/省略和 metadata 形状。
- `agentdash-agent-protocol/tests/fixtures/context_frames_canonical_roundtrip.json` 与
  `owned_context_frame_vocabulary_round_trips_without_claiming_producer_semantics` 只验证 owned
  payload vocabulary 的 serde 形状；它不提供 main producer、触发顺序或 delivery 语义证据。
- `wrapper_neutral_normalization_only_removes_coordinates_identity_and_time` 证明 normalization
  只允许 wrapper coordinate、frame ID 和时间变化，rendered text 等 payload 变化必须失败。
- `agentdash-agent-runtime::context_projection::projector::tests::projection_is_replayable_and_keeps_main_payload_shape`
  固定验证显式 operation identity/time、稳定 ID、digest 和 identity family payload。
- 各 family 的 producer oracle 必须由 main-reference production builder 规则与当前
  actual-producer command/UoW 测试逐字段建立，不允许从协议 fixture 或前端 consumer 反推。
