# 代码质量与高耦合清理总览

## Goal

从全仓 review 中沉淀快清项与架构级高耦合清理任务。

## Requirements

- 将本次全仓 review 中需要持续追踪的问题收敛为架构级清理项。
- 边界明确的快清项不创建 Trellis 子任务，直接通过独立代码提交说明目标和验证结果。
- 架构级子任务只作为追踪入口，启动实现前必须补充对应 `design.md` 与 `implement.md`。
- 所有清理遵守项目当前阶段约束：不保留兼容层，不为旧字段或旧协议形态增加回退路径。
- 任务树作为后续工作的索引，父任务不直接承载代码实现。

## Acceptance Criteria

- [ ] 已创建架构级追踪子任务，每个任务都有明确目标、范围和验收标准。
- [ ] 父任务能说明子任务之间的关系和建议处理顺序。
- [ ] 后续启动任一复杂子任务前，都能从其 PRD 看出为什么需要设计阶段。
- [ ] 边界明确的快清项已通过独立 commit 表达变更范围和验证结果。

## Child Task Map

- `06-10-architecture-application-boundary-cleanup`：追踪 application 层对 executor、relay、contracts 的边界去耦。
- `06-10-api-contract-dto-boundary-cleanup`：追踪 API routes 返回 contract DTO 的边界收敛。
- `06-10-extension-manifest-single-source`：追踪 extension manifest、SDK、validator、Rust domain 的单一事实源。
- `06-10-legacy-local-runtime-identity-cleanup`：追踪 legacy 本机身份链路删除。
- `06-10-workspace-tab-runtime-decoupling`：追踪 workspace tab store 与 render registry 解耦。
- `06-10-relay-protocol-current-model-cleanup`：追踪 relay protocol 当前模型收敛和文档更新。

## Suggested Order

1. 先启动 `application 边界去耦`，因为它会影响 executor、relay、contracts 的后续收敛方式。
2. 然后按业务影响启动 `extension manifest 单一事实源` 与 `API contract DTO 边界收敛`。
3. 最后处理 legacy 身份链路、workspace tab runtime、relay protocol 文档与语义收紧。

## Notes

- 本父任务不应直接 start 实现；架构级工作以子任务为单位推进。
