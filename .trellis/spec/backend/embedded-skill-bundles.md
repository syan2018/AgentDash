# Embedded Skill Bundles

Embedded Skill Bundle 是源码内嵌、可同步到 Canvas/VFS/其它受管载体的 skill 文档包。它用于批量携带 `SKILL.md`、`references/`、`scripts/`、`assets/` 等 Agent-facing 文档或资源，避免每个业务域手写一套 `include_str!`、路径常量和同步逻辑。

## Core Contract

- 通用模型定义在 `agentdash-domain/src/embedded_skill.rs`。
- 每个 bundle 必须声明：
  - `name`：skill 名称，例如 `canvas-system`。
  - `root_path`：落到目标载体内的根路径，例如 `skills/canvas-system`。
  - `entry_path`：入口 skill 文件，通常是 `SKILL.md`。
  - `files`：bundle 内所有文件，路径相对 `root_path`。
- `entry_path` 必须存在于 `files`，且对应文件类型必须是 `EmbeddedSkillFileKind::Skill`。
- 文件路径必须是安全相对路径，不得为空、包含 `..`、盘符、绝对路径、空 path segment 或 `:`。
- 业务域不得复制 materialize 逻辑；应调用 `ensure_embedded_skill_bundle`。

## Authoring Layout

推荐保持源码目录与落地目录一致：

```text
crates/agentdash-domain/src/<domain>/skills/<skill-name>/
  SKILL.md
  references/
    api.md
```

然后在领域值对象中声明 bundle：

```rust
const MY_SKILL_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: include_str!("skills/my-skill/SKILL.md"),
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/api.md",
        content: include_str!("skills/my-skill/references/api.md"),
        kind: EmbeddedSkillFileKind::Reference,
    },
];

pub const MY_SKILL_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: "my-skill",
    root_path: "skills/my-skill",
    entry_path: "SKILL.md",
    files: MY_SKILL_FILES,
};
```

## Sync Policy

- 新建受管载体时，默认 materialize 完整 bundle。
- 更新已有载体时，如果该载体已有 bundle entry 文件，可同步完整 bundle，包括新增 reference 文件。
- Materializer 只管理 bundle 声明内的文件；不得删除用户其它文件。
- 若受管文件路径使用 `\`，materializer 应归一为 `/`。
- 若受管文件内容与源码 bundle 不一致，materializer 以源码 bundle 为准覆盖。

## Runtime Builtin SkillAsset Contract

### 1. Scope / Trigger

平台 Assets 展示的是 project-scoped `SkillAsset`，embedded bundle registry 是这些
受管资产的 catalog 事实源。catalog 当前包含 `canvas-system`、
`workspace-module-system`、`companion-system`、`routine-memory` 和
`memory-manager`；新增 catalog 条目会通过相同 provisioning 自动进入所有 Project。

Project 创建、Project clone 和 API 启动都是 provisioning trigger。这样 Assets 的
可见性只取决于 Project 与当前发布物，不取决于某个 Agent 是否运行过；runtime
lifecycle 则只选择本次 frame 需要暴露的已存在资产。

### 2. Signatures

```rust
agentdash_application_skill::skill_asset::SkillAssetService::new(repo)
    .provision_project_builtins(project_id, None)
    .await;

AgentRunLifecycleSurfaceInput {
    explicit_skill_asset_keys,
    builtin_skills: BuiltinLifecycleSkillPolicy::Project(skills),
    ..
}
```

### 3. Contracts

- `agentdash-application-skill` owns catalog lookup、embedded bundle 解析、project
  provisioning、内容同步和 `builtin_seed` mutation 边界。
- `provision_project_builtins(project_id, None)` 幂等收敛 catalog 全集；已有同 key
  snapshot 会保留实体 identity，并同步 source、metadata 与完整 bundle 文件。
- Project create/clone 在项目记录建立后 provision 全集；API bootstrap 在开始服务前
  枚举既有 Project 执行相同 reconciliation，失败信息携带 Project ID 并中止启动。
- `agentdash-application-lifecycle` 只读取并验证最终 key 集合，再把它写入唯一
  `lifecycle` mount metadata。`Project` 表达本次选择的 builtin keys；
  `PreserveProjected` 继承同 Project base VFS 中已有的 keys。
- `LifecycleMountProvider` 根据 mount metadata 暴露
  `lifecycle://skills/<key>/SKILL.md` 及 bundle 文件。
