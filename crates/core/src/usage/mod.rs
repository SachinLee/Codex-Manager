use serde::{Deserialize, Serialize};
use serde_json::Value;

const EXTRA_RATE_LIMITS_JSON_KEY: &str = "_codexmanager_extra_rate_limits";
pub const RESET_CREDITS_JSON_KEY: &str = "rate_limit_reset_credits";

#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub used_percent: Option<f64>,
    pub window_minutes: Option<i64>,
    pub resets_at: Option<i64>,
    pub secondary_used_percent: Option<f64>,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_resets_at: Option<i64>,
    pub credits_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredit {
    pub id: Option<String>,
    pub status: Option<String>,
    pub reset_type: Option<String>,
    pub granted_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub redeemed_at: Option<i64>,
    pub raw_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResetCreditsSnapshot {
    pub available_count: Option<i64>,
    pub credits: Vec<ResetCredit>,
    pub next_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResetCreditConsumeResult {
    pub consumed: bool,
    pub usage_refreshed: bool,
    pub snapshot: Option<ResetCreditsSnapshot>,
    pub warning: Option<String>,
}

fn normalize_rate_limit_entry(source_key: Option<&str>, value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let rate_limit = obj
        .get("rate_limit")
        .and_then(Value::as_object)
        .unwrap_or(obj);
    let has_primary = rate_limit.get("primary_window").is_some();
    let has_secondary = rate_limit.get("secondary_window").is_some();
    if !has_primary && !has_secondary {
        return None;
    }

    let mut normalized = serde_json::Map::new();
    if let Some(source_key) = source_key.map(str::trim).filter(|value| !value.is_empty()) {
        normalized.insert(
            "source_key".to_string(),
            Value::String(source_key.to_string()),
        );
    }
    for key in ["limit_name", "metered_feature"] {
        if let Some(field) = obj.get(key) {
            normalized.insert(key.to_string(), field.clone());
        }
    }
    if let Some(field) = obj.get("limit_id").or_else(|| obj.get("metered_feature")) {
        normalized.insert("limit_id".to_string(), field.clone());
    }
    for key in ["allowed", "limit_reached"] {
        if let Some(field) = obj.get(key).or_else(|| rate_limit.get(key)) {
            normalized.insert(key.to_string(), field.clone());
        }
    }
    normalized.insert(
        "primary_window".to_string(),
        rate_limit
            .get("primary_window")
            .cloned()
            .unwrap_or(Value::Null),
    );
    normalized.insert(
        "secondary_window".to_string(),
        rate_limit
            .get("secondary_window")
            .cloned()
            .unwrap_or(Value::Null),
    );
    Some(Value::Object(normalized))
}

fn collect_extra_rate_limits(value: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(root) = value.as_object() else {
        return out;
    };

    for (key, nested) in root {
        if key == "rate_limit" || !key.ends_with("_rate_limit") {
            continue;
        }
        if let Some(item) = normalize_rate_limit_entry(Some(key.as_str()), nested) {
            out.push(item);
        }
    }

    match root.get("additional_rate_limits") {
        Some(Value::Array(items)) => {
            for (index, item) in items.iter().enumerate() {
                let source_key = item
                    .get("limit_id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("metered_feature").and_then(Value::as_str))
                    .or_else(|| item.get("limit_name").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("additional_rate_limits[{index}]"));
                if let Some(normalized) =
                    normalize_rate_limit_entry(Some(source_key.as_str()), item)
                {
                    out.push(normalized);
                }
            }
        }
        Some(Value::Object(items)) => {
            for (key, item) in items {
                if let Some(normalized) = normalize_rate_limit_entry(Some(key.as_str()), item) {
                    out.push(normalized);
                }
            }
        }
        _ => {}
    }

    out
}

fn serialize_credits_payload(
    credits: Option<&Value>,
    extra_rate_limits: &[Value],
    reset_credits: Option<&Value>,
) -> Option<String> {
    if extra_rate_limits.is_empty() && reset_credits.is_none_or(Value::is_null) {
        return credits.and_then(|value| (!value.is_null()).then(|| value.to_string()));
    }

    let mut payload = match credits {
        Some(Value::Object(obj)) => obj.clone(),
        Some(value) if !value.is_null() => {
            let mut wrapped = serde_json::Map::new();
            wrapped.insert("credits".to_string(), value.clone());
            wrapped
        }
        _ => serde_json::Map::new(),
    };
    payload.insert(
        EXTRA_RATE_LIMITS_JSON_KEY.to_string(),
        Value::Array(extra_rate_limits.to_vec()),
    );
    if let Some(reset_credits) = reset_credits.filter(|value| !value.is_null()) {
        payload.insert(RESET_CREDITS_JSON_KEY.to_string(), reset_credits.clone());
    }
    Some(Value::Object(payload).to_string())
}

/// 函数 `normalize_base_url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - base_url: 参数 base_url
///
/// # 返回
/// 返回函数执行结果
pub fn normalize_base_url(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/').to_string();
    let is_chatgpt_host =
        base.starts_with("https://chatgpt.com") || base.starts_with("https://chat.openai.com");
    if is_chatgpt_host && !base.contains("/backend-api") {
        base.push_str("/backend-api");
    }
    base
}

/// 函数 `usage_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - base_url: 参数 base_url
///
/// # 返回
/// 返回函数执行结果
pub fn usage_endpoint(base_url: &str) -> String {
    let base = normalize_base_url(base_url);
    if base.contains("/backend-api") {
        format!("{base}/wham/usage")
    } else {
        format!("{base}/api/codex/usage")
    }
}

pub fn reset_credits_endpoint(base_url: &str) -> String {
    let base = normalize_base_url(base_url);
    if base.contains("/backend-api") {
        format!("{base}/wham/rate-limit-reset-credits")
    } else {
        format!("{base}/api/codex/rate-limit-reset-credits")
    }
}

pub fn reset_credits_consume_endpoint(base_url: &str) -> String {
    format!("{}/consume", reset_credits_endpoint(base_url))
}

fn parse_reset_credit_timestamp(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => {
            let mut timestamp = number
                .as_i64()
                .or_else(|| number.as_u64().and_then(|raw| i64::try_from(raw).ok()))?;
            if timestamp > 1_000_000_000_000 {
                timestamp /= 1000;
            }
            Some(timestamp)
        }
        Value::String(text) => text.trim().parse::<i64>().ok().or_else(|| {
            chrono::DateTime::parse_from_rfc3339(text.trim())
                .ok()
                .map(|value| value.timestamp())
        }),
        _ => None,
    }
}

fn reset_credit_timestamp(record: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| parse_reset_credit_timestamp(record.get(*key)))
}

fn reset_credit_string(record: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        record.get(*key).and_then(|value| match value {
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    })
}

fn parse_reset_credit(value: &Value) -> Option<ResetCredit> {
    let record = value.as_object()?;
    let raw_status = reset_credit_string(record, &["status", "state"]);
    let expires_at = reset_credit_timestamp(record, &["expires_at", "expire_at", "expiresAt"]);
    let status = raw_status
        .as_deref()
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            expires_at
                .is_some_and(|timestamp| timestamp <= chrono::Utc::now().timestamp())
                .then(|| "expired".to_string())
        });

    Some(ResetCredit {
        id: reset_credit_string(record, &["id", "credit_id", "creditId"]),
        status,
        reset_type: reset_credit_string(record, &["type", "reset_type", "resetType"]),
        granted_at: reset_credit_timestamp(record, &["granted_at", "created_at", "grantedAt"]),
        expires_at,
        redeemed_at: reset_credit_timestamp(
            record,
            &["redeemed_at", "used_at", "consumed_at", "redeemedAt"],
        ),
        raw_status,
    })
}

