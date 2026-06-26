use crate::account_availability::{evaluate_snapshot, Availability};
use crate::account_status::{
    load_account_status_context, set_account_status_with_context, AccountStatusContext,
};
use codexmanager_core::storage::{now_ts, Storage, UsageSnapshotRecord};
use codexmanager_core::usage::parse_usage_snapshot;

const DEFAULT_USAGE_SNAPSHOTS_RETAIN_PER_ACCOUNT: usize = 1;
const USAGE_SNAPSHOTS_RETAIN_PER_ACCOUNT_ENV: &str =
    "CODEXMANAGER_USAGE_SNAPSHOTS_RETAIN_PER_ACCOUNT";

fn usage_status_updates_blocked(context: &AccountStatusContext) -> bool {
    context.status.trim().eq_ignore_ascii_case("disabled")
}

/// 函数 `usage_snapshots_retain_per_account`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn usage_snapshots_retain_per_account() -> usize {
    std::env::var(USAGE_SNAPSHOTS_RETAIN_PER_ACCOUNT_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_USAGE_SNAPSHOTS_RETAIN_PER_ACCOUNT)
}

/// 函数 `apply_status_from_snapshot`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn apply_status_from_snapshot(
    storage: &Storage,
    record: &UsageSnapshotRecord,
) -> Availability {
    let availability = evaluate_snapshot(record);
    let context = load_account_status_context(storage, &record.account_id);

    if usage_status_updates_blocked(&context) {
        return availability;
    }

    match availability {
        Availability::Available => {
            set_account_status_with_context(
                storage,
                &record.account_id,
                "active",
                "usage_ok",
                Some(&context),
            );
        }
        Availability::Unavailable("usage_exhausted_primary" | "usage_exhausted_secondary") => {
            set_account_status_with_context(
                storage,
                &record.account_id,
                "limited",
                "usage_limit_exhausted",
                Some(&context),
            );
        }
        Availability::Unavailable(_) => {}
    }
    availability
}

/// 函数 `store_usage_snapshot`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn store_usage_snapshot(
    storage: &Storage,
    account_id: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    // 解析并写入用量快照
    let parsed = parse_usage_snapshot(&value);
    let record = UsageSnapshotRecord {
        account_id: account_id.to_string(),
        used_percent: parsed.used_percent,
        window_minutes: parsed.window_minutes,
        resets_at: parsed.resets_at,
        secondary_used_percent: parsed.secondary_used_percent,
        secondary_window_minutes: parsed.secondary_window_minutes,
        secondary_resets_at: parsed.secondary_resets_at,
        credits_json: parsed.credits_json,
        captured_at: now_ts(),
    };
    storage
        .insert_usage_snapshot(&record)
        .map_err(|e| e.to_string())?;
    let retain = usage_snapshots_retain_per_account();
    if retain > 0 {
        let _ = storage.prune_usage_snapshots_for_account(account_id, retain);
    }
    let _ = apply_status_from_snapshot(storage, &record);
    Ok(())
}
