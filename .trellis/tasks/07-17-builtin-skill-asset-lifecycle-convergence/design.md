# 内嵌 Skill 资产生命周期与 Runtime 投影设计

## 1. 边界与事实源

### Platform builtin catalog

`agentdash-application-skill::skill_asset::definition` 中的
`BUILTIN_SKILL_TEMPLATES` 是平台内嵌 Skill 的唯一 catalog。每个模板引用一个
通过 `include_str!` 编译进 binary 的 `EmbeddedSkillBundle`。

### Project builtin provisioning

`SkillAssetService` 提供显式、幂等的 project provisioning：

```rust
SkillAssetService::provision_project_builtins(project_id, builtin_key).await
```

- `builtin_key = None` provision catalog 全集。
- 新建/clone Project 在项目记录建立后调用全集 provisioning。
- API repository bootstrap 在对外提供服务前枚举所有 Project，并调用全集
  provisioning；任一 Project 失败即启动失败。
- 同 key 的旧 snapshot 收敛为 `builtin_seed`，内容与 embedded bundle 一致。

### Runtime lifecycle projection

`AgentRunLifecycleSurfaceProjector` 只执行：

1. 根据 policy 计算 builtin keys；
2. 合并 explicit keys 或已投影 keys；
3. 从 `SkillAssetRepository` 验证每个 key 在 Project 中存在；
4. 写入 lifecycle mount metadata。

projector 不调用 provisioning，也不写 repository。

## 2. Runtime 坐标与 frame identity

`AgentFrameBuilder` 在构造时预分配 frame ID，并通过 `frame_id()` 暴露。最终
`build_uncommitted()` 必须使用同一 ID。

ProjectAgent owner composition 的输入直接携带 dispatch 已知的
`runtime_session_id`。composer 使用：

```text
run.id
agent.id
builder.frame_id()
runtime_session_id
```

构造 `AgentRunRuntimeAddress` 和 `MessageStreamProjectionRef`，不再调用
`resolve_runtime_surface_refs()` 反查尚未建立的 binding。

这样首次 launch 的顺序是：

```text
provisioned Project assets
  → allocate frame identity
  → project lifecycle surface
  → build/persist same frame
  → provision runtime binding
```

## 3. Policy hard cut

将：

```rust
BuiltinLifecycleSkillPolicy::EnsureAndProject(skills)
```

替换为：

```rust
BuiltinLifecycleSkillPolicy::Project(skills)
```

`Project` 只表示把指定 builtin keys 写入本次 projection；不含 ensure、seed 或
reconciliation 语义。`PreserveProjected` 继续表示读取 base VFS 中相同 Project
的已有 keys。

## 4. 受管资产 mutation

`builtin_seed` 是 catalog 在 Project 中的受管物化，不是用户副本：

- `SkillAssetService::update` 对 builtin 返回 conflict。
- `SkillAssetService::delete` 对 builtin 返回 conflict。
- upload overwrite 与 Shared Library install overwrite 拒绝覆盖 builtin key。
- Shared Library publish 拒绝把 builtin 复制为 library asset。
- provisioning 内部保留覆盖 embedded template 的特权同步路径。
- 前端 builtin 卡片只提供查看入口，不展示删除/发布动作。

`skill_asset_fs` 已是只读 provider，无需增加第二套写保护。

## 5. 数据与 migration

本任务不改变表结构。既有数据库通过应用启动时的幂等 provisioning 收敛；这不是
兼容 fallback，而是 builtin catalog 版本随发布物启动时必须完成的标准
reconciliation。新 Project 在创建路径立即获得相同状态。

## 6. 错误与失败策略

| 条件 | 行为 |
| --- | --- |
| embedded template 无效 | provisioning 返回错误；启动或 Project 创建失败 |
| 既有同 key user snapshot | 转换并同步为 builtin seed |
| projector 找不到声明 key | 返回含 project ID/key 的 projection error |
| 首次 frame 缺 runtime session ID | frame construction 明确拒绝 |
| builtin update/delete | conflict |
| startup 任一 Project reconciliation 失败 | API 启动失败 |

## 7. 验证策略

- skill service：全集 provisioning、幂等同步、受管 mutation 拒绝。
- lifecycle projector：纯读取、缺失失败、keys metadata。
- frame builder：预分配 ID 与最终 frame ID 一致。
- launch frame adapter/owner composer：无 runtime binding 首次构造仍投影三个默认
  builtin keys。
- project create/bootstrap：新旧 Project 均得到五个 builtin。
- 前端：builtin menu 为只读且无删除。

## 8. Workspace Module presentation 闭环

`workspace_module_present` 是 presentation intent，不是 resource-surface mutation。
成功路径向当前 AgentRun canonical journal 追加：

```text
Platform(ControlPlaneProjectionChanged {
  projection: resource_surface,
  reason: workspace_module_presented,
  workspace_module_presentation: {
    module_id,
    view_key,
    renderer_kind,
    presentation_uri,
    title,
    payload,
  }
})
```

前端从 typed payload 读取 concrete `presentation_uri` 并立即执行 panel open。打开动作
不等待 Workspace state/catalog refresh，原因是该事件已经携带完整 tab identity，且
present 不改变 AgentFrame、mount 或 tool surface。Canvas 用户主动打开已有 tab 时的
content refresh 是另一个显式动作，不与 Agent presentation intent 共用状态标记。

`workspace_module_presented` 同时是可渲染的成功事件，聊天历史可以回放这条事实。外部
panel open 仍只消费 `historyReplayBoundarySeq` 之后的 live 事件，原因是历史事实不能在
用户关闭面板后反复重放 UI 意图；已打开 tab 的恢复由 AgentRun workspace layout
持久化负责。
