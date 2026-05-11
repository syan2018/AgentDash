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

Canvas 当前使用：

```rust
ensure_embedded_skill_bundle(files, &CANVAS_SYSTEM_BUNDLE)
```

`ensure_canvas_system_skill` 只是 Canvas 兼容包装，不应继续扩展手写文件同步逻辑。

## Tests Required

- Domain 单测覆盖：
  - bundle 能 materialize 所有文件。
  - 已存在文件内容漂移时会被更新。
  - 缺少 `entry_path` 时 `validate()` 失败。
- 业务域单测覆盖：
  - 新建实体包含 bundle entry 和 reference 文件。
  - mutation/update 后已有 bundle 会同步新增/更新文件。
- Skill 作者更新 `SKILL.md` 后，应运行 skill-creator `quick_validate.py` 校验 skill folder。

## Wrong vs Correct

### Wrong

```rust
const MY_SKILL_MD: &str = include_str!("skills/my-skill/SKILL.md");
const MY_REFERENCE: &str = include_str!("skills/my-skill/references/api.md");

fn ensure_my_skill(files: &mut Vec<MyFile>) {
    // 手写 find/update/push，多处重复
}
```

问题：每个业务域复制同步逻辑，新增 reference 时容易漏同步或漏测试。

### Correct

```rust
pub const MY_SKILL_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle { ... };

fn ensure_my_skill(files: &mut Vec<MyFile>) -> bool {
    ensure_embedded_skill_bundle(files, &MY_SKILL_BUNDLE)
        .expect("embedded skill bundle should be valid")
        .changed()
}
```

这样新增文件只需要加入 bundle 声明，受管载体的同步规则保持一致。
