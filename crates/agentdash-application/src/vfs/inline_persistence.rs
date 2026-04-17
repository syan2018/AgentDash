use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::vfs::mount::parse_inline_mount_owner;
use crate::runtime::Mount;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_spi::mount::{MountEvent, MountEventKind, MountEventReceiver};
use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;

/// broadcast channel 容量。
/// 溢出时老订阅者会收到 Lagged；128 对 inline 写入频率足够。
const MOUNT_EVENT_CHANNEL_CAPACITY: usize = 128;

// ─── Inline Content Persistence ─────────────────────────────

/// 内联文件写入持久化接口。
/// 实现方负责将 inline_fs mount 的文件修改写回到独立的 inline_fs_files 表。
#[async_trait]
pub trait InlineContentPersister: Send + Sync {
    /// 将文件内容持久化到 inline_fs_files 表。
    async fn persist_write(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String>;

    async fn persist_delete(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), String>;
}

type InlineOverrideMap = HashMap<(String, String), Option<String>>;

/// Per-session 的内联文件写入覆盖层。
///
/// 设计目标：
/// - 同一 session 内 write 后立即可 read（write-through cache）
/// - 写入同时通过 `InlineContentPersister` 持久化到 DB
/// - 多个 Agent 工具共享同一个 overlay（`Arc<InlineContentOverlay>`）
/// - 写入/删除时通过 broadcast channel 推送 `MountEvent`，供编排引擎订阅
pub struct InlineContentOverlay {
    overrides: tokio::sync::RwLock<InlineOverrideMap>,
    persister: Arc<dyn InlineContentPersister>,
    event_tx: broadcast::Sender<MountEvent>,
}

impl InlineContentOverlay {
    pub fn new(persister: Arc<dyn InlineContentPersister>) -> Self {
        let (event_tx, _) = broadcast::channel(MOUNT_EVENT_CHANNEL_CAPACITY);
        Self {
            overrides: Default::default(),
            persister,
            event_tx,
        }
    }

    /// 订阅该 overlay 上的所有 inline mount 事件。
    pub fn subscribe_events(&self) -> MountEventReceiver {
        self.event_tx.subscribe()
    }

    pub async fn read_override(&self, mount_id: &str, path: &str) -> Option<Option<String>> {
        self.overrides
            .read()
            .await
            .get(&(mount_id.to_string(), path.to_string()))
            .cloned()
    }

    pub async fn has_override(&self, mount_id: &str, path: &str) -> bool {
        self.overrides
            .read()
            .await
            .contains_key(&(mount_id.to_string(), path.to_string()))
    }

    /// 返回指定 mount 下所有被覆盖的文件（用于 list 时合并新增文件）
    pub async fn apply_to_files(&self, mount_id: &str, files: &mut BTreeMap<String, String>) {
        let overrides = self.overrides.read().await;
        for ((mid, path), content) in overrides.iter() {
            if mid != mount_id {
                continue;
            }
            match content {
                Some(content) => {
                    files.insert(path.clone(), content.clone());
                }
                None => {
                    files.remove(path);
                }
            }
        }
    }

    pub async fn write(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let (owner_kind, owner_id, container_id) = parse_inline_mount_owner(mount)?;

        // 判断是否之前存在（决定事件 kind = Created / Modified）
        let existed_before = {
            let overrides = self.overrides.read().await;
            matches!(
                overrides.get(&(mount.id.clone(), path.to_string())),
                Some(Some(_))
            )
        };

        // 1. 写入本地覆盖缓存（立即可读）
        self.overrides.write().await.insert(
            (mount.id.clone(), path.to_string()),
            Some(content.to_string()),
        );

        // 2. 持久化到 DB
        self.persister
            .persist_write(owner_kind, owner_id, &container_id, path, content)
            .await?;

        // 3. 推送事件（订阅者缺失时 send 返回 Err，忽略即可）
        let kind = if existed_before {
            MountEventKind::Modified
        } else {
            MountEventKind::Created
        };
        let _ = self
            .event_tx
            .send(MountEvent::new(&mount.id, path, kind));

        Ok(())
    }

    pub async fn delete(
        &self,
        mount: &Mount,
        path: &str,
    ) -> Result<(), String> {
        let (owner_kind, owner_id, container_id) = parse_inline_mount_owner(mount)?;

        self.overrides
            .write()
            .await
            .insert((mount.id.clone(), path.to_string()), None);

        self.persister
            .persist_delete(owner_kind, owner_id, &container_id, path)
            .await?;

        let _ = self
            .event_tx
            .send(MountEvent::new(&mount.id, path, MountEventKind::Deleted));

        Ok(())
    }

    pub async fn sync_files(
        &self,
        mount: &Mount,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for (path, content) in after {
            if before.get(path) != Some(content) {
                self.write(mount, path, content).await?;
            }
        }

        for path in before.keys() {
            if !after.contains_key(path) {
                self.delete(mount, path).await?;
            }
        }

        Ok(())
    }
}

// ─── DB Inline Content Persister ────────────────────────────

/// 基于 InlineFileRepository 的 InlineContentPersister 实现。
///
/// 直接操作 inline_fs_files 表，不再加载整个 Project/Story 实体。
pub struct DbInlineContentPersister {
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl DbInlineContentPersister {
    pub fn new(inline_file_repo: Arc<dyn InlineFileRepository>) -> Self {
        Self { inline_file_repo }
    }
}

#[async_trait]
impl InlineContentPersister for DbInlineContentPersister {
    async fn persist_write(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let file = InlineFile::new(owner_kind, owner_id, container_id, path, content);
        self.inline_file_repo
            .upsert_file(&file)
            .await
            .map_err(|e| format!("inline 文件写入失败: {e}"))
    }

