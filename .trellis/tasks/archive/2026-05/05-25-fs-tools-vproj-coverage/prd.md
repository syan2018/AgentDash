# fs 工具 virtual projection 覆盖与 relay 协议补齐

> **Follow-up to:** `05-25-fs-tools-optimization-review`（已 archive）
> **Branch:** `release/fs-tools-rebuild`（同 release，amend PR #31）

## Goal

补全 PR #31 在 lifecycle virtual projection 与 relay_fs 上的核心场景缺口：
让 agent 能用 `fs_grep` / `fs_read` 从 journey state（含 tool-call 输出、
events.json、records 等）精确定位被截断的信息。

## Background — 当前 PR #31 的关键缺陷

agent 的核心使用场景：bash 输出 / 工具调用结果因长度被截断 → agent
后续从 journey 中用 `fs_read offset/limit` 切窗口、用 `fs_grep` 查关键词
重新接上下文。这是 fs_read/grep 工具最重要的 use case。

**PR #31 的实际行为：**

- `fs_grep` 在 lifecycle mount 上对 virtual projection（`nodes/{key}/session/...`、
  `tool-calls/{id}/...`、`records/...`、`runs/...`）路径返回**空集合**：
  [provider_lifecycle.rs:743-760](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs#L743)
  只覆盖 `skills` 子树，其余路径直接 `Ok(SearchResult { matches: vec![] })`。
- SPI `grep_text` 默认实现 forward 给 `search_text`，于是 lifecycle / canvas /
  skill_asset 的 grep 都继承上面这个 bug 或退化成 substring。
- `fs_read` 在 lifecycle virtual projection 上能工作（默认 `read_text + slice`），
  但 relay_fs 上 `read_text_range` 只能全文 fetch + 本地 slice —
  ToolFileReadPayload 没 `offset/limit` 字段。

## Requirements

### R1 — SPI `grep_text` 默认实现升级（通用 list + read + regex）

```rust
async fn grep_text(&self, mount, &GrepQuery, ctx) -> Result<SearchResult, _> {
    // 1. self.list(mount, ListOptions { recursive: true, ... }, ctx) 拿到所有
    //    可读文件（含 virtual projection 条目）。
    // 2. 对每个文件 self.read_text(...) 拿全文。
    // 3. 在内存里跑 regex + include_glob filter + before/after_lines context。
    // 4. case_sensitive / multiline 走 RegexBuilder 设置。
}
```

让所有 provider（含 lifecycle 的 virtual projection、canvas、skill_asset）
**自动获得完整 grep 能力**，不再需要逐 provider override。

inline 因为有 in-memory 表能优化（不走 list+read 二次 round-trip）保留 override。
其它 provider 默认即可。

### R2 — Relay 协议补齐

[`ToolFileReadPayload`](../../../crates/agentdash-relay/src/protocol/tool.rs#L6)：

```rust
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    // NEW
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,    // 0-based 行号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}
```

[`ToolSearchPayload`](../../../crates/agentdash-relay/src/protocol/tool.rs#L72)
补齐 grep 字段：

```rust
pub struct ToolSearchPayload {
    // ... existing fields ...
    // NEW
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub before_lines: usize,
    #[serde(default)]
    pub after_lines: usize,
}
```

`#[serde(default)]` 兜底，旧远端 backend 收到不识别字段会忽略，
新字段缺失也不破坏。

### R3 — `relay_fs` provider 真按协议传字段

- `read_text_range` 实现：构造带 offset/limit 的 ToolFileReadPayload，不再走
  SPI 默认 read_text + slice。
- `grep_text` 实现：构造带全部 grep 字段的 ToolSearchPayload，不再走 SPI
  默认 list+read+regex。

### R4 — `lifecycle` `search_text` 不再 short-circuit empty

非 skills 路径不再返回 `Ok(empty)`。具体方案：让 search_text 仅在 skills
路径生效；其他路径返回 `MountError::NotSupported`。这样 grep_text 默认实现
（R1 中升级为 list+read+regex）会自动覆盖 virtual projection 全部条目。

通用 substring 搜索（search_text）在 lifecycle 上没意义（journey 里很少有人
想做 substring grep），允许 `NotSupported`。

### R5 — 集成测试：journey state grep / range read

构造一个 mock lifecycle mount，模拟典型场景：

- node 的 tool-calls 中含有 5KB 的 bash 输出
- agent 用 `fs_read .../tool-calls/{id}/stdout offset=100 limit=50` 切 50 行窗口
- agent 用 `fs_grep .../tool-calls/{id}/ pattern="error.*"` 在 tool-call
  输出里找关键词

两条路径都应返回正确结果。

### R6 — 测试覆盖小补

- canvas / skill_asset 的 grep_text（走 SPI 默认）regex / include_glob /
  context_lines 各一项端到端测试。
- relay_fs 协议字段透传：用 relay mock 验证 ToolFileReadPayload offset/limit +
  ToolSearchPayload 多字段都能序列化 + 反序列化。

## Acceptance Criteria

- [ ] `cargo build --workspace --lib` + `cargo test --workspace --lib` 全绿。
- [ ] R1 SPI grep_text 默认实现完成；inline 保留 override，其它 provider 走默认。
- [ ] R2 协议字段补齐（带 `#[serde(default)]` 兼容兜底）。
- [ ] R3 relay_fs.read_text_range / grep_text 走真协议。
- [ ] R4 lifecycle.search_text 非 skills 返回 NotSupported；grep_text 自动通过 SPI 默认覆盖。
- [ ] R5 集成测试覆盖 journey state grep + range read 场景。
- [ ] R6 canvas / skill_asset 的 grep_text 路径有测试。
- [ ] amend 当前 PR #31 描述：追加 follow-up 章节、更新 breaking change /
      lifecycle / relay_fs 行为描述、移除原"warn-and-degrade 退化"那段。

## Constraints / 不在范围

- **不**实现远端 backend 端对新协议字段的处理（远端先按"忽略未识别字段"
  兜底；agent 端通过 relay_fs provider 把字段传出去后，远端启用支持是后续工作）。
- **不**改 SPI 的 SearchQuery / GrepQuery 类型（grep-query-split 已稳定）。
- **不**做 lifecycle records / runs 等路径的 list 覆盖增强（当前覆盖度已足够）。

## Notes

- 在 `release/fs-tools-rebuild` 分支上累积 commits，不开新分支。
- amend 当前 PR #31。
- 工时估算：< 1 day。
