use codexmanager_core::storage::{now_ts, GatewayErrorLog};

use crate::storage_helpers::open_storage;

const ENV_GATEWAY_ERROR_STDOUT: &str = "CODEXMANAGER_GATEWAY_ERROR_STDOUT";
const LOG_FIELD_LIMIT_CHARS: usize = 1200;

#[derive(Debug, Clone, Default)]
pub(crate) struct GatewayErrorLogInput<'a> {
    pub(crate) trace_id: Option<&'a str>,
    pub(crate) key_id: Option<&'a str>,
    pub(crate) account_id: Option<&'a str>,
    pub(crate) request_path: &'a str,
    pub(crate) method: &'a str,
    pub(crate) stage: &'a str,
    pub(crate) error_kind: Option<&'a str>,
    pub(crate) upstream_url: Option<&'a str>,
    pub(crate) cf_ray: Option<&'a str>,
    pub(crate) status_code: Option<u16>,
    pub(crate) compression_enabled: bool,
    pub(crate) compression_retry_attempted: bool,
    pub(crate) message: &'a str,
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn gateway_error_stdout_enabled() -> bool {
    env_flag_enabled(ENV_GATEWAY_ERROR_STDOUT)
}

fn sanitize_log_field(value: &str) -> String {
    let redacted = redact_bearer_tokens(value);
    let collapsed = redacted.replace(['\r', '\n'], " ");
    truncate_chars(collapsed.as_str(), LOG_FIELD_LIMIT_CHARS)
}

fn sanitize_url_field(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let sanitized = sanitize_log_field(value);
    if let Some((prefix, _query)) = sanitized.split_once('?') {
        return format!("{prefix}?<redacted>");
    }
    sanitized
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut output = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= limit {
            output.push_str("...");
            return output;
        }
        output.push(ch);
    }
    output
}

fn redact_bearer_tokens(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    loop {
        let Some(index) = rest.find("Bearer ") else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..index]);
        output.push_str("Bearer <redacted>");
        let token_start = index + "Bearer ".len();
        let token_tail = &rest[token_start..];
        let token_end = token_tail
            .find(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ')'))
            .unwrap_or(token_tail.len());
        rest = &token_tail[token_end..];
    }
    output
}

fn log_gateway_error_stdout(input: &GatewayErrorLogInput<'_>) {
    if !gateway_error_stdout_enabled() {
        return;
    }
    log::warn!(
        "event=gateway_error_log trace_id={} key_id={} account_id={} method={} path={} stage={} kind={} status={} upstream_url={} cf_ray={} compression={} compression_retry={} message={}",
        sanitize_log_field(input.trace_id.unwrap_or("-")),
        sanitize_log_field(input.key_id.unwrap_or("-")),
        sanitize_log_field(input.account_id.unwrap_or("-")),
        sanitize_log_field(input.method),
        sanitize_log_field(input.request_path),
        sanitize_log_field(input.stage),
        sanitize_log_field(input.error_kind.unwrap_or("-")),
        input.status_code.map(|status| status.to_string()).unwrap_or_else(|| "-".to_string()),
        sanitize_url_field(input.upstream_url),
        sanitize_log_field(input.cf_ray.unwrap_or("-")),
        if input.compression_enabled { "true" } else { "false" },
        if input.compression_retry_attempted { "true" } else { "false" },
        sanitize_log_field(input.message),
    );
}

/// 函数 `write_gateway_error_log`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-04
///
/// # 参数
/// - input: 参数 input
///
/// # 返回
/// 无
pub(crate) fn write_gateway_error_log(input: GatewayErrorLogInput<'_>) {
    log_gateway_error_stdout(&input);
    let Some(storage) = open_storage() else {
        return;
    };
    let log = GatewayErrorLog {
        trace_id: input.trace_id.map(str::to_string),
        key_id: input.key_id.map(str::to_string),
        account_id: input.account_id.map(str::to_string),
        request_path: input.request_path.to_string(),
        method: input.method.to_string(),
        stage: input.stage.to_string(),
        error_kind: input.error_kind.map(str::to_string),
        upstream_url: input.upstream_url.map(str::to_string),
        cf_ray: input.cf_ray.map(str::to_string),
        status_code: input.status_code.map(i64::from),
        compression_enabled: input.compression_enabled,
        compression_retry_attempted: input.compression_retry_attempted,
        message: input.message.to_string(),
        created_at: now_ts(),
    };
    if let Err(err) = storage.insert_gateway_error_log(&log) {
        log::warn!("insert gateway error log failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::{gateway_error_stdout_enabled, sanitize_log_field, sanitize_url_field};

    #[test]
    fn gateway_error_stdout_flag_defaults_off() {
        let _guard = crate::test_env_guard();
        std::env::remove_var("CODEXMANAGER_GATEWAY_ERROR_STDOUT");

        assert!(!gateway_error_stdout_enabled());
    }

    #[test]
    fn gateway_error_stdout_flag_accepts_true_values() {
        let _guard = crate::test_env_guard();
        std::env::set_var("CODEXMANAGER_GATEWAY_ERROR_STDOUT", "yes");

        assert!(gateway_error_stdout_enabled());
        std::env::remove_var("CODEXMANAGER_GATEWAY_ERROR_STDOUT");
    }

    #[test]
    fn gateway_error_stdout_sanitizes_tokens_and_urls() {
        let field = sanitize_log_field("failed Authorization: Bearer secret-token\nnext");
        assert!(field.contains("Bearer <redacted>"));
        assert!(!field.contains("secret-token"));
        assert!(!field.contains('\n'));

        assert_eq!(
            sanitize_url_field(Some("https://example.com/v1?api_key=secret")),
            "https://example.com/v1?<redacted>"
        );
    }
}
