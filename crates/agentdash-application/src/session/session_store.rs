use std::path::PathBuf;

use agentdash_protocol::BackboneEnvelope;

use super::types::SessionMeta;

#[derive(Clone)]
pub(super) struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn jsonl_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.jsonl"))
    }

    fn meta_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.meta.json"))
    }

    pub async fn write_meta(&self, meta: &SessionMeta) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.meta_path(&meta.id);
        let json = serde_json::to_string_pretty(meta).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("序列化 session meta 失败: {error}"),
            )
        })?;
        tokio::fs::write(path, json).await
    }

    pub async fn read_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        let path = self.meta_path(session_id);
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let meta = serde_json::from_str::<SessionMeta>(&content)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Some(meta))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        let mut entries = match tokio::fs::read_dir(&self.base_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut sessions = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.ends_with(".meta.json") {
                continue;
            }
            let content = tokio::fs::read_to_string(entry.path()).await?;
            let meta = serde_json::from_str::<SessionMeta>(&content).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("解析 session meta 失败: {error}"),
                )
            })?;
            sessions.push(meta);
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(sessions)
    }

    pub async fn delete(&self, session_id: &str) -> std::io::Result<()> {
        let jsonl = self.jsonl_path(session_id);
        let meta = self.meta_path(session_id);
        let _ = tokio::fs::remove_file(jsonl).await;
        let _ = tokio::fs::remove_file(meta).await;
        Ok(())
    }

    pub async fn append(&self, session_id: &str, envelope: &BackboneEnvelope) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.jsonl_path(session_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let line = serde_json::to_string(envelope).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("序列化 BackboneEnvelope 失败: {error}"),
            )
        })?;
        use tokio::io::AsyncWriteExt as _;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }

    pub async fn read_all(&self, session_id: &str) -> std::io::Result<Vec<BackboneEnvelope>> {
        let path = self.jsonl_path(session_id);
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut out = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let envelope = serde_json::from_str::<BackboneEnvelope>(t).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("解析 BackboneEnvelope 第 {} 行失败: {error}", index + 1),
                )
            })?;
            out.push(envelope);
        }
        Ok(out)
    }
}
