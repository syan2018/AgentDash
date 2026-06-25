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

Canvas authoring 协议使用项目级内嵌 Skill 路径：`canvas-system` 先通过
`SkillAssetService::bootstrap_builtins(project_id, Some(key))` 同步到项目 SkillAsset，
再经 AgentRun lifecycle VFS projection 暴露给 session。这样 Canvas runnable asset
只保存 `Canvas.files` 的业务源码与数据文件，Agent-facing authoring 指南由 lifecycle skill
surface 统一提供。`canvas-system` 必须描述 Canvas authoring、runtime bridge、VFS asset、
interaction snapshot、render diagnostics 和 submit-to-Agent 的当前协议，因为这些能力都是
Agent 在同一 Canvas runtime surface 上可观察或可触发的操作面。

项目级内嵌 Skill 当前通过 `SkillAssetService::bootstrap_builtins(project_id, Some(key))`
同步到项目 SkillAsset，再由 AgentRun lifecycle VFS projection 暴露给 session。
`AgentRunLifecycleSurfaceProjector` 通过 `BuiltinLifecycleSkillPolicy` 表达是否只保留已有
projection，或 ensure 并投影 `companion-system` / `workspace-module-system` / `routine-memory`。
projector 将 SkillAsset keys 写入唯一 `lifecycle` mount metadata，由 `LifecycleMountProvider`
在 `lifecycle://skills/<key>/...` 下暴露同一组 skill 文件。
同一个 lifecycle mount 可由 agent preset、companion system、workspace module system 或 routine
memory 多个来源追加 SkillAsset key；projection helper 负责合并去重，原因是执行器 skill baseline、
AgentRun workspace resource surface 和前端 capability 展示必须观察同一组已投影 skill。
这样 session 的 skill baseline 与 lifecycle runtime 上下文使用同一条 mount 事实源，
同时保持 skill 内容仍由 embedded bundle 与项目 SkillAsset 管理。Project SkillAsset
文件管理 surface 继续使用 `skill_asset_fs` provider 直接浏览和编辑项目级 Skill 文件。

Routine 信息管理使用同一条项目级内嵌 Skill 路径：`routine-memory` 先通过
`SkillAssetService::bootstrap_builtins(project_id, Some(key))` 同步到项目 SkillAsset，
再在 Routine frame construction 中通过 lifecycle VFS projection 注入。这样 Routine
Session 默认具备 memory 协议说明，而 skill 内容仍由 embedded bundle 与 SkillAsset
管理；`routine_vfs` 只负责 Routine state projection。

Workspace Module 操作协议也使用项目级内嵌 Skill 路径：`workspace-module-system` 应作为 builtin SkillAsset
同步到项目 SkillAsset，并在 session 具备 `workspace_module` capability 时经 lifecycle VFS projection
暴露。这个 skill 只描述 Agent 调用 `workspace_module_operate/list/describe/invoke/present` 的顺序、
`canvas:{canvas_mount_id}` / `ext:{extension_key}` / `builtin:{key}` module id 形态、describe 返回的
operation schema 是调用事实源，以及 Canvas work 如何进入 `canvas-system` 指南。原因是 workspace module 与 canvas authoring 都是 session 级
Agent 操作协议，统一经 lifecycle skill surface 投影，避免指南可见性绑定到某个 Canvas 实例文件树。

`workspace-module-system` 的最小注册建议：

- 在 domain 层按现有 embedded bundle 模式声明 `WORKSPACE_MODULE_SYSTEM_BUNDLE`，文件根为 `skills/workspace-module-system`。
- 在 builtin SkillAsset template 列表中加入 `workspace-module-system`。
- 在 session assembly 中，当 effective capability 包含 `workspace_module` 时 bootstrap 该 builtin 并把 key 加入 lifecycle skill projection。
- 注册代码应复用 `SkillAssetService::bootstrap_builtins(project_id, Some(key))` 与 `append_lifecycle_skill_asset_projection`，保持项目级 skill 内容、lifecycle mount 和 skill baseline 使用同一事实源。

## Validation Contract

- bundle materialization 必须覆盖声明内所有文件。
- 已存在受管文件内容漂移时，materializer 以源码 bundle 为准更新。
- 缺少 `entry_path` 时 `validate()` 必须失败。
- 新建受管实体必须包含 bundle entry 和 reference 文件。
- mutation/update 后已有 bundle 必须同步新增/更新文件。
- Skill 作者更新 `SKILL.md` 后，应运行 skill-creator `quick_validate.py` 校验 skill folder。

## 禁止模式

- 每个业务域手写 `include_str!` + find/update/push 同步逻辑（应使用 `ensure_embedded_skill_bundle`）
- 新增文件时不加入 bundle 声明
