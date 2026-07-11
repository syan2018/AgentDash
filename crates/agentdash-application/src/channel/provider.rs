use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_domain::channel::{
    ChannelBinding, ChannelBindingStatus, ChannelOwner, ChannelRegistryDocument,
};
use agentdash_spi::channel_binding::{ChannelBindingError, ChannelBindingProvider};
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{ChannelBindingResolution, ChannelBindingResolver, ProviderEventKey};
use crate::ApplicationError;

pub struct ChannelBindingProviderRegistry {
    providers: BTreeMap<String, Arc<dyn ChannelBindingProvider>>,
}

impl ChannelBindingProviderRegistry {
    pub fn new(
        providers: impl IntoIterator<Item = Arc<dyn ChannelBindingProvider>>,
    ) -> Result<Self, ApplicationError> {
        let mut registry = Self {
            providers: BTreeMap::new(),
        };
        for provider in providers {
            registry.register(provider)?;
        }
        Ok(registry)
    }

    pub fn register(
        &mut self,
        provider: Arc<dyn ChannelBindingProvider>,
    ) -> Result<(), ApplicationError> {
        let provider_key = provider.provider_key();
        if provider_key.is_empty() || provider_key.trim() != provider_key {
            return Err(ApplicationError::InvalidConfig(
                "channel binding provider key must be non-empty and trimmed".to_string(),
            ));
        }
        if self.providers.contains_key(provider_key) {
            return Err(ApplicationError::Conflict(format!(
                "channel binding provider `{provider_key}` is registered more than once"
            )));
        }
        self.providers.insert(provider_key.to_string(), provider);
        Ok(())
    }

