from pathlib import Path

def replace_one(path, old, new, label):
    p = Path(path)
    text = p.read_text(encoding='utf-8')
    if old not in text:
        raise SystemExit(f'{label}: pattern not found in {path}')
    p.write_text(text.replace(old, new), encoding='utf-8')
    print(f'fixed {label}')

replace_one(
'apps/src/types/account.ts',
'''<<<<<<< HEAD
export interface AccountDailyUsageStat {
  accountId: string;
  requestCount: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens: number;
  billableInputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reasoningOutputTokens: number;
  estimatedCostUsd: number;
  cacheHitRate: number;
=======
export interface ResetCredit {
  id: string | null;
  status: string | null;
  resetType: string | null;
  grantedAt: number | null;
  expiresAt: number | null;
  redeemedAt: number | null;
  rawStatus: string | null;
}

export interface ResetCreditsSnapshot {
  availableCount: number | null;
  credits: ResetCredit[];
  nextExpiresAt: number | null;
}

export interface ResetCreditConsumeResult {
  consumed: boolean;
  usageRefreshed: boolean;
  snapshot: ResetCreditsSnapshot | null;
  warning: string | null;
>>>>>>> origin/main
}''',
'''export interface AccountDailyUsageStat {
  accountId: string;
  requestCount: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens: number;
  billableInputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reasoningOutputTokens: number;
  estimatedCostUsd: number;
  cacheHitRate: number;
}

export interface ResetCredit {
  id: string | null;
  status: string | null;
  resetType: string | null;
  grantedAt: number | null;
  expiresAt: number | null;
  redeemedAt: number | null;
  rawStatus: string | null;
}

export interface ResetCreditsSnapshot {
  availableCount: number | null;
  credits: ResetCredit[];
  nextExpiresAt: number | null;
}

export interface ResetCreditConsumeResult {
  consumed: boolean;
  usageRefreshed: boolean;
  snapshot: ResetCreditsSnapshot | null;
  warning: string | null;
}''',
'account.ts')

replace_one(
'apps/src/lib/api/account-client.ts',
'''import {
<<<<<<< HEAD
  normalizeAggregateApiCapabilities,
  normalizeAggregateApiCapabilityAttempts,
} from "./aggregate-capabilities";
=======
  normalizeAccountProxyUrlTestListResult,
  normalizeProxyDiagnosticTestListResult,
  normalizeProxySpeedTestListResult,
  normalizeProxyTestJobState,
} from "./proxy-normalize";
import {
  type AccountProxySource,
  readAccountProxySettings,
  type AccountProxySettings,
  type AccountProxySetPayload,
  type AccountProxyTestPayload,
} from "./account-proxy-settings";
export type {
  AccountProxySettings,
  AccountProxySetPayload,
  AccountProxySource,
  AccountProxyTestPayload,
};
>>>>>>> origin/main''',
'''import {
  normalizeAggregateApiCapabilities,
  normalizeAggregateApiCapabilityAttempts,
} from "./aggregate-capabilities";
import {
  normalizeAccountProxyUrlTestListResult,
  normalizeProxyDiagnosticTestListResult,
  normalizeProxySpeedTestListResult,
  normalizeProxyTestJobState,
} from "./proxy-normalize";
import {
  type AccountProxySource,
  readAccountProxySettings,
  type AccountProxySettings,
  type AccountProxySetPayload,
  type AccountProxyTestPayload,
} from "./account-proxy-settings";
export type {
  AccountProxySettings,
  AccountProxySetPayload,
  AccountProxySource,
  AccountProxyTestPayload,
};''',
'account-client.ts')

replace_one(
'apps/src/components/layout/sidebar.tsx',
'''<<<<<<< HEAD
  Rocket,
=======
  Globe,
>>>>>>> origin/main''',
'''  Rocket,
  Globe,''',
'sidebar.tsx')

replace_one(
'apps/tests/top-level-routes.test.mjs',
'''<<<<<<< HEAD
      ["/settings", "/plugins", "/codex-launcher"],
=======
      ["/settings", "/proxy-settings", "/plugins", "/author"],
>>>>>>> origin/main''',
'''      ["/settings", "/proxy-settings", "/plugins", "/codex-launcher", "/author"],''',
'top-level-routes')

replace_one(
'crates/service/src/lib.rs',
'''<<<<<<< HEAD
pub mod codex_session;
=======
mod codex_runtime;
>>>>>>> origin/main''',
'''pub mod codex_session;
mod codex_runtime;''',
'lib.rs modules')

replace_one(
'crates/service/src/lib.rs',
'''<<<<<<< HEAD
pub(crate) use requestlog::account_daily_usage as requestlog_account_daily_usage;
pub(crate) use requestlog::aggregate_api_daily_usage as requestlog_aggregate_api_daily_usage;
pub(crate) use requestlog::aggregate_api_reasoning_guard as requestlog_aggregate_api_reasoning_guard;
=======
pub(crate) use proxy_registry::{
    cancel_proxy_test_job, create_proxy_profile, delete_proxy_profile,
    get_proxy_profile_diagnostics_history, get_proxy_profile_latency_test_history,
    get_proxy_profile_speed_test_history, get_proxy_test_job, list_proxy_profiles,
    test_proxy_profile, test_proxy_profile_cloudflare_style_speed, test_proxy_profile_latency,
    test_proxy_profile_speed, update_proxy_profile,
};

>>>>>>> origin/main''',
'''pub(crate) use proxy_registry::{
    cancel_proxy_test_job, create_proxy_profile, delete_proxy_profile,
    get_proxy_profile_diagnostics_history, get_proxy_profile_latency_test_history,
    get_proxy_profile_speed_test_history, get_proxy_test_job, list_proxy_profiles,
    test_proxy_profile, test_proxy_profile_cloudflare_style_speed, test_proxy_profile_latency,
    test_proxy_profile_speed, update_proxy_profile,
};

pub(crate) use requestlog::account_daily_usage as requestlog_account_daily_usage;
pub(crate) use requestlog::aggregate_api_daily_usage as requestlog_aggregate_api_daily_usage;
pub(crate) use requestlog::aggregate_api_reasoning_guard as requestlog_aggregate_api_reasoning_guard;
''',
'lib.rs reexports')

