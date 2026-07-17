# Main 对照恢复基线

## Git基线

- 当前任务实现起点：`36c5484e6`之后的任务重订提交。
- 生产oracle：`D:\Projects\AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- main reference worktree只读；W0直接从该固定oracle捕获deterministic fixtures，产物只写当前共享工作区，不创建临时worktree或第二套Cargo target；Cargo锁由共享target自然排队。

## 已确认的失败方向

当前`36c5484e6`仍由frontend `runtimeSessionAdapter.ts`把`RuntimeEventEnvelope`反向投影成session event；Codex/Native/tool producer也先压成较窄RuntimeEvent。这与“wrapper可变、presentation body完全一致”冲突。

已确认需要W0逐项重验的高风险差异：

| 范围 | 当前风险 |
| --- | --- |
| User | TurnStarted/UserInput顺序、source metadata、Text/Image/LocalImage/Skill/Mention表达 |
| Assistant | Native Assistant MessageStart产生空ItemStarted，进入旧ToolCall renderer |
| Reasoning | 与assistant共享identity，缺独立durable terminal |
| Tool | 无完整ItemUpdated；ToolProgress/typed delta、terminal payload不等价 |
| Codex | source ID canonicalize；diff/plan/status/title/usage/error/compaction/interaction信息压缩 |
| Tool taxonomy | 新Runtime presentation discriminant与main实际ThreadItem/Platform表达不同 |
| Platform | 原title/hook/context/terminal/PTY/control-plane/rewind等producer缺失或改为internal fact |
| History | fork inherited prefix、journal identity、NDJSON ordering/heartbeat/reconnect需要恢复 |
| Frontend | session代码认识RuntimeEvent；AgentRun outer command/fork/mailbox/context/page与main不同 |

## W0输出账本

W0必须生成并维护：

1. `BackboneEvent` variant → main producer → current owner → fixture。
2. `PlatformEvent` variant → main producer → current owner → fixture。
3. Codex method/request → owned event → fixture。
4. Native AgentEvent → ordered presentation events → fixture。
5. Tool contribution → main ThreadItem/Platform → full lifecycle fixture。
6. main route/service → current route/service → observable behavior。
7. main frontend file → current file → explicit allowed seam或恢复状态。
8. browser scenario → expected entries/cards/actions/side effects。

任何空owner、空fixture或“由Runtime summary推断”的行都视为未完成。

## 禁止回归模式

- API/frontend `match RuntimeEvent`后`serde_json`重造presentation。
- 为通过测试过滤status/progress/hook/interaction terminal。
- 读取journal时生成新timestamp或ID。
- 空turn、空AgentMessage item、generic tool card或unknown→AgentMessage。
- total=last、error详情拍平、structured input文本化。
- 只按variant/profile/类型检查宣告full fidelity。

W0应把这些模式做成静态审计或负例fixture，防止再次出现“局部绿灯、整体行为错误”。
