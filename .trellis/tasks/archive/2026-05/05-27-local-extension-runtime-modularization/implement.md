# Local Extension Runtime 模块目录化 Implement Plan

1. 移动文件：
   - `extension_artifact_cache.rs` -> `extensions/artifact_cache.rs`
   - `extension_host.rs` -> `extensions/host.rs`
2. 新增 `extensions/mod.rs` 并 re-export 稳定入口。
3. 更新 `lib.rs` 模块声明与 re-export。
4. 修正 `host.rs` 对 artifact cache entry 的 sibling module import。
5. 运行：

```powershell
cargo test -p agentdash-local extensions
cargo check -p agentdash-local
```
