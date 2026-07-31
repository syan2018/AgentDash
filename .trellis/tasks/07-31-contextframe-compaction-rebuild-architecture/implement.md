# 实施计划

## 准备门槛

- [x] 用户确认`prd.md`、`design.md`与本计划。
- [x] 执行`task.py start`后再修改产品代码。
- [x] 记录工作区已有修改并保持不触碰。
- [x] 读取implement/check manifests中的Trellis specs与research。

## Phase 1：锁定边界与失败测试

- [x] 更新backend/cross-layer/frontend specs，明确conversation record与system ContextFrame双lane。
- [x] 增加多Identity/Guidelines/Environment contribution测试，证明当前实现错误地产生逐来源Frame。
- [x] 增加`S1 → S2 → compact → S3`纵向测试，先证明当前实现会在compact后保留旧delta。
- [x] 增加provider、context recipe与canonical compaction batch exact equality测试。
- [x] 增加负向断言：retained message、ToolCall、ToolResult不生成ContextFrame。
- [x] 增加负向断言：Surface revoke/audit-only提示不伪装成ContextFrame。

Validation:

```powershell
cargo test -p agentdash-agent --test dash_service
cargo test -p agentdash-integration-native-agent --test complete_agent_service
```

## Phase 2：收缩ContextFrame协议

- [x] 删除无消费者的`ContextDeliveryPlan/Target/Entry`。
- [x] 删除旧RuntimeSession planner的cache/model-channel/connector-profile/consumption metadata。
- [x] 删除无生产语义的phase/apply-mode/delivery-channel/message-role与旧Frame kind/section。
- [x] 删除固定值top-level `ContextFrameSource`，provenance只保留在section与History occurrence。
- [x] 将`ContextDeliveryStatus`收缩为normal apply与compaction rebuild两个真实状态。
- [x] 引入唯一通用`ContextFragments` section，删除重复或从未生产的stable text section变体。
- [x] 让usage label、UI label和Frame order直接由kind/compiler派生。
- [x] 删除前端metadata fallback与phase/order重排。

Validation:

```powershell
cargo test -p agentdash-agent-protocol
pnpm --dir packages/app-web test -- src/features/session/model/contextFrame.test.ts
pnpm --dir packages/app-web typecheck
```

## Phase 3：建立Dash ContextFrame compiler

- [x] 将Frame编排从Native `accepted_context.rs`移动到`agentdash-agent::dash`深模块。
- [x] 实现initial、surface update、surface revoke、compaction rebuild四个crate-internal
  use-case入口。
- [x] 聚合Initial Context与Surface，同一stable kind只生成一个Frame。
- [x] 按accepted order把同类来源保存为一个`ContextFragments` section。
- [x] 从fragments/typed sections派生`rendered_text`，禁止独立正文输入。
- [x] 保持capability多维section聚合；normal使用`previous → current`，首次/rebuild使用
  `empty → current`。
- [x] Native integration只保留Bound Surface到Dash raw facts映射与canonical projection。
- [x] 迁移materializer tests，删除原crate renderer。

Validation:

```powershell
cargo test -p agentdash-agent
cargo test -p agentdash-integration-native-agent
```

## Phase 4：把Frames放回accepted occurrence

- [x] 从`InitialContextInstallation`与`DashSurface`删除嵌套`context_frames`。
- [x] 为`InitialContextInstalled`、`SurfaceApplied/SurfaceRevoked`增加本次accepted
  `context_frames`。
- [x] Dash apply API在history commit前调用compiler，不接受adapter预填Frames。
- [x] projector只发布History payload中已接受的Frames。
- [x] Surface stable comparison改为聚合后的Frame semantic content。
- [x] Surface revoke删除Ignore/AuditOnly假Frame，保存Initial-only stable Frames与
  current-to-empty capability Frame。

Validation:

