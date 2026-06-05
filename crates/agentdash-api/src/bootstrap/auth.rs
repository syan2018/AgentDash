use anyhow::Result;

use agentdash_integration_api::AuthMode;

pub(crate) fn resolve_configured_auth_mode() -> Result<AuthMode> {
    match std::env::var("AGENTDASH_AUTH_MODE") {
        Ok(raw) => raw
            .parse::<AuthMode>()
            .map_err(|err| anyhow::anyhow!("AGENTDASH_AUTH_MODE 配置无效: {err}")),
        Err(std::env::VarError::NotPresent) => Ok(AuthMode::Personal),
        Err(err) => Err(anyhow::anyhow!("读取 AGENTDASH_AUTH_MODE 失败: {err}")),
    }
}

pub(crate) fn validate_auth_provider_registered(
    auth_mode: AuthMode,
    has_auth_provider: bool,
) -> Result<AuthMode> {
    if !has_auth_provider {
        anyhow::bail!("认证模式 `{auth_mode}` 未注册 AuthProvider，无法启动服务");
    }

    tracing::info!(auth_mode = %auth_mode, "认证模式已加载");
    Ok(auth_mode)
}
