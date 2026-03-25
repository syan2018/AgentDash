# Address Space 搜索能力增强：本地 ripgrep + 正则支持

## 背景

当前 `RelayAddressSpaceService::search_text` 的实现是"暴力遍历"：
1. 递归 `list` 目标路径下所有文件
2. 逐个 `read_text` 读取文件内容
3. 对每行做 `line.contains(query)` 纯字符串匹配

这个方案对 **inline_fs**（内联文件，通常文件数极少）可以接受，但对 **relay_fs**（本地工作空间）来说效率极低：
- 每次搜索需要通过 WebSocket relay 逐文件传输内容到云端
- 无法利用本机文件系统的高效搜索工具
- 不支持正则表达式
- 大型项目搜索延迟不可接受

## 目标

为 Address Space 搜索能力提供分层实现，本地工作空间使用 ripgrep 执行搜索。

## 需求

### P0：本地工作空间 ripgrep 搜索

- [ ] 新增 relay 命令 `CommandToolSearch`（或复用 shell exec），在本机后端调用 `rg`
- [ ] 搜索参数：`query`（支持正则）、`path`（搜索根路径）、`max_results`、`include_glob`（文件过滤）
- [ ] 返回格式：`file:line:content` 结构化结果
- [ ] `search_text` 对 relay_fs provider 走新的 relay 搜索命令，不再逐文件遍历
- [ ] 本机后端需检测 `rg` 是否可用，不可用时降级为当前逐文件方案

### P1：inline_fs 正则支持

- [ ] inline_fs 搜索从 `line.contains(query)` 升级为 `Regex::is_match`
- [ ] 保持向后兼容：纯字符串查询自动 escape 为正则字面量

### P2：fs_search 工具增强

- [ ] `FsSearchTool` 参数新增 `regex: bool`（默认 false，向后兼容）
- [ ] 新增 `include` glob 过滤参数
- [ ] 返回结果包含匹配行号和上下文行（可选 `-C` 参数）

## 技术方案

### Relay 协议扩展

```
CommandToolSearch {
    id: String,
    payload: ToolSearchPayload {
        call_id: String,
        workspace_root: String,
        query: String,
        path: Option<String>,
        is_regex: bool,
        include_glob: Option<String>,
        max_results: usize,
        context_lines: usize,
    },
}

ResponseToolSearch {
    id: String,
    payload: ToolSearchResultPayload {
        call_id: String,
        hits: Vec<SearchHit>,
        truncated: bool,
    },
}
```

### 本机后端实现

- 优先使用 `rg`（通过 `which rg` 检测）
- 构造命令：`rg --json --max-count {max_results} {query} {path}`
- 解析 JSON 输出为结构化 `SearchHit`
- fallback：无 rg 时使用当前逐文件方案（通过本地文件 I/O，仍比 relay 逐文件快）

### 云端 search_text 路由

```rust
pub async fn search_text(...) -> Result<Vec<String>, String> {
    let mount = resolve_mount(...)?;
    match mount.provider.as_str() {
        PROVIDER_RELAY_FS => self.relay_search(mount, query, path, max_results).await,
        PROVIDER_INLINE_FS => self.inline_search(mount, query, path, max_results, overlay).await,
        _ => Err("不支持的 provider".into()),
    }
}
```

## 验收标准

- [ ] 本地工作空间搜索走 ripgrep，不再逐文件 relay 传输
- [ ] 搜索延迟从秒级降到毫秒级（中型项目 < 500ms）
- [ ] 支持正则表达式搜索
- [ ] rg 不可用时自动降级，不影响功能
- [ ] 现有 inline_fs 搜索行为不变（向后兼容）

## 影响范围

| 层 | 文件 | 改动 |
|----|------|------|
| 域 | `relay-protocol` | 新增 `CommandToolSearch` / `ResponseToolSearch` 消息 |
| 本机 | `agentdash-local` | 实现 rg 调用 + fallback |
| 应用 | `address_space_access.rs` | `search_text` 路由分发 |
| API | `address_space_access.rs` | `FsSearchTool` 参数增强 |
