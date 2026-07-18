//! Default [`FunctionRunner`] implementation.
//!
//! Owns the workflow function-activity IO: `tera` template rendering,
//! `reqwest` HTTP requests, and `tokio::process` command execution. Returns
//! raw outcomes; the application layer decides success / failure and builds
//! the corresponding activity event.

use agentdash_domain::workflow::{ApiRequestExecutorSpec, BashExecExecutorSpec};
use agentdash_process::{
    ProcessDomain, background_tokio_command, background_tokio_command_with_cwd,
};
use agentdash_platform_spi::{ApiRequestOutcome, BashExecOutcome, FunctionRunner};
use async_trait::async_trait;
use serde_json::Value;

/// Function runner backed by `reqwest` + `tokio::process` + `tera`.
#[derive(Debug, Default, Clone)]
pub struct DefaultFunctionRunner;

impl DefaultFunctionRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl FunctionRunner for DefaultFunctionRunner {
    async fn run_api_request(
        &self,
        spec: &ApiRequestExecutorSpec,
        context: &Value,
    ) -> Result<ApiRequestOutcome, String> {
        let method_text = render_template(&spec.method, context)?;
        let method = reqwest::Method::from_bytes(method_text.as_bytes())
            .map_err(|error| format!("API method 非法: {error}"))?;
        let url = render_template(&spec.url_template, context)?;

        let client = reqwest::Client::new();
        let mut request = client.request(method, url);
        if let Some(body_template) = &spec.body_template {
            let body = render_json_templates(body_template, context)?;
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("API request 失败: {error}"))?;
        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|error| format!("读取 API response 失败: {error}"))?;
        let body_json = serde_json::from_str::<Value>(&body_text).ok();

        Ok(ApiRequestOutcome {
            status: status.as_u16(),
            body_text,
            body_json,
        })
    }

    async fn run_bash(
        &self,
        spec: &BashExecExecutorSpec,
        context: &Value,
    ) -> Result<BashExecOutcome, String> {
        let command = render_template(&spec.command, context)?;
        let args = spec
            .args
            .iter()
            .map(|arg| render_template(arg, context))
            .collect::<Result<Vec<_>, _>>()?;

        let working_directory = spec
            .working_directory
            .as_ref()
            .map(|template| render_template(template, context))
            .transpose()?;
        let mut command_builder = match working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(cwd) => {
                background_tokio_command_with_cwd(ProcessDomain::FunctionRunner, &command, cwd)
            }
            None => background_tokio_command(ProcessDomain::FunctionRunner, &command),
        };
        command_builder.args(args);

        let output = command_builder
            .output()
            .await
            .map_err(|error| format!("Bash exec 启动失败: {error}"))?;
        Ok(BashExecOutcome {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        })
    }
}

fn render_template(template: &str, context: &Value) -> Result<String, String> {
    let context = tera::Context::from_serialize(context)
        .map_err(|error| format!("Function template context 非法: {error}"))?;
    tera::Tera::one_off(template, &context, false)
        .map_err(|error| format!("Function template 渲染失败: {error}"))
}

fn render_json_templates(value: &Value, context: &Value) -> Result<Value, String> {
    match value {
        Value::String(template) => render_template(template, context).map(Value::String),
        Value::Array(values) => values
            .iter()
            .map(|value| render_json_templates(value, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_json_templates(value, context)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}
