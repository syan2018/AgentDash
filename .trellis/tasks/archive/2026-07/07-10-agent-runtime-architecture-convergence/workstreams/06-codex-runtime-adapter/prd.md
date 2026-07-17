# Codex App Server Runtime Adapter

## Goal

按完整 Codex App Server Protocol 重写 adapter，复用成熟 Thread/Turn/Item/Interaction 词汇，同时将 vendor DTO、管理面与 native opaque context限制在adapter内。

## Depends On

- `02-managed-runtime-kernel`
- `03-business-agent-surface`
- `04-integration-driver-host`

## Parent Design

- `../../design.md` 第 8、10 节
- `../../research/codex-app-server-l4-vocabulary.md`
- `../../research/codex-hook-projection.md`

## Requirements

- 统一 Rust/npm/reference protocol revision，避免同一adapter内版本漂移。
- 映射 Thread start/resume/fork/read、Turn start/steer/interrupt、Item lifecycle/delta与source IDs。
- structured/multimodal UserInput、base/developer instructions、additional context、workspace roots无损映射。
- dynamic_tools + `item/tool/call`接Tool Broker。
- approval、user-input、MCP elicitation进入durable Interaction；删除auto accept/empty/null。
- interrupt等待canonical terminal；EOF/transport loss映射Lost。
- 明确thread-static tool surface与context fidelity。
- 对Codex原生支持的trigger，将HookPlan materialize为隔离的Codex配置/脚本并验证applied digest；其余trigger按Host/Broker/Observed能力诚实声明。
- Steer只能表达terminal后或已观察事件后的补充输入，不得冒充BeforeTool block、BeforeProvider改写或BeforeStop same-loop decision。
- 生成按digest不可变的Codex plugin/capability artifact（`hooks/hooks.json` + 单一bridge），通过ThreadStart selected capability root绑定；不以覆盖项目`.codex/hooks.json`作为主路径。
- 只声明当前真正可运行的sync command handler；`hooks/list`与`hook/started/completed`用于discovery/reconcile，不冒充register或Host decision callback。
- AgentDash自行校验manifest/bridge/schema/adapter完整ArtifactDigest并使用digest路径；Codex currentHash只作native trust证据，正式路径禁止`bypass_hook_trust`。
- 首期hook plan update boundary为Binding/ThreadStart/Resume；无reload+ack conformance时不声明HotAcked。
- Native compaction只作Observed/Opaque；只有新增exact context prepare/activate扩展后才声明managed compaction。
- 删除旧Codex bridge、follow-up=fork与cancel=kill语义。

## Acceptance Criteria

- [x] Codex protocol DTO不出adapter crate。
- [x] 图片/structured input、instruction channel与workspace roots端到端无损。
- [x] Dynamic tool call有identity/policy/result/terminal闭环。
- [x] Approval/UserInput不会自动决定，pending interaction可恢复。
- [x] Interrupt accepted与turn terminal分离，EOF不Completed。
- [x] `thread/read`不冒充exact context，opaque compact不推进platform head。
- [x] 未实现hot update时typed说明boundary/rebind，不假成功。
- [x] PreToolUse block/rewrite、Permission allow/deny与Stop continuation在side effect/terminal candidate前真实生效，并关联canonical HookRun。
- [x] Bridge timeout按required rule failure policy收敛，duplicate callback与late notification不重复effect。
- [x] linked worktree与脚本内容替换场景通过artifact digest/path/trust tests。
