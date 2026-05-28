use crate::common::error::DomainError;

/// LLM Provider 密文编解码端口。
///
/// 领域层只关心“密文能否被转换为运行态 secret”，具体算法与主密钥来源由基础设施实现。
pub trait LlmSecretCodec: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<String, DomainError>;
    fn decrypt(&self, ciphertext: &str) -> Result<String, DomainError>;
}

pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= 8 {
        return "****".to_string();
    }
    let prefix = trimmed.chars().take(4).collect::<String>();
    let suffix = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}
