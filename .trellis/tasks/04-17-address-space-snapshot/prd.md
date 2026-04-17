# Address Space 快照与回滚机制

## 背景

当前 Address Space 没有点时间冻结与回滚能力。Agent 执行有风险的操作（批量文件修改、配置变更、workflow step 产出覆写）后，如果结果不符合预期，只能靠 git（仅 relay_fs）或手动恢复。对于 `inline_fs`、`canvas_fs`、`lifecycle_vfs` 等非物理 mount，没有任何回退手段。

类比：ZFS/Btrfs 的快照——在任意时刻冻结文件系统状态，后续修改不影响快照，需要时瞬间回滚。成本极低（Copy-on-Write），但提供了强大的安全网。

## 目标

为 Address Space 提供轻量级的快照（snapshot）和回滚（rollback）能力，让 Agent Workflow 中的高风险操作有安全回退路径。

---

## 核心场景

### 场景 1：Agent 执行前快照

Workflow step 开始执行前，自动对目标 mount 做快照。如果 step 执行失败或产出不符合预期（人工审核不通过），可以回滚到快照状态。

### 场景 2：用户手动快照

用户在 UI 上对某个 mount 做手动快照（"保存当前状态"），然后放心让 Agent 做大规模修改。不满意时一键回滚。

### 场景 3：Workflow 分支探索

Agent 在某个决策点需要尝试多种方案——快照当前状态，尝试方案 A；不满意则回滚，尝试方案 B。配合未来的 session branching 使用。

---

## 设计方向

### 快照层级

不做全 Address Space 级快照（多 mount 联合快照一致性问题复杂），而是做 **单 Mount 级快照**。

### 分 Provider 策略

| Provider | 快照策略 | 说明 |
|----------|---------|------|
| `relay_fs` | 委托 git | `git stash` 或 `git commit` 作为快照点；回滚用 `git checkout`。不重新发明 |
| `inline_fs` | DB 级快照 | 将当前 mount 下所有 inline files 的内容序列化为一个快照记录（JSON blob 或独立表） |
| `canvas_fs` | 同 inline_fs | Canvas 内容序列化为快照记录 |
| `lifecycle_vfs` | 只读居多，不需要 | Port output 是 append-only，天然有历史；状态文件是 projection |
| 外部插件 | 按需实现 | 默认 NotSupported |

### SPI 层设计草案

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSnapshot {
    pub id: String,
    pub mount_id: String,
    pub created_at: i64,
    pub label: Option<String>,  // 可选的人类可读标签
}

// MountProvider trait 新增可选方法
async fn create_snapshot(
    &self,
    mount: &Mount,
    label: Option<&str>,
    ctx: &MountOperationContext,
) -> Result<MountSnapshot, MountError> {
    Err(MountError::NotSupported(...))
}

async fn restore_snapshot(
    &self,
    mount: &Mount,
    snapshot_id: &str,
    ctx: &MountOperationContext,
) -> Result<(), MountError> {
    Err(MountError::NotSupported(...))
}

async fn list_snapshots(
    &self,
    mount: &Mount,
    ctx: &MountOperationContext,
) -> Result<Vec<MountSnapshot>, MountError> {
    Err(MountError::NotSupported(...))
}

async fn delete_snapshot(
    &self,
    mount: &Mount,
    snapshot_id: &str,
    ctx: &MountOperationContext,
) -> Result<(), MountError> {
    Err(MountError::NotSupported(...))
}
```

### MountCapability 新增

```rust
pub enum MountCapability {
    Read, Write, List, Search, Exec, Watch,
    Snapshot,  // 新增
}
```

### 存储设计（inline_fs / canvas_fs）

快照存储方案有两种，需后续调研确定：

**方案 A：全量序列化**
- 快照时将 mount 下所有文件序列化为一个 JSON blob，存入 `mount_snapshots` 表
- 优点：实现简单，回滚就是反序列化覆写
- 缺点：文件多时快照体积大

**方案 B：Copy-on-Write 差异**
- 快照时只记录时间戳，后续写操作保留旧版本到 `snapshot_history` 表
- 回滚时从 history 恢复被修改的文件
- 优点：快照瞬间完成，存储高效
- 缺点：实现复杂，需要 hook 到每次 write/delete

初期建议采用方案 A（全量序列化），数据量可控时够用；未来如果 inline_fs 文件量增长再迁移到方案 B。

### Workflow 集成

- Step 执行引擎在调用 Agent 前，对目标 writable mount 自动 `create_snapshot`
- Step 失败或被用户 reject 时，提供 `restore_snapshot` 操作
- 快照自动过期策略：保留最近 N 个（如 10 个），超出自动删除最旧的

---

## 验收标准

- [ ] `MountProvider` trait 有 snapshot CRUD 四个可选方法
- [ ] `MountCapability` 枚举包含 `Snapshot`
- [ ] `inline_fs` provider 实现全量序列化快照
- [ ] Workflow step 执行前自动创建快照
- [ ] Step 失败时可通过 API 回滚到快照
- [ ] 快照自动过期清理

## 不在范围内

- 跨 mount 联合快照（一致性问题留待未来）
- relay_fs 的 git 快照集成（relay_fs 本身有 git，不需要额外机制）
- 快照 diff / 对比查看（可作为后续增强）
- Agent tool 层的快照操作命令（初期由编排层自动管理）

## 长期演进

- 方案 B（CoW 差异快照）在文件量增长后引入
- 快照 + session branching 结合：不同分支基于不同快照
- 快照 diff 可视化：UI 上对比两个快照的文件变更
- 跨 mount 事务性快照（如果出现多 mount 联合回滚的刚需）
