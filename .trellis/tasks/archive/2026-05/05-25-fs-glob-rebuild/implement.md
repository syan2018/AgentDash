# Implement — fs_glob 重做

> 设计依据：[design.md](./design.md)。Branch: `release/fs-tools-rebuild`。

## S1 — Schema + prompt 改写

- [ ] **S1.1** [glob.rs](../../../crates/agentdash-application/src/vfs/tools/fs/glob.rs)
      改 `FsGlobParams`（pattern 必填、去 recursive、加 max_results）+
      `deny_unknown_fields`。
- [ ] **S1.2** description 改写为 design.md §4 措辞。

## S2 — execute 主体

- [ ] **S2.1** detect_recursion(pattern) 推断 recursive（pattern 含 `**`）。
- [ ] **S2.2** 调 service.list；filter is_vcs_path（暴露 service 层 helper 或在 tool 层重复）。
- [ ] **S2.3** sort_by_key：mtime desc + path asc 兜底。
- [ ] **S2.4** max_results = 0 ⇒ usize::MAX；其他 ⇒ truncate + 提示。
- [ ] **S2.5** 输出格式：目录 trailing slash，文件无前缀。

## S3 — 暴露 is_vcs_path

- [ ] **S3.1** [relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)
      `is_vcs_path` helper 改为 `pub(crate) fn`，供 fs_glob 调用。

## S4 — 测试矩阵 9 项

按 design.md §5 落 9 项测试。fixture 复用 fs_grep 的 inline mock 设置。

## S5 — 全量回归 + commit + archive

- [ ] cargo test --workspace --lib 全绿。
- [ ] commit + archive。

## 风险

| 风险 | 应对 |
|------|------|
| service.list 接口的 pattern 字段已支持 substring fallback，fs_glob 改后行为变 | 仅 fs_glob tool 层不再用 substring；service.list 接口语义不动，其它调用方（如 fs_glob 之外）不受影响 |
| inline list 不返回 modified_at | RuntimeFileEntry.modified_at 在 SPI 已支持（spi-fix）；inline 本任务前未填，本任务在 inline list 里加上 updated_at_ms |

## 完成判定

- [ ] S1-S5 全 ✓，9 项测试全绿；workspace 通过。