    pub fn require(
        &self,
        provider_key: &str,
    ) -> Result<Arc<dyn ChannelBindingProvider>, ApplicationError> {
        self.providers.get(provider_key).cloned().ok_or_else(|| {
            ApplicationError::Unavailable(
                ChannelBindingError::Unavailable {
                    provider: provider_key.to_string(),
                }
                .to_string(),
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelBindingIndexEntry {
    pub owner: ChannelOwner,
    pub channel_id: Uuid,
    pub binding: ChannelBinding,
}

#[async_trait]
pub trait ChannelBindingIndex: Send + Sync {
    async fn resolve(
        &self,
        key: &ProviderEventKey,
    ) -> Result<Option<ChannelBindingIndexEntry>, ApplicationError>;

    async fn replace_owner(
        &self,
        owner: &ChannelOwner,
        registry: &ChannelRegistryDocument,
    ) -> Result<(), ApplicationError>;

    async fn validate_owner_replacement(
        &self,
        owner: &ChannelOwner,
        registry: &ChannelRegistryDocument,
    ) -> Result<(), ApplicationError>;

    async fn remove_owner(&self, owner: &ChannelOwner) -> Result<(), ApplicationError>;

    async fn clear(&self) -> Result<(), ApplicationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ChannelBindingIndexKey {
    provider: String,
    external_workspace_ref: String,
    external_room_ref: Option<String>,
    external_thread_ref: Option<String>,
}

impl ChannelBindingIndexKey {
    fn from_event(key: &ProviderEventKey) -> Self {
        Self {
            provider: key.provider.clone(),
            external_workspace_ref: key.external_workspace_ref.clone(),
            external_room_ref: key.external_room_ref.clone(),
            external_thread_ref: key.external_thread_ref.clone(),
        }
    }

    fn from_binding(binding: &ChannelBinding) -> Self {
        Self {
            provider: binding.provider.clone(),
            external_workspace_ref: binding.external_workspace_ref.clone(),
            external_room_ref: binding.external_room_ref.clone(),
            external_thread_ref: binding.external_thread_ref.clone(),
        }
    }
}

#[derive(Default)]
pub struct InMemoryChannelBindingIndex {
    entries: RwLock<BTreeMap<ChannelBindingIndexKey, ChannelBindingIndexEntry>>,
}

#[async_trait]
impl ChannelBindingIndex for InMemoryChannelBindingIndex {
    async fn resolve(
        &self,
        key: &ProviderEventKey,
    ) -> Result<Option<ChannelBindingIndexEntry>, ApplicationError> {
        key.validate()?;
        Ok(self
            .entries
            .read()
            .await
            .get(&ChannelBindingIndexKey::from_event(key))
            .cloned())
    }

    async fn replace_owner(
        &self,
        owner: &ChannelOwner,
        registry: &ChannelRegistryDocument,
    ) -> Result<(), ApplicationError> {
        let mut entries = self.entries.write().await;
        let next = owner_replacement(&entries, owner, registry)?;
        *entries = next;
        Ok(())
    }

    async fn validate_owner_replacement(
        &self,
        owner: &ChannelOwner,
        registry: &ChannelRegistryDocument,
    ) -> Result<(), ApplicationError> {
        let entries = self.entries.read().await;
        owner_replacement(&entries, owner, registry).map(|_| ())
    }

    async fn remove_owner(&self, owner: &ChannelOwner) -> Result<(), ApplicationError> {
        self.entries
            .write()
            .await
            .retain(|_, entry| &entry.owner != owner);
        Ok(())
    }

    async fn clear(&self) -> Result<(), ApplicationError> {
        self.entries.write().await.clear();
        Ok(())
    }
}

fn owner_replacement(
    entries: &BTreeMap<ChannelBindingIndexKey, ChannelBindingIndexEntry>,
    owner: &ChannelOwner,
    registry: &ChannelRegistryDocument,
) -> Result<BTreeMap<ChannelBindingIndexKey, ChannelBindingIndexEntry>, ApplicationError> {
    registry.validate().map_err(ApplicationError::from)?;
    let mut next = entries.clone();
    next.retain(|_, entry| &entry.owner != owner);
    for record in &registry.channels {
        if &record.channel.owner != owner {
            return Err(ApplicationError::Conflict(format!(
                "channel {} owner {} does not match projection owner {}",
                record.channel.id,
                record.channel.owner.stable_key(),
                owner.stable_key()
            )));
        }
        for binding in &record.bindings {
            if binding.status != ChannelBindingStatus::Active {
                continue;
            }
            let entry = ChannelBindingIndexEntry {
                owner: owner.clone(),
                channel_id: record.channel.id,
                binding: binding.clone(),
            };
            let key = ChannelBindingIndexKey::from_binding(binding);
            if let Some(existing) = next.get(&key) {
                return Err(ApplicationError::Conflict(format!(
                    "external channel binding key is already owned by {}:{}",
                    existing.owner.stable_key(),
                    existing.channel_id
                )));
            }
            next.insert(key, entry);
        }
    }
    Ok(next)
}

pub struct IndexedChannelBindingResolver {
    index: Arc<dyn ChannelBindingIndex>,
}

impl IndexedChannelBindingResolver {
    pub fn new(index: Arc<dyn ChannelBindingIndex>) -> Self {
        Self { index }
    }
}

#[async_trait]
impl ChannelBindingResolver for IndexedChannelBindingResolver {
    async fn resolve_binding(
        &self,
        key: &ProviderEventKey,
    ) -> Result<ChannelBindingResolution, ApplicationError> {
        Ok(match self.index.resolve(key).await? {
            Some(entry) => ChannelBindingResolution::Resolved {
                owner: entry.owner,
                channel_id: entry.channel_id,
                binding: entry.binding,
            },
            None => ChannelBindingResolution::Unresolved,
        })
    }
}

pub(crate) fn map_provider_error(error: ChannelBindingError) -> ApplicationError {
    match error {
        ChannelBindingError::Unavailable { .. } => ApplicationError::Unavailable(error.to_string()),
        ChannelBindingError::Rejected(_) => ApplicationError::Conflict(error.to_string()),
        ChannelBindingError::Failed(_) => ApplicationError::Internal(error.to_string()),
    }
}
