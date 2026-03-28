# Hook Snapshot Metadata 强类型化

## Goal

将 `SessionHookSnapshot.metadata` 从 `Option<serde_json::Value>` 替换为强类型的 `SessionSnapshotMetadata` struct，消除 `snapshot_helpers.rs` 中大量 `.get("key")?.as_str()` 式的 stringly-typed 访问，获得编译期类型安全。

## 背景

当前 `crates/agentdash-connector-contract/src/hooks.rs` 中：

```rust
pub struct SessionHookSnapshot {
    // ...
    pub metadata: Option<serde_json::Value>,
}
```

而 `crates/agentdash-application/src/hooks/snapshot_helpers.rs` 中有大量如下模式：

```rust
snapshot.metadata.as_ref()?.get("active_workflow")?.get("step_key")?.as_str()
snapshot.metadata.as_ref()?.get("active_task")?.get("status")?.as_str()
```

这种访问方式：
- 无编译期检查 — 字段名拼错运行时才发现
- 难以重构 — 改字段名需全局搜索字符串
- 类型信息丢失 — 所有值都是 `serde_json::Value`

## Requirements

### 定义 `SessionSnapshotMetadata` 强类型

在 `agentdash-connector-contract/src/hooks.rs` 中新增：

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSnapshotMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task: Option<ActiveTaskMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workflow: Option<ActiveWorkflowMeta>,

    // session-level 运行时字段（之前散落在 SESSION_LEVEL_METADATA_KEYS 中）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,

    /// 保留扩展口 — 非核心字段仍可用 JSON
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTaskMeta {
    pub task_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveWorkflowMeta {
    pub lifecycle_id: Option<String>,
    pub lifecycle_key: Option<String>,
    pub run_id: Option<String>,
    pub step_key: Option<String>,
    pub step_advance: Option<String>,
    pub default_artifact_type: Option<String>,
    pub default_artifact_title: Option<String>,
    pub checklist_evidence_present: Option<bool>,
    pub effective_contract: Option<serde_json::Value>,
}
```

### 替换 `SessionHookSnapshot.metadata` 字段

```rust
pub struct SessionHookSnapshot {
    // ...
    pub metadata: Option<SessionSnapshotMetadata>,  // 原: Option<serde_json::Value>
}
```

### 迁移 `snapshot_helpers.rs`

将所有 `.get("key")?.as_str()` 链式访问替换为直接字段访问：

```rust
// Before:
snapshot.metadata.as_ref()?.get("active_workflow")?.get("step_key")?.as_str()

// After:
snapshot.metadata.as_ref()?.active_workflow.as_ref()?.step_key.as_deref()
```

### 迁移 `hooks/mod.rs` 中的 metadata 构建

将 `serde_json::json!({ "active_task": { ... } })` 替换为结构体构造：

```rust
// Before:
metadata.insert("active_task".to_string(), serde_json::json!({ "task_id": id }));

// After:
meta.active_task = Some(ActiveTaskMeta { task_id: Some(id), ..Default::default() });
```

### 迁移 `SESSION_LEVEL_METADATA_KEYS` preserve 逻辑

当前的 `preserve_session_level_metadata` 函数通过遍历 key 数组拷贝字段；替换为对 `SessionSnapshotMetadata` 的逐字段合并。

## Acceptance Criteria

- [ ] `SessionHookSnapshot.metadata` 类型为 `Option<SessionSnapshotMetadata>`
- [ ] `snapshot_helpers.rs` 中不再有 `.get("字符串key")` 模式
- [ ] `hooks/mod.rs` 中 metadata 构建使用结构体而非 `serde_json::json!`
- [ ] `preserve_session_level_metadata` 使用类型安全的字段合并
- [ ] `#[serde(flatten)] extra` 保留扩展能力 — 不影响未知字段的序列化/反序列化
- [ ] 所有现有 test 通过
- [ ] `cargo check --workspace` 无错误

## Technical Notes

- `#[serde(flatten)]` 的 `extra: serde_json::Map<String, Value>` 确保未来新增的、非核心字段仍可 fallback 到 JSON
- `effective_contract` 保持 `Option<serde_json::Value>` — 因为其内容本身是动态 schema 的 workflow contract
- 此 task 建议在 `hook-provider-decompose` 之后执行，因为拆分后各子服务的 metadata 构建逻辑更清晰

## 依赖

- 前置：`03-28-hook-provider-decompose`（建议但非强制）

## 优先级

P0 — 高优先级，消除大量潜在运行时错误
