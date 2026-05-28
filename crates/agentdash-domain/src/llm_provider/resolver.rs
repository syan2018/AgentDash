use super::{
    LlmCredentialMode, LlmCredentialSource, LlmProvider, LlmProviderCredentialRepository,
    LlmSecretCodec,
};
use crate::common::error::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLlmCredential {
    pub api_key: String,
    pub source: LlmCredentialSource,
}

pub async fn resolve_effective_credential(
    provider: &LlmProvider,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    user_id: Option<&str>,
) -> Result<Option<ResolvedLlmCredential>, DomainError> {
    match provider.credential_mode {
        LlmCredentialMode::GlobalOnly => resolve_global_credential(provider, secret_codec),
        LlmCredentialMode::GlobalOrUser => {
            if let Some(user_credential) =
                resolve_user_credential(provider, credential_repo, secret_codec, user_id).await?
            {
                return Ok(Some(user_credential));
            }
            resolve_global_credential(provider, secret_codec)
        }
        LlmCredentialMode::UserRequired => {
            resolve_user_credential(provider, credential_repo, secret_codec, user_id).await
        }
    }
}

pub fn resolve_global_credential(
    provider: &LlmProvider,
    secret_codec: &dyn LlmSecretCodec,
) -> Result<Option<ResolvedLlmCredential>, DomainError> {
    if !provider.global_api_key_ciphertext.trim().is_empty() {
        let api_key = secret_codec.decrypt(&provider.global_api_key_ciphertext)?;
        if !api_key.trim().is_empty() {
            return Ok(Some(ResolvedLlmCredential {
                api_key,
                source: LlmCredentialSource::GlobalDb,
            }));
        }
    }

    if let Some(api_key) = provider.resolve_env_api_key() {
        if !api_key.trim().is_empty() {
            return Ok(Some(ResolvedLlmCredential {
                api_key,
                source: LlmCredentialSource::GlobalEnv,
            }));
        }
    }

    Ok(None)
}

pub async fn resolve_user_credential(
    provider: &LlmProvider,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    user_id: Option<&str>,
) -> Result<Option<ResolvedLlmCredential>, DomainError> {
    let (Some(repo), Some(user_id)) = (credential_repo, user_id) else {
        return Ok(None);
    };
    let credential = repo
        .get_for_user_provider(user_id, provider.id)
        .await?
        .filter(|credential| !credential.api_key_ciphertext.trim().is_empty());
    let Some(credential) = credential else {
        return Ok(None);
    };
    let api_key = secret_codec.decrypt(&credential.api_key_ciphertext)?;
    if api_key.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(ResolvedLlmCredential {
        api_key,
        source: LlmCredentialSource::UserByok,
    }))
}

