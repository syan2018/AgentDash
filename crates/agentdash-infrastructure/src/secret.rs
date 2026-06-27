use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::llm_provider::LlmSecretCodec;
use base64::Engine;
use std::io::Write;
use std::path::{Path, PathBuf};

const SECRET_ENV: &str = "AGENTDASH_SECRET_KEY";
const CIPHERTEXT_VERSION: &str = "v1";
const DEFAULT_KEY_RELATIVE_PATH: &[&str] = &[".agentdash", "secrets", "llm-provider-master-key"];

#[derive(Clone)]
pub struct LlmProviderSecretCipher {
    key: [u8; 32],
}

impl LlmProviderSecretCipher {
    pub fn from_env_or_create_default() -> Result<Self, DomainError> {
        if let Some(raw) = non_empty_env(SECRET_ENV) {
            let key = parse_secret_key(&raw).ok_or_else(|| {
                DomainError::InvalidConfig(format!(
                    "{SECRET_ENV} 必须是 32 字节原文或 32 字节 key 的 base64 表示"
                ))
            })?;
            return Ok(Self { key });
        }

        Self::from_or_create_key_file(default_master_key_path()?)
    }

    pub fn from_or_create_key_file(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref();
        if path.exists() {
            return read_key_file(path).map(|key| Self { key });
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                DomainError::InvalidConfig(format!(
                    "创建 LLM Provider 主密钥目录失败: {} ({error})",
                    parent.display()
                ))
            })?;
        }

        let key = generate_key();
        let encoded = base64::engine::general_purpose::STANDARD.encode(key);

        match create_key_file(path) {
            Ok(mut file) => {
                file.write_all(encoded.as_bytes()).map_err(|error| {
                    DomainError::InvalidConfig(format!(
                        "写入 LLM Provider 主密钥失败: {} ({error})",
                        path.display()
                    ))
                })?;
                Ok(Self { key })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                read_key_file(path).map(|key| Self { key })
            }
            Err(error) => Err(DomainError::InvalidConfig(format!(
                "创建 LLM Provider 主密钥文件失败: {} ({error})",
                path.display()
            ))),
        }
    }

    pub fn new_with_key(key: [u8; 32]) -> Self {
        Self { key }
    }

    fn cipher(&self) -> Result<Aes256Gcm, DomainError> {
        Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            DomainError::InvalidConfig(format!("LLM 密钥加密器初始化失败: {error}"))
        })
    }
}

impl LlmSecretCodec for LlmProviderSecretCipher {
    fn encrypt(&self, plaintext: &str) -> Result<String, DomainError> {
        let cipher = self.cipher()?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|error| DomainError::InvalidConfig(format!("LLM 密钥加密失败: {error}")))?;
        let nonce_bytes: [u8; 12] = nonce.into();
        let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce_bytes);
        let ciphertext_b64 = base64::engine::general_purpose::STANDARD.encode(ciphertext);
        Ok(format!("{CIPHERTEXT_VERSION}:{nonce_b64}:{ciphertext_b64}"))
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String, DomainError> {
        let cipher = self.cipher()?;
        let mut parts = ciphertext.splitn(3, ':');
        let version = parts.next().unwrap_or_default();
        let nonce_b64 = parts.next().unwrap_or_default();
        let ciphertext_b64 = parts.next().unwrap_or_default();
        if version != CIPHERTEXT_VERSION || nonce_b64.is_empty() || ciphertext_b64.is_empty() {
            return Err(DomainError::InvalidConfig(
                "LLM Provider 密文格式无效，请重新保存密钥".to_string(),
            ));
        }
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(nonce_b64)
            .map_err(|error| DomainError::InvalidConfig(format!("LLM 密文 nonce 非法: {error}")))?;
        let nonce_bytes: [u8; 12] = nonce_bytes.try_into().map_err(|_| {
            DomainError::InvalidConfig(String::from("LLM 密文 nonce 长度无效，请重新保存密钥"))
        })?;
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext_bytes = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|error| DomainError::InvalidConfig(format!("LLM 密文内容非法: {error}")))?;
        let plaintext = cipher
            .decrypt(&nonce, ciphertext_bytes.as_ref())
            .map_err(|error| DomainError::InvalidConfig(format!("LLM 密钥解密失败: {error}")))?;
        String::from_utf8(plaintext)
            .map_err(|error| DomainError::InvalidConfig(format!("LLM 密钥不是 UTF-8: {error}")))
    }
}

fn default_master_key_path() -> Result<PathBuf, DomainError> {
    let data_root = match non_empty_env("AGENTDASH_DATA_ROOT") {
        Some(value) => PathBuf::from(value),
        None => std::env::current_dir().map_err(|error| {
            DomainError::InvalidConfig(format!("无法定位 AgentDash 数据目录: {error}"))
        })?,
    };
    Ok(DEFAULT_KEY_RELATIVE_PATH
        .iter()
        .fold(data_root, |path, segment| path.join(segment)))
}

fn create_key_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options.open(path)
}

fn read_key_file(path: &Path) -> Result<[u8; 32], DomainError> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        DomainError::InvalidConfig(format!(
            "读取 LLM Provider 主密钥文件失败: {} ({error})",
            path.display()
        ))
    })?;
    parse_secret_key(&content).ok_or_else(|| {
        DomainError::InvalidConfig(format!(
            "LLM Provider 主密钥文件格式无效: {}",
            path.display()
        ))
    })
}

fn generate_key() -> [u8; 32] {
    let mut key = [0_u8; 32];
    let mut rng = OsRng;
    rng.fill_bytes(&mut key);
    key
}

fn parse_secret_key(raw: &str) -> Option<[u8; 32]> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed)
        && let Ok(key) = decoded.try_into()
    {
        return Some(key);
    }
    if let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(trimmed)
        && let Ok(key) = decoded.try_into()
    {
        return Some(key);
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 32 {
        let mut key = [0_u8; 32];
        key.copy_from_slice(bytes);
        return Some(key);
    }
    None
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn secret_cipher_roundtrips_plaintext() {
        let cipher = LlmProviderSecretCipher::new_with_key([7; 32]);
        let encrypted = cipher.encrypt("sk-test-secret").expect("encrypt");
        assert_ne!(encrypted, "sk-test-secret");
        assert_eq!(
            cipher.decrypt(&encrypted).expect("decrypt"),
            "sk-test-secret"
        );
    }

    #[test]
    fn secret_cipher_creates_and_reuses_key_file() {
        let dir = std::env::temp_dir().join(format!("agentdash-secret-test-{}", Uuid::new_v4()));
        let path = dir.join("llm-provider-master-key");

        let first =
            LlmProviderSecretCipher::from_or_create_key_file(&path).expect("create key file");
        let encrypted = first.encrypt("sk-test-secret").expect("encrypt");
        assert!(path.exists());

        let second =
            LlmProviderSecretCipher::from_or_create_key_file(&path).expect("reuse key file");
        assert_eq!(
            second.decrypt(&encrypted).expect("decrypt"),
            "sk-test-secret"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_env_key_takes_precedence() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var(SECRET_ENV, "12345678901234567890123456789012");
        }
        let cipher = LlmProviderSecretCipher::from_env_or_create_default().expect("env key");
        let encrypted = cipher.encrypt("sk-test-secret").expect("encrypt");
        assert_eq!(
            cipher.decrypt(&encrypted).expect("decrypt"),
            "sk-test-secret"
        );
        unsafe {
            std::env::remove_var(SECRET_ENV);
        }
    }
}