fn reset_credit_is_available(credit: &ResetCredit, now: i64) -> bool {
    let status = credit
        .status
        .as_deref()
        .or(credit.raw_status.as_deref())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if status != "available" {
        return false;
    }
    credit
        .expires_at
        .map(|timestamp| timestamp > now)
        .unwrap_or(true)
}

pub fn parse_reset_credits_snapshot(value: &Value) -> ResetCreditsSnapshot {
    let credits = value
        .get("credits")
        .or_else(|| value.get("data").and_then(|data| data.get("credits")))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_reset_credit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    let available_count = value
        .get("available_count")
        .or_else(|| value.get("availableCount"))
        .or_else(|| {
            value.get("data").and_then(|data| {
                data.get("available_count")
                    .or_else(|| data.get("availableCount"))
            })
        })
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|raw| i64::try_from(raw).ok()))
        })
        .map(|count| count.max(0))
        .or_else(|| {
            Some(
                credits
                    .iter()
                    .filter(|credit| reset_credit_is_available(credit, now))
                    .count() as i64,
            )
        });
    let next_expires_at = credits
        .iter()
        .filter(|credit| reset_credit_is_available(credit, now))
        .filter_map(|credit| credit.expires_at)
        .min();

    ResetCreditsSnapshot {
        available_count,
        credits,
        next_expires_at,
    }
}

/// 函数 `subscription_endpoint`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-17
///
/// # 参数
/// - base_url: 参数 base_url
/// - account_id: 参数 account_id
///
/// # 返回
/// 返回函数执行结果
pub fn subscription_endpoint(base_url: &str, account_id: &str) -> String {
    let base = normalize_base_url(base_url);
    let trimmed_account_id = account_id.trim();
    let base_endpoint = format!("{base}/subscriptions");
    format!(
        "{base_endpoint}?account_id={}",
        urlencoding::encode(trimmed_account_id)
    )
}

pub fn accounts_check_endpoint(base_url: &str) -> String {
    let base = normalize_base_url(base_url);
    format!("{base}/accounts/check/v4-2023-04-27")
}

/// 函数 `parse_usage_snapshot`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
pub fn parse_usage_snapshot(value: &Value) -> UsageSnapshot {
    let used_percent = value
        .pointer("/rate_limit/primary_window/used_percent")
        .and_then(Value::as_f64);
    let window_minutes = value
        .pointer("/rate_limit/primary_window/limit_window_seconds")
        .and_then(Value::as_i64)
        .map(|s| (s + 59) / 60);
    let resets_at = value
        .pointer("/rate_limit/primary_window/reset_at")
        .and_then(Value::as_i64);
    let secondary_used_percent = value
        .pointer("/rate_limit/secondary_window/used_percent")
        .and_then(Value::as_f64);
    let secondary_window_minutes = value
        .pointer("/rate_limit/secondary_window/limit_window_seconds")
        .and_then(Value::as_i64)
        .map(|s| (s + 59) / 60);
    let secondary_resets_at = value
        .pointer("/rate_limit/secondary_window/reset_at")
        .and_then(Value::as_i64);
    let extra_rate_limits = collect_extra_rate_limits(value);
    let credits_json = serialize_credits_payload(
        value.get("credits"),
        &extra_rate_limits,
        value.get(RESET_CREDITS_JSON_KEY),
    );

    UsageSnapshot {
        used_percent,
        window_minutes,
        resets_at,
        secondary_used_percent,
        secondary_window_minutes,
        secondary_resets_at,
        credits_json,
    }
}