```powershell
cargo test -p agentdash-agent --test dash_history
cargo test -p agentdash-agent --test dash_service
cargo test -p agentdash-integration-native-agent --test complete_agent_service
```

## Phase 5：让CompactionApplied保存完整reset Frames

- [x] 将`summary_frame`改为`context_frames: Vec<ContextFrame>`。
- [x] `complete_compaction()`从current Initial Context与Surface调用共享compiler。
- [x] Surface capability使用`previous=None`生成full current state。
- [x] 为全部reset Frames重建compaction occurrence ID、status与accepted time。
- [x] 加入本次summary Frame，统一排序后与`CompactionCompleted`原子提交。
- [x] History fold校验IDs、status、顺序、accepted summary与context revision。
- [x] `CompactionState`只保存Frame集合，不再复制summary字段。

Validation:

```powershell
cargo test -p agentdash-agent --test dash_history
cargo test -p agentdash-agent --test dash_service
```

## Phase 6：重写Active Frame Fold

- [x] 用Initial install、Surface apply/revoke与successful compaction四条event rule实现active fold。
- [x] latest successful compaction直接替换active Frames，并只fold其后的Surface facts。
- [x] Surface apply/revoke用payload替换整个stable slot集合，`CapabilityStateDelta`执行append。
- [x] 验证Surface删除某个stable kind时旧Frame不会残留。
- [x] 删除`accepted_surface_append_frames()`扫描全部history的旧消费路径。
- [x] provider round、context recipe、usage与compaction input统一调用该fold。
- [x] retained conversation继续由现有record fold独立物化。

Validation:

```powershell
cargo test -p agentdash-agent
```

## Phase 7：Canonical Projection 与前端呈现

- [x] `CompactionApplied` projector原样发布payload `context_frames`。
- [x] 删除compaction projector对recipe/history rematerializer的调用。
- [x] normal Surface projector继续previous/current真实change规则。
- [x] 前端基于`AppliedToCompactedContext`显示“上下文已重建”。
- [x] 同一stable Frame完整展示全部fragments/typed sections，保持backend Vec顺序。
- [x] 覆盖normal update、full rebuild、Frame顺序与完整展开测试。

Validation:

```powershell
cargo test -p agentdash-integration-native-agent
pnpm --dir packages/app-web test -- src/features/session
pnpm --dir packages/app-web typecheck
```

## Phase 8：Persistence Hard Migration

- [x] 新增下一号PostgreSQL migration，收敛不兼容Dash source documents。
- [x] 同事务清除指向旧Dash source的Product runtime bindings。
- [x] 释放未终态Mailbox上的旧source/generation delivery evidence。
- [x] 清理旧Dash source/effect，runtime只接受新History payload与新ContextFrame协议。
- [x] 加入embedded PostgreSQL migration + reprovision owner-invariant测试。

Validation:

```powershell
cargo test -p agentdash-infrastructure
```

## Phase 9：跨层验收与规范同步

- [x] 更新`agent-runtime-context`、`agent-runtime-native-adapter`、
  `agent-runtime-persistence`与`backbone-protocol`最终合同。
- [x] 运行manual/automatic overflow、restart、reopen、fork、revoke与second compaction测试。
- [x] 对同一history head比较accepted reset Frames、canonical events、Context inspector、
  provider capture、tools、messages、usage与digest。
- [x] 验证同一stable kind最多一个Frame，且contribution order在内部fragments中保持。
- [x] 验证生产代码不存在旧DeliveryPlan metadata消费者或前端fallback。
- [x] 运行格式化、clippy、targeted Rust/TS tests与`git diff --check`。

Validation:

```powershell
cargo fmt --all -- --check
cargo clippy -p agentdash-agent -p agentdash-integration-native-agent -p agentdash-infrastructure --all-targets -- -D warnings
cargo test -p agentdash-agent
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-infrastructure
pnpm --dir packages/app-web test -- src/features/session
pnpm --dir packages/app-web typecheck
git diff --check
```
