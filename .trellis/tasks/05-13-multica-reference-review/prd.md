# 调研 multica 项目并纳入 references

## Goal

将 `https://github.com/multica-ai/multica` 拉取到本项目 `references/` 目录，按现有参考仓管理方式登记，并基于源码 review 其多个模块，提炼相对当前 AgentDashboard 项目值得学习的设计与实现经验。

## What I already know

* 用户希望把 multica 添加到 references 中，并 review 多个模块。
* 用户特别提醒：不要让 subagent 递归调用 subagent 导致卡死。
* 本项目 AGENTS.md 明确要求当前环境不要使用 `spawn_agent` 或派发任何 subagent。
* 现有 `references/` 使用本地参考仓目录加 `references/repositories.json` 索引；`references/.gitignore` 会忽略部分参考仓目录。
* 本次调研不涉及当前项目代码实现修改，重点是资料纳入与设计学习总结。

## Assumptions

* `references/multica` 是合适的本地路径。
* 外部参考仓源码应被 `references/.gitignore` 忽略，只登记索引，避免把整个外部仓提交进主仓。
* review 输出以中文总结为主，优先关注与当前项目架构相关的模块设计、工程组织、可借鉴点和不适合直接照搬的差异。

## Requirements

* 拉取 multica 仓库到 `references/multica`。
* 更新 `references/repositories.json`，登记 multica 的 name/path/url/branch。
* 更新 `references/.gitignore`，保持 multica 参考仓目录不进入主仓源码跟踪。
* 从 multica 的项目结构、核心模块、前后端/运行时/工具链等多个角度做 review。
* 对照 AgentDashboard 的当前定位，输出“值得学习之处”和“需要谨慎对待之处”。
* 全程不派发 subagent。

## Acceptance Criteria

* [x] `references/multica` 存在且可读取源码。
* [x] `references/repositories.json` 包含 multica 条目。
* [x] `references/.gitignore` 忽略 `multica/`。
* [x] 调研笔记落在本任务目录下，便于后续追溯。
* [x] 最终回复用中文给出模块 review 和对当前项目的启发。

## Definition of Done

* 完成 clone / pull 验证。
* 完成 references 索引与忽略规则更新。
* 完成源码结构扫描和重点模块阅读。
* 不改动用户已有未提交业务变更。
* 明确说明本次未使用 subagent。

## Out of Scope

* 不将 multica 的实现直接迁移到 AgentDashboard。
* 不改动 AgentDashboard 运行时代码。
* 不创建兼容性层或回退方案。
* 不做完整安全审计、性能基准测试或许可证法律审查。

## Technical Notes

* 当前工作树已有用户/其他任务变更：`crates/agentdash-application/src/vfs/tools/fs.rs` 和 `.trellis/tasks/05-13-vfs-uri-rewrite/`，本任务不触碰它们。
* `references/repositories.json` 目前登记了 codex、pi-mono、rig、Trellis。
* `references/.gitignore` 目前忽略 codex、claude-code、pi-mono、rig、Trellis、zed、GSD-2、Actant、AgentDispatch、MeFinks。