- Project owner 首次 frame 在 composition 前预分配稳定 frame ID，并直接使用
  dispatch 已持有的 run、agent、frame、runtime session 坐标完成 projection。该顺序
  让 mount 建立早于 runtime binding，同时让最终持久化 frame 与 mount metadata
  引用同一个 identity。
- 普通 update、delete、upload overwrite、library install overwrite 与 publish
  surface 都拒绝把 `builtin_seed` 当作用户资产处理；这使 Project 资产始终是当前
  embedded catalog 的受管物化，而不是可漂移或可发布的用户副本。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| catalog template 无效或入口文件缺失 | provisioning 返回 validation error |
| Project 已有同 key snapshot | 收敛为 builtin seed 并同步 embedded files |
| Project 缺少被选择的 builtin/explicit key | projector 返回包含 Project ID 与 key 的 projection error |
| 首次 frame 缺 runtime session ID | frame construction 明确拒绝 |
| builtin update/delete | application service 返回 conflict |
| upload/library install 使用 builtin key | mutation surface 拒绝覆盖受管资产 |
| publish builtin | publish surface 拒绝把受管资产复制为 library asset |
| startup 任一 Project provisioning 失败 | API bootstrap 失败 |

### 5. Good/Base/Bad Cases

- Catalog release：API bootstrap 对每个 Project 调用全集 provisioning，Assets
  随即展示五个受管 Skill；重复启动保持相同资产 identity。
- Project owner launch：caller 传入
  `Project([CompanionSystem, CanvasSystem, WorkspaceModuleSystem])`，projector 验证
  三个资产后把 keys 写入首次 frame 的 lifecycle mount。
- 延续 surface：caller 传入 `PreserveProjected`，projector 读取 base VFS 的 keys
  并重新验证 Project 资产存在性。
- Catalog/Project 状态不一致：projector 显式失败，由 Project provisioning 边界修复
  catalog 状态；runtime projection 不承担数据修复副作用。

### 6. Tests Required

- Skill service 测试覆盖 catalog 全集、重复 provisioning identity、旧 snapshot
  收敛、完整 `SKILL.md` 文件与 builtin mutation conflict。
- lifecycle projector 使用 recording repository 证明 projection 期间
  create/update/delete 调用数为零，并覆盖缺失 key 错误。
- frame builder 测试锁定 build 前后的 frame ID 一致。
- API embedded PostgreSQL 集成测试锁定首次 Project owner launch frame 已包含唯一
  lifecycle mount 及三项默认 builtin keys。
- 前端 typecheck 和 Assets 交互测试锁定 builtin 只呈现查看语义。

### 7. Ownership Flow

```text
EmbeddedSkillBundle registry
  → Project SkillAsset provisioning
  → platform Assets read model
  → frame composition selects keys
  → read-only lifecycle projection
  → LifecycleMountProvider / skill baseline
```

这个职责流把“平台发布了哪些 Skill”“某个 Project 当前拥有哪些受管资产”和“某个
frame 要暴露哪些 Skill”分成三个稳定事实。Canvas、Workspace Module、Companion 与
Routine 只声明选择策略；它们共享同一套 Project 资产与 lifecycle mount，因此 runtime
skill baseline、workspace resource surface 和前端 capability 展示观察到同一组文件。

Canvas runnable asset 继续只保存 `Canvas.files` 的业务源码与数据；`canvas-system`
承载 authoring、runtime bridge、VFS asset、interaction snapshot、render diagnostics
和 submit-to-Agent 协议。`workspace-module-system` 承载 module
list/describe/invoke/present 操作协议。`routine-memory` 与 `memory-manager` 承载
Routine/Memory 协议，而 Routine state 继续由 `routine_vfs` 投影。各领域指南经统一
lifecycle skill surface 可见，原因是它们属于 session 操作协议，而非某个业务实体的
文件树。

## Validation Contract

- bundle materialization 必须覆盖声明内所有文件。
- 已存在受管文件内容漂移时，materializer 以源码 bundle 为准更新。
- 缺少 `entry_path` 时 `validate()` 必须失败。
- 新建受管实体必须包含 bundle entry 和 reference 文件。
- mutation/update 后已有 bundle 必须同步新增/更新文件。
- Skill 作者更新 `SKILL.md` 后，应运行 skill-creator `quick_validate.py` 校验 skill folder。

## Authoring Conventions

- 业务域通过 `EmbeddedSkillBundle` 声明所有 Agent-facing 文件，并复用
  `ensure_embedded_skill_bundle`，使验证、路径归一化和内容同步使用同一实现。
- bundle 新增文件时同步更新 `files` 声明，使编译产物中的 catalog 与源码目录保持
  完整一致。