pub fn provider_allows_empty_api_key(provider: &LlmProvider) -> bool {
    provider.protocol == super::WireProtocol::OpenaiCompatible
        && provider.credential_mode != LlmCredentialMode::UserRequired
        && provider.global_api_key_ciphertext.trim().is_empty()
        && provider.env_api_key.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::{LlmProviderUserCredential, WireProtocol};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct IdentitySecretCodec;

    impl LlmSecretCodec for IdentitySecretCodec {
        fn encrypt(&self, plaintext: &str) -> Result<String, DomainError> {
            Ok(plaintext.to_string())
        }

        fn decrypt(&self, ciphertext: &str) -> Result<String, DomainError> {
            Ok(ciphertext.to_string())
        }
    }

    #[derive(Default)]
    struct MemoryCredentialRepository {
        credentials: Mutex<HashMap<(String, Uuid), LlmProviderUserCredential>>,
    }

    impl MemoryCredentialRepository {
        fn with_credential(user_id: &str, provider_id: Uuid, api_key_ciphertext: &str) -> Self {
            let credential =
                LlmProviderUserCredential::new(provider_id, user_id, api_key_ciphertext);
            let credentials = HashMap::from([((user_id.to_string(), provider_id), credential)]);
            Self {
                credentials: Mutex::new(credentials),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProviderCredentialRepository for MemoryCredentialRepository {
        async fn get_for_user_provider(
            &self,
            user_id: &str,
            provider_id: Uuid,
        ) -> Result<Option<LlmProviderUserCredential>, DomainError> {
            Ok(self
                .credentials
                .lock()
                .expect("credential map lock")
                .get(&(user_id.to_string(), provider_id))
                .cloned())
        }

        async fn list_for_user(
            &self,
            user_id: &str,
        ) -> Result<Vec<LlmProviderUserCredential>, DomainError> {
            Ok(self
                .credentials
                .lock()
                .expect("credential map lock")
                .iter()
                .filter(|((candidate_user_id, _), _)| candidate_user_id == user_id)
                .map(|(_, credential)| credential.clone())
                .collect())
        }

        async fn upsert_for_user_provider(
            &self,
            credential: &LlmProviderUserCredential,
        ) -> Result<(), DomainError> {
            self.credentials
                .lock()
                .expect("credential map lock")
                .insert(
                    (credential.user_id.clone(), credential.provider_id),
                    credential.clone(),
                );
            Ok(())
        }

        async fn delete_for_user_provider(
            &self,
            user_id: &str,
            provider_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(self
                .credentials
                .lock()
                .expect("credential map lock")
                .remove(&(user_id.to_string(), provider_id))
                .is_some())
        }
    }

    fn provider(mode: LlmCredentialMode) -> LlmProvider {
        let mut provider = LlmProvider::new("Test", "test", WireProtocol::Anthropic);
        provider.credential_mode = mode;
        provider.global_api_key_ciphertext = "global-key".to_string();
        provider
    }

    #[tokio::test]
    async fn global_or_user_prefers_user_key_over_global_key() {
        let provider = provider(LlmCredentialMode::GlobalOrUser);
        let repo = MemoryCredentialRepository::with_credential("user-1", provider.id, "user-key");
        let resolved = resolve_effective_credential(
            &provider,
            Some(&repo),
            &IdentitySecretCodec,
            Some("user-1"),
        )
        .await
        .expect("resolve credential")
        .expect("credential");

        assert_eq!(resolved.api_key, "user-key");
        assert_eq!(resolved.source, LlmCredentialSource::UserByok);
    }

    #[tokio::test]
    async fn global_or_user_falls_back_to_global_key_without_user_key() {
        let provider = provider(LlmCredentialMode::GlobalOrUser);
        let repo = MemoryCredentialRepository::default();
        let resolved = resolve_effective_credential(
            &provider,
            Some(&repo),
            &IdentitySecretCodec,
            Some("user-1"),
        )
        .await
        .expect("resolve credential")
        .expect("credential");

        assert_eq!(resolved.api_key, "global-key");
        assert_eq!(resolved.source, LlmCredentialSource::GlobalDb);
    }

    #[tokio::test]
    async fn user_required_ignores_global_key_without_user_key() {
        let provider = provider(LlmCredentialMode::UserRequired);
        let resolved =
            resolve_effective_credential(&provider, None, &IdentitySecretCodec, Some("user-1"))
                .await
                .expect("resolve credential");

        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn global_only_ignores_user_key() {
        let provider = provider(LlmCredentialMode::GlobalOnly);
        let repo = MemoryCredentialRepository::with_credential("user-1", provider.id, "user-key");
        let resolved = resolve_effective_credential(
            &provider,
            Some(&repo),
            &IdentitySecretCodec,
            Some("user-1"),
        )
        .await
        .expect("resolve credential")
        .expect("credential");

        assert_eq!(resolved.api_key, "global-key");
        assert_eq!(resolved.source, LlmCredentialSource::GlobalDb);
    }

    #[test]
    fn empty_api_key_endpoint_is_only_allowed_for_non_required_openai_compatible_provider() {
        let mut provider = LlmProvider::new("Local", "local", WireProtocol::OpenaiCompatible);
        provider.credential_mode = LlmCredentialMode::GlobalOrUser;
        assert!(provider_allows_empty_api_key(&provider));

        provider.credential_mode = LlmCredentialMode::UserRequired;
        assert!(!provider_allows_empty_api_key(&provider));

        provider.credential_mode = LlmCredentialMode::GlobalOrUser;
        provider.protocol = WireProtocol::Anthropic;
        assert!(!provider_allows_empty_api_key(&provider));
    }
}