replace_one(
'crates/service/src/gateway/mod.rs',
'''<<<<<<< HEAD
    request_gate_wait_timeout, trace_body_preview_max_bytes, upstream_client_for_account,
    upstream_stream_timeout, upstream_total_timeout, DEFAULT_GATEWAY_DEBUG,
};
pub(crate) use runtime_config::{
    prepare_upstream_client_for_aggregate_api_candidate,
    upstream_client_for_aggregate_api_candidate,
=======
    upstream_client_for_account,
};
use runtime_config::{
    prepare_upstream_client_for_aggregate_api_candidate, request_gate_wait_timeout,
    trace_body_preview_max_bytes, upstream_client_for_aggregate_api_candidate,
    upstream_stream_timeout, upstream_total_timeout, DEFAULT_GATEWAY_DEBUG,
>>>>>>> origin/main
};''',
'''    request_gate_wait_timeout, trace_body_preview_max_bytes, upstream_client_for_account,
    upstream_stream_timeout, upstream_total_timeout, DEFAULT_GATEWAY_DEBUG,
};
pub(crate) use runtime_config::{
    prepare_upstream_client_for_aggregate_api_candidate,
    upstream_client_for_aggregate_api_candidate,
};''',
'gateway/mod.rs')

replace_one(
'crates/service/src/dashboard.rs',
'''<<<<<<< HEAD
const TOKEN_ACTIVITY_MAX_RANGE_DAYS: i64 = 365;

pub(crate) fn read_token_activity(
    actor: &RpcActor,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<codexmanager_core::rpc::types::DashboardTokenActivityResult, String> {
    if !actor.is_admin() {
        return Err("permission_denied: token activity requires admin session".to_string());
    }
    crate::initialize_storage_if_needed()?;
    let storage =
        storage_helpers::open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (today_start, today_end) = time_bounds::local_day_bounds_ts()?;
    let requested_range_start = start_ts.filter(|value| *value > 0).unwrap_or_else(|| {
        today_start.saturating_sub((ADMIN_USAGE_RANGE_DAYS - 1) * time_bounds::DAY_SECONDS)
    });
    let range_end = end_ts
        .filter(|value| *value > requested_range_start)
        .unwrap_or(today_end);
    let range_start = requested_range_start
        .max(range_end.saturating_sub(TOKEN_ACTIVITY_MAX_RANGE_DAYS * time_bounds::DAY_SECONDS));
    let items = storage
        .summarize_request_token_stats_daily(range_start, range_end, time_bounds::DAY_SECONDS)
        .map_err(|err| format!("summarize token activity failed: {err}"))?;
    let days = fill_daily_usage(range_start, range_end, time_bounds::DAY_SECONDS, items);
    let total_tokens = days.iter().map(|item| item.usage.total_tokens.max(0)).sum();
    Ok(
        codexmanager_core::rpc::types::DashboardTokenActivityResult {
            range_start_ts: range_start,
            range_end_ts: range_end,
            total_tokens,
            days,
        },
    )
}
=======
const ADMIN_MODEL_SERIES_LIMIT: usize = 8;
const ADMIN_HOURLY_SERIES_MAX_DAYS: i64 = 31;
const HOUR_SECONDS: i64 = 3_600;
>>>>>>> origin/main''',
'''const TOKEN_ACTIVITY_MAX_RANGE_DAYS: i64 = 365;
const ADMIN_MODEL_SERIES_LIMIT: usize = 8;
const ADMIN_HOURLY_SERIES_MAX_DAYS: i64 = 31;
const HOUR_SECONDS: i64 = 3_600;

pub(crate) fn read_token_activity(
    actor: &RpcActor,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<codexmanager_core::rpc::types::DashboardTokenActivityResult, String> {
    if !actor.is_admin() {
        return Err("permission_denied: token activity requires admin session".to_string());
    }
    crate::initialize_storage_if_needed()?;
    let storage =
        storage_helpers::open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (today_start, today_end) = time_bounds::local_day_bounds_ts()?;
    let requested_range_start = start_ts.filter(|value| *value > 0).unwrap_or_else(|| {
        today_start.saturating_sub((ADMIN_USAGE_RANGE_DAYS - 1) * time_bounds::DAY_SECONDS)
    });
    let range_end = end_ts
        .filter(|value| *value > requested_range_start)
        .unwrap_or(today_end);
    let range_start = requested_range_start
        .max(range_end.saturating_sub(TOKEN_ACTIVITY_MAX_RANGE_DAYS * time_bounds::DAY_SECONDS));
    let items = storage
        .summarize_request_token_stats_daily(range_start, range_end, time_bounds::DAY_SECONDS)
        .map_err(|err| format!("summarize token activity failed: {err}"))?;
    let days = fill_daily_usage(range_start, range_end, time_bounds::DAY_SECONDS, items);
    let total_tokens = days.iter().map(|item| item.usage.total_tokens.max(0)).sum();
    Ok(
        codexmanager_core::rpc::types::DashboardTokenActivityResult {
            range_start_ts: range_start,
            range_end_ts: range_end,
            total_tokens,
            days,
        },
    )
}''',
'dashboard.rs')
print('done simple')
