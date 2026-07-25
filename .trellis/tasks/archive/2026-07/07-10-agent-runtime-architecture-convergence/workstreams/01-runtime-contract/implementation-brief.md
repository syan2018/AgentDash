# 工作包 01 Implement Brief

仅实施父任务工作包01：Runtime Contract、Runtime Wire与Conformance Harness。

## 交付范围

- 新建`agentdash-agent-runtime-contract`，承载AgentDash-owned IDs、commands、events、snapshots、profiles、availability、errors与Driver SPI所需canonical types。
- 新建`agentdash-agent-runtime-wire`，承载typed request/response/notification envelope、protocol revision、critical frame与protocol violation。
- 新建`agentdash-agent-runtime-test-support`或等价的dependency-light conformance harness，覆盖工作包PRD要求的共同不变量。
- 将新crate加入workspace，建立最小、正确的依赖方向与Rust测试。
- 提供Rust contract同源的TypeScript/JSON Schema生成入口与受控生成产物；不得引用Codex/ACP/vendor DTO。

## 强制边界

- 不实现Managed Runtime数据库、具体Driver、Application facade、Native/Codex Adapter或UI切换。
- 不在旧`AgentConnector`、Backbone或`application-runtime-session`上增加兼容facade。
- canonical/source IDs必须是不可混用的newtypes。
- command、event、profile与error必须typed；核心生命周期不允许arbitrary JSON escape hatch。
- Level只做reference class；availability使用typed predicates/profile。
- unsupported必须在side effect前返回typed error，不提供default no-op。
- authoritative final item与exactly-one terminal必须有executable conformance tests。
- 保留工作区中主会话已经产生的`task.json`与本brief，不提交、不推送、不合并。

## 工作方式

1. 完整读取`implement.jsonl`、本目录`prd.md`、父任务`design.md`、`target-crate-shape.md`与相关spec。
2. 先搜索现有newtype、schema generation、wire envelope与test-support模式，复用真正有所有权的模式。
3. 实现后运行format、目标crate tests/check及必要schema drift验证。
4. 通过Trellis channel报告修改文件、设计决策、验证结果与未决问题。
