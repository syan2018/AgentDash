use agentdash_domain::DomainError;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryInlineFileRepository {
    files: Mutex<Vec<InlineFile>>,
}

#[async_trait::async_trait]
impl InlineFileRepository for MemoryInlineFileRepository {
    async fn get_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<Option<InlineFile>, DomainError> {
        Ok(self
            .files
            .lock()
            .await
            .iter()
            .find(|file| {
                file.owner_kind == owner_kind
                    && file.owner_id == owner_id
                    && file.container_id == container_id
                    && file.path == path
            })
            .cloned())
    }

    async fn list_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<Vec<InlineFile>, DomainError> {
        Ok(self
            .files
            .lock()
            .await
            .iter()
            .filter(|file| {
                file.owner_kind == owner_kind
                    && file.owner_id == owner_id
                    && file.container_id == container_id
            })
            .cloned()
            .collect())
    }

    async fn list_files_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<Vec<InlineFile>, DomainError> {
        Ok(self
            .files
            .lock()
            .await
            .iter()
            .filter(|file| file.owner_kind == owner_kind && file.owner_id == owner_id)
            .cloned()
            .collect())
    }

    async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
        let mut files = self.files.lock().await;
        if let Some(existing) = files.iter_mut().find(|existing| {
            existing.owner_kind == file.owner_kind
                && existing.owner_id == file.owner_id
                && existing.container_id == file.container_id
                && existing.path == file.path
        }) {
            *existing = file.clone();
        } else {
            files.push(file.clone());
        }
        Ok(())
    }

    async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
        for file in files {
            self.upsert_file(file).await?;
        }
        Ok(())
    }

    async fn delete_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), DomainError> {
        self.files.lock().await.retain(|file| {
            !(file.owner_kind == owner_kind
                && file.owner_id == owner_id
                && file.container_id == container_id
                && file.path == path)
        });
        Ok(())
    }

    async fn delete_by_container(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<(), DomainError> {
        self.files.lock().await.retain(|file| {
            !(file.owner_kind == owner_kind
                && file.owner_id == owner_id
                && file.container_id == container_id)
        });
        Ok(())
    }

    async fn delete_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<(), DomainError> {
        self.files
            .lock()
            .await
            .retain(|file| !(file.owner_kind == owner_kind && file.owner_id == owner_id));
        Ok(())
    }

    async fn count_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<i64, DomainError> {
        Ok(self
            .list_files(owner_kind, owner_id, container_id)
            .await?
            .len() as i64)
    }
}

impl MemoryInlineFileRepository {
    pub fn new_with_files(files: Vec<InlineFile>) -> Self {
        Self {
            files: Mutex::new(files),
        }
    }

    pub async fn debug_list(&self) -> Vec<InlineFile> {
        self.files.lock().await.clone()
    }
}