    async fn persist_delete(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), String> {
        self.inline_file_repo
            .delete_file(owner_kind, owner_id, container_id, path)
            .await
            .map_err(|e| format!("inline 文件删除失败: {e}"))
    }
}

// ─── Container Files Sync ──────────────────────────────────

/// 将 ContextContainerDefinition 中的 InlineFiles 初始文件同步到 inline_fs_files 表。
///
/// 用于 Project/Story 创建或更新时，确保 API 层传入的初始文件
/// 写入独立存储（provider 不再从实体 JSONB 读取）。
pub async fn sync_container_inline_files(
    inline_file_repo: &dyn InlineFileRepository,
    owner_kind: InlineFileOwnerKind,
    owner_id: Uuid,
    containers: &[agentdash_domain::context_container::ContextContainerDefinition],
) -> Result<(), String> {
    use agentdash_domain::context_container::ContextContainerProvider;

    for container in containers {
        if let ContextContainerProvider::InlineFiles { files } = &container.provider {
            for file in files {
                let path = crate::vfs::normalize_mount_relative_path(&file.path, false)
                    .map_err(|e| format!("容器 {} 文件路径无效: {e}", container.id))?;
                let inline_file =
                    InlineFile::new(owner_kind, owner_id, &container.id, path, &file.content);
                inline_file_repo
                    .upsert_file(&inline_file)
                    .await
                    .map_err(|e| format!("同步容器 {} 初始文件失败: {e}", container.id))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::inline_file::InlineFileOwnerKind;

    /// 内存中的 InlineContentPersister，用于测试
    struct MemoryInlinePersister;

    #[async_trait]
    impl InlineContentPersister for MemoryInlinePersister {
        async fn persist_write(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
            _path: &str,
            _content: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn persist_delete(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
            _path: &str,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn make_inline_mount(owner_kind: &str, owner_id: Uuid, container_id: &str) -> Mount {
        Mount {
            id: "brief".to_string(),
            provider: "inline_fs".to_string(),
            backend_id: String::new(),
            root_ref: format!("context://inline/{container_id}"),
            capabilities: vec![],
            default_write: true,
            display_name: "brief".to_string(),
            metadata: serde_json::json!({
                "container_id": container_id,
                "agentdash_context_owner_kind": owner_kind,
                "agentdash_context_owner_id": owner_id.to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn overlay_write_and_read() {
        let overlay = InlineContentOverlay::new(Arc::new(MemoryInlinePersister));
        let owner_id = Uuid::new_v4();
        let mount = make_inline_mount("project", owner_id, "brief");

        overlay
            .write(&mount, "test.md", "hello")
            .await
            .expect("write should succeed");

        let result = overlay.read_override("brief", "test.md").await;
        assert_eq!(result, Some(Some("hello".to_string())));
    }

    #[tokio::test]
    async fn overlay_delete() {
        let overlay = InlineContentOverlay::new(Arc::new(MemoryInlinePersister));
        let owner_id = Uuid::new_v4();
        let mount = make_inline_mount("project", owner_id, "brief");

        overlay
            .delete(&mount, "test.md")
            .await
            .expect("delete should succeed");

        let result = overlay.read_override("brief", "test.md").await;
        assert_eq!(result, Some(None));
    }

    #[tokio::test]
    async fn overlay_write_emits_created_then_modified_events() {
        let overlay = InlineContentOverlay::new(Arc::new(MemoryInlinePersister));
        let owner_id = Uuid::new_v4();
        let mount = make_inline_mount("project", owner_id, "brief");

        let mut rx = overlay.subscribe_events();

        overlay.write(&mount, "note.md", "v1").await.expect("write 1");
        let evt = rx.recv().await.expect("event 1");
        assert_eq!(evt.mount_id, "brief");
        assert_eq!(evt.path, "note.md");
        assert_eq!(evt.kind, MountEventKind::Created);

        overlay.write(&mount, "note.md", "v2").await.expect("write 2");
        let evt = rx.recv().await.expect("event 2");
        assert_eq!(evt.kind, MountEventKind::Modified);

        overlay.delete(&mount, "note.md").await.expect("delete");
        let evt = rx.recv().await.expect("event 3");
        assert_eq!(evt.kind, MountEventKind::Deleted);
    }

    #[test]
    fn mount_without_owner_metadata_is_rejected() {
        let mount = Mount {
            id: "brief".to_string(),
            provider: "inline_fs".to_string(),
            backend_id: String::new(),
            root_ref: "context://inline/brief".to_string(),
            capabilities: vec![],
            default_write: true,
            display_name: "brief".to_string(),
            metadata: serde_json::json!({ "container_id": "brief" }),
        };

        let err = parse_inline_mount_owner(&mount).expect_err("should require owner metadata");
        assert!(err.contains("agentdash_context_owner_kind"));
    }
}
