# Implementation Plan

## Ordered Work

### W1. Protocol And Delivery Metadata

- 在 `agentdash-spi::hooks` 中新增 Memory typed section 和 runtime memory source/diagnostic entry。
- `memory_context` 迁移到 Memory typed snapshot section，保留 rendered text 作为 connector-facing 文本。
- 不新增 `memory_inventory_delta` frame kind；Memory delta 作为 section 进入现有动态上下文 frame。
- 增加 SPI 序列化测试，覆盖 snapshot/delta section 的 snake_case shape。

### W2. Runtime Memory Dimension Delta

- 新增 `crates/agentdash-application-runtime-session/src/session/dimension/memory.rs`，参考 `dimension/skill.rs`。
- 实现 `MemoryDimensionDelta`：
  - added sources
  - removed sources
  - changed sources
  - source/index revision
- diff 粒度只到 source/index，不解析 topics。
- 单测覆盖 added / removed / changed / empty diff。

### W3. Runtime Surface Integration

- 在现有 runtime context transition / discovery refresh 路径中接入 Memory delta。
- 优先跟随 mount / runtime discovery surface 变化触发；普通 VFS 文件写入不作为首版触发源。
- 保存上一份 `MemoryDiscoveryOutput` 的轻量 projection，用于和下一份 discovery output 对比。
- Memory delta section 进入现有 context_frame event，并沿用 discovered inventory / discovery digest / context 的 delivery metadata。

### W4. Frontend Typed Parser And Renderer

- 在 `packages/app-web/src/features/session/model/contextFrame.ts` 中新增 Memory section union。
- `ContextFrameStream` 在 discovered inventory 层把 Memory 与 tool/MCP/VFS/skill 合并展示；不再把 Memory 当特殊 launch/system 块。
- `SectionRenderers.tsx` 新增 Memory snapshot/delta renderer。
- 更新 ContextFrameCard / SessionEntry context-frame 测试，覆盖 Memory snapshot 与 index/source delta 展示。

### W5. PiAgent Regression

- 保留并加强 PiAgent system prompt 测试，确保 `memory_context` 与包含 Memory delta section 的动态上下文 frame 都不会进入 system prompt。
- 验证 delivery metadata 仍为 discovered inventory / context / discovery digest。

### W6. Spec Updates

- 更新 `.trellis/spec/backend/session/execution-context-frames.md`：
  - Memory snapshot vs Memory delta section 职责。
  - Memory section contract。
  - Memory 不进入 system prompt。
- 更新 `.trellis/spec/backend/capability/integration-api.md`：
  - Memory provider 负责识别 source/index，平台负责 VFS 权限、diff、ContextFrame event。

## Suggested Sub-agent Split

| Lane | Focus | Notes |
| --- | --- | --- |
| A | SPI protocol + typed Memory section | unblock B/D/E |
| B | runtime-session MemoryDimensionDelta | after A |
| C | runtime surface/mount-change integration | after B |
| D | frontend parser/render/grouping | after A |
| E | PiAgent/spec regression | after A/B |

## Validation Plan

Rust:

```powershell
cargo test -p agentdash-spi memory
cargo test -p agentdash-application-runtime-session memory
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
- `crates/agentdash-application-runtime-session/src/session/dimension/mod.rs`
- `crates/agentdash-application-runtime-session/src/session/dimension/skill.rs`
- `crates/agentdash-application-runtime-session/src/session/hub/runtime_context_transition.rs`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`

## Start Gate

- 首版范围固定为 source/index 级 delta，不做 topic-level diff。
- 首版触发源固定为 mount / runtime discovery surface 变化，不追踪每次普通 VFS 文件写入。
- 实现时优先复用 Skill/VFS 维度 delta 路径，不新增 Memory 专属 live update 通道。
- `implement.jsonl` 与 `check.jsonl` 必须保留真实 spec/context entries，不能只保留 seed `_example`。
