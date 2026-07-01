# Implementation Plan

## Ordered Work

### W1. Protocol And Delivery Metadata

- 在 `agentdash-spi::hooks` 中新增 Memory section 类型。
- 新增 `memory_inventory_delta` frame kind 的 delivery metadata mapping。
- 为 `memory_context` 迁移到 Memory typed snapshot section，保留 rendered text 作为 connector-facing 文本。
- 增加 SPI 序列化测试，覆盖 snapshot/delta section 的 snake_case shape。

### W2. Runtime Memory Inventory Diff

- 新增 runtime-session helper：
  - `diff_memory_inventory(before, after)`
  - `build_memory_inventory_delta_frame(input)`
  - stable revision/digest 计算。
- 单测覆盖 source created / updated / removed / reindexed / diagnostics_changed。
- 保证空 diff 不生成 frame。

### W3. Active Runtime Integration

- 找到运行中 Memory inventory refresh 的最小接入点：
  - 首选 runtime VFS mutation 后按 memory discovery rules 触发 re-discovery。
  - 若当前没有统一 mutation hook，则先在 runtime surface update path 中接入可调用的 refresh helper。
- active projection 保存 latest Memory inventory 或 revision summary。
- Memory delta frame 进入 context_frame event，并在 live connector 需要时走 turn-start/context notification。

### W4. Frontend Typed Parser And Renderer

- 在 `packages/app-web/src/features/session/model/contextFrame.ts` 中新增 Memory section union。
- `ContextFrameStream` 按 delivery phase 分组或至少在 discovered inventory 下合并 Memory 与 tool/MCP/VFS/skill。
- `SectionRenderers.tsx` 新增 Memory snapshot/delta renderer。
- 更新 ContextFrameCard / SessionEntry context-frame 测试，覆盖 snapshot 与 delta 展示。

### W5. PiAgent Regression

- 保留并加强 PiAgent system prompt 测试，确保 `memory_context` 与 `memory_inventory_delta` 都不会进入 system prompt。
- 验证 delivery metadata 仍为 discovered inventory / context / discovery digest。

### W6. Spec Updates

- 更新 `.trellis/spec/backend/session/execution-context-frames.md`：
  - Memory snapshot vs delta 职责。
  - Memory section contract。
  - Memory 不进入 system prompt。
- 更新 `.trellis/spec/backend/capability/integration-api.md`：
  - Memory provider 负责识别 source/index，平台负责 VFS 权限、diff、ContextFrame event。

## Suggested Sub-agent Split

| Lane | Focus | Notes |
| --- | --- | --- |
| A | SPI protocol + metadata mapping | unblock B/D/E |
| B | runtime-session diff + frame builder | after A |
| C | active runtime integration | after B, may require code search for VFS mutation path |
| D | frontend parser/render/grouping | after A |
| E | PiAgent/spec regression | after A/B |

## Validation Plan

Rust:

```powershell
cargo test -p agentdash-spi memory
cargo test -p agentdash-application-runtime-session memory_inventory
cargo test -p agentdash-application-runtime-session context_frame
cargo test -p agentdash-executor assemble_system_prompt
cargo check -p agentdash-application-runtime-session -p agentdash-executor
```

Frontend:

```powershell
pnpm --filter app-web run typecheck
pnpm --filter app-web test -- ContextFrameCard
pnpm --filter app-web test -- SessionEntry.context-frame
```

## Risky Files

- `crates/agentdash-spi/src/hooks/mod.rs`
- `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`
- `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`

## Start Gate

- Confirm whether first slice only needs source/index-level delta，还是必须解析 topic-level diff。
- Confirm runtime integration path after code inspection: VFS mutation trigger vs explicit memory refresh path.
- `implement.jsonl` 与 `check.jsonl` 必须保留真实 spec/context entries，不能只保留 seed `_example`。
