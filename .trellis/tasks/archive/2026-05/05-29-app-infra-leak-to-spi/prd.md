# application 基础设施泄漏下沉为 SPI port

> 病灶 1。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> 依赖：`drop-step-lifecycle` 之后（agent_executor 已瘦身）。

## Scope
application 层不得直接持有基础设施。为每类外部 IO 定义 SPI port（在 spi 声明 trait），实现下沉 infrastructure，application 仅依赖 trait。

## 证据 + 拆解（逐 port 可独立 gate + commit）
1. **RemoteSkillSource**：`skill_asset/service.rs:328` 直接 `reqwest::Client` 抓 GitHub/ClawHub/skills.sh（`fetch_github_skill_files`/`fetch_clawhub_skill_files`/`fetch_skills_sh_skill_files`，:639–890）。抽 `RemoteSkillSource` port（locator→`Vec<SkillFileInput>`），三 provider 实现移 infrastructure。合并 `RawSkillUploadFile`≈`SkillAssetFileInput`。service.rs 目标 ~500 行。
2. **McpProbeTransport**：`mcp_preset/probe.rs:13` 直接 new `rmcp` StreamableHttp。抽 port，rmcp 实现下沉。
3. **HookScriptEvaluator**：`hooks/script_engine.rs:5` 直接嵌 `rhai`。抽 port（context snapshot→`ScriptDecision`），rhai 实现下沉。
4. **FunctionRunner**：`workflow/agent_executor.rs:629-748` 直接 `reqwest` + `tokio::process::Command`（`execute_api_request`/`execute_bash`）+ tera 渲染。下沉 infrastructure 的 function runner，application 持 port。
5. **MemorySessionPersistence 移出**：`session/memory_persistence.rs`(1466 行内存存储)移 infrastructure（或 test-support），`#[cfg(test)]`/feature 门控，剔出 release。

## Acceptance
- [ ] application crate `grep -rn "reqwest::\|rmcp::\|rhai::\|tokio::process" crates/agentdash-application/src/` 归零（或仅剩 port 定义无具体客户端构造）
- [ ] `MemorySessionPersistence` 不在 release 产物
- [ ] `cargo check --workspace` 通过

## Constraints
- 改 `crates/` 多个（application/spi/infrastructure）。**不要 git commit**，orchestrator 逐 port gate 后提交。
- 行为不变，仅搬运 + 抽象。每个 port 可独立交付。
