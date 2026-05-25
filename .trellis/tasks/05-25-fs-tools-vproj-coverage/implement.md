# Implement — fs 工具 virtual projection 覆盖与 relay 协议补齐

> 设计依据：[design.md](./design.md)。Branch: `release/fs-tools-rebuild`（amend PR #31）。

## S1 — SPI grep_text 默认实现升级

- [ ] [mount.rs](../../../crates/agentdash-spi/src/platform/mount.rs)
      `grep_text` 默认实现替换为 design.md §1.1 的 list+read+regex 通用算法。
- [ ] 加一个 helper 判断 `RuntimeFileEntry` 是否 binary（attributes.content_kind = "binary"）。
- [ ] 错误容错：read_text 返回 NotFound / NotSupported 的条目跳过 + warn 一次。
- [ ] **Validation:** `cargo check -p agentdash-spi`。

## S2 — Relay 协议字段补齐

- [ ] [relay/protocol/tool.rs](../../../crates/agentdash-relay/src/protocol/tool.rs)
      `ToolFileReadPayload` 加 `offset: Option<u64>` + `limit: Option<u64>`。
- [ ] `ToolSearchPayload` 加 `case_sensitive` + `multiline` + `before_lines` + `after_lines`
      四字段；`#[serde(default)]` 兼容兜底。
- [ ] `default_case_sensitive() -> bool { true }` helper。
- [ ] **Validation:** `cargo check -p agentdash-relay`。

## S3 — relay_fs provider 真协议实现

- [ ] [api/mount_providers/relay_fs.rs](../../../crates/agentdash-api/src/mount_providers/relay_fs.rs)
      实现 `read_text_range`（构造带 offset/limit 的 ToolFileReadPayload）。
- [ ] 实现 `grep_text`（构造带全部新字段的 ToolSearchPayload）。
- [ ] 现有 `search_text` 保持通用搜索语义（不传 grep 字段）。
- [ ] **Validation:** `cargo check -p agentdash-api`。

## S4 — lifecycle.search_text 调整

- [ ] [provider_lifecycle.rs](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs)
      `search_text` 非 skills 路径改返回 `MountError::NotSupported`，不再 `Ok(empty)`。

## S5 — 集成测试

### S5.1 lifecycle journey grep / range read

- [ ] 在 [provider_lifecycle.rs](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs)
      tests 模块中加 mock LifecycleRunRepository / Journey 接口（用 in-memory
      实现，模拟一个 step state + 一个含 5KB stdout 的 tool-call）。
- [ ] T1: `read_text_range` 在 `tool-calls/{id}/stdout` 上 offset=100, limit=50 ⇒
      返回 100-149 行。
- [ ] T2: `grep_text` 在 `tool-calls/{id}` 子树 ⇒ 找到含 "error" 的行（pattern
      正则 + 行号正确）。

### S5.2 canvas / skill_asset grep_text 测试

- [ ] [provider_canvas.rs](../../../crates/agentdash-application/src/vfs/provider_canvas.rs)
      加 grep_text 测试：构造一个 canvas 含 src/main.tsx + README.md，
      grep regex `(?i)render` + include_glob `*.tsx` ⇒ 仅命中 main.tsx。
- [ ] [provider_skill_asset.rs](../../../crates/agentdash-application/src/vfs/provider_skill_asset.rs)
      加 grep_text 测试：context_lines=1 ⇒ 命中行 + 上下 1 行。

### S5.3 relay 协议 serde round-trip

- [ ] [relay/protocol/tool.rs](../../../crates/agentdash-relay/src/protocol/tool.rs)
      加 #[cfg(test)] mod tests：
      - ToolFileReadPayload 含 offset=Some(10), limit=Some(50) 序列化 + 反序列化 round-trip。
      - ToolSearchPayload 含全部新字段 round-trip。
      - 旧 JSON（缺 case_sensitive/multiline/before_lines/after_lines）反序列化
        到默认值。

## S6 — 全量回归

- [ ] `cargo build --workspace --lib`。
- [ ] `cargo test --workspace --lib` 全绿。
- [ ] 已有 fs_grep / fs_read / fs_glob / provider_inline 测试不退化（行为兼容）。

## S7 — Commit + amend PR + archive

- [ ] Commits（拆分清楚便于 review）：
  - `refactor(vfs-spi): grep_text 默认实现升级为通用 list+read+regex`
  - `feat(relay): ToolFileReadPayload + ToolSearchPayload 补齐 offset/limit/grep 字段`
  - `feat(relay-fs): read_text_range + grep_text 走真协议`
  - `fix(lifecycle): search_text 非 skills 不再 short-circuit empty`
  - `test(vfs): journey/canvas/skill_asset grep + relay 协议 serde 覆盖`
- [ ] amend PR #31 描述：追加 follow-up 章节、修订 lifecycle/relay_fs/降级说明。
- [ ] `task.py finish && task.py archive`。

## 风险与回滚

| 风险 | 应对 |
|------|------|
| SPI grep_text 默认实现的 list recursive 在大 mount 上太慢 | head_limit 短路；include_glob 过滤；二进制跳过 |
| lifecycle list 在某些路径上未枚举到所需 entry（导致 grep 漏 hit） | 测试 T2 验证 tool-calls 子树覆盖；不全的路径作为 follow-up 增量 |
| relay_fs 远端 backend 没适配新字段 | 协议字段全 Option/serde default，远端能忽略；agent 端不会因此 break |
| lifecycle search_text 改 NotSupported 后某调用方崩 | grep 工具 PR #31 后已经走 grep_text 路径；其他调用方 grep 不到 | 

## 完成判定

- [ ] S1-S7 全 ✓
- [ ] cargo test --workspace --lib 全绿
- [ ] PR #31 描述更新到位

## 执行策略

全 inline 处理。lifecycle journey mock 是本任务唯一较重的 fixture 工作；
其余都是单点改动。
