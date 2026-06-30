# 架构最终彻底收口设计

## Strategy

本任务把上一轮 review 后仍然偏“处理面”的四类问题放进同一个收口篮子。目标不是继续证明方向可行，而是把真实使用路径切到新的深模块边界上：质量门由 manifest 驱动，运行时流校验由生成契约或集中校验模块驱动，WorkspaceModule surface 返回领域 outcome，AgentRun 控制面由直接模型测试兜住。

任务保持单体管理，不创建子任务。执行时可以按工作线并行派发，提交时按工作线拆分。

## Workstream A: Quality Gate Manifest Adoption

### Current Shape

`scripts/lib/quality-gates.js` 已经表达了 gate 到命令的结构关系，但 CI workflow 与根脚本仍可能保留重复命令编排。只要 manifest 不被真实采用，后续新增或调整检查项仍会出现“本地清单正确、CI 另有一套”的漂移。

### Target Shape

- `scripts/quality-gates.js run <gate>` 成为 CI 可调用入口。
- PR quick、deployment contract、full local 等 gate 的命令组成只在 manifest 中维护。
- workflow 保留环境安装、缓存、产物上传等 CI 编排；具体检查命令从 manifest 进入。
- package/root scripts 使用相同入口，避免本地和 CI 分叉。
- 测试覆盖 gate 展示、命令展开、未知 gate 报错和 CI 入口行为。

### Design Notes

CI workflow 不应被抽象到不可读；setup 仍属于 workflow，检查命令属于 manifest。manifest runner 需要输出足够清晰，失败时能定位具体 step。

## Workstream B: Generated Runtime Validators

### Current Shape

前端 NDJSON 流解析已经消费生成的 TypeScript union，但运行时 envelope 验证仍主要依赖手写 shape 判断。类型生成解决了静态漂移，运行时入口仍可能漂移。

### Target Shape

默认目标是在 `agentdash-contracts` 生成链路中输出运行时校验能力或元数据，至少覆盖当前 stream envelope 的核心 tagged union/struct。前端 stream parser 直接消费生成结果，手写代码只保留流协议级组装、错误上下文和日志语义。

### Design Notes

- 优先在现有生成器内扩展，不新增大型 schema 框架，除非仓库已有明确依赖和收益。
- 生成物应可由 `pnpm contracts:check` 或等价命令验证，避免手改生成文件。
- 先覆盖 `SessionNdjsonEnvelope` 与 `ProjectEventStreamEnvelope` 等实际流入口，再评估是否泛化到全部 DTO。
- 如果生成层遇到不可绕开的语义缺口，必须把缺口写进任务实施记录，并实现最强集中化替代方案：例如 formal `ndjsonEnvelopeValidator` 模块加 cross-fixture 测试，而不是继续散落在 hook 内。

## Workstream C: WorkspaceModule Pure Outcome

### Current Shape

`WorkspaceModuleAgentSurface` 已经把 workspace module 的 invoke/present 行为集中起来，但 outcome 中仍有 `AgentToolResult` 泄漏。这让 surface 既承担领域决策，又知道工具 SPI 的返回形态。

### Target Shape

- surface 返回 `WorkspaceModuleOperationOutcome` 下的领域化 variants。
- invoke 分支返回 runtime action、channel result、canvas binding、inspection、interaction state、diagnostic 等明确 outcome。
- present 分支保持 presentation 语义，不把 tool protocol 作为 surface 的内部类型。
- 工具 adapter 是唯一将领域 outcome 投影成 `AgentToolResult` 的地方。
- surface 层测试直接覆盖 outcome；adapter 层测试覆盖投影。

### Design Notes

这条线应优先删除 SPI 反向依赖，而不是包一层同名类型。完成后，新增 workspace module 能力时应先扩展领域 outcome，再由 adapter 决定对外工具表现。

## Workstream D: AgentRun Control-Plane Direct Tests

### Current Shape

AgentRun 工作台控制面已从 UI 页面中抽出来，但现有覆盖更多来自 page、row 或 ChatView 的 walkthrough。此覆盖能证明主流程可用，却不利于快速锁定命令模型和状态投影错误。

### Target Shape

- 对 `useAgentRunWorkspaceControlPlane` 或其下沉的纯投影函数建立直接测试。
- 覆盖 conversation command state、刷新状态、提交/取消/提升、presentation 映射、禁用态和错误态。
- 页面测试保留用户路径信心，直接测试承担模型正确性。
- 如 hook 环境过重，可以提取纯 control-plane model/projection 函数，但提取本身要让接口更深，而不是为测试复制一套状态机。

## Boundaries

- 不做兼容迁移层。
- 不创建子任务，除非用户明确重新拆分。
- 不把四条线揉成一个提交。
- 不为当前任务写只记录历史错误的文档；规格只记录未来开发应遵守的原因和约定。
