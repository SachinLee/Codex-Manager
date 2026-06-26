use codexmanager_core::storage::{now_ts, Storage, Token, UsageSnapshotRecord};

use super::{should_retry_models_with_openai_fallback, sort_model_picker_candidates, Account};

/// 函数 `encode_base64url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - bytes: 参数 bytes
///
/// # 返回
/// 返回函数执行结果
fn encode_base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | (bytes[index + 2] as u32);
        out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(chunk & 0x3f) as usize] as char);
        index += 3;
    }
    match bytes.len().saturating_sub(index) {
        1 => {
            let chunk = (bytes[index] as u32) << 16;
            out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        }
        2 => {
            let chunk = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        }
        _ => {}
    }
    out
}

/// 函数 `plan_token`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - plan: 参数 plan
///
/// # 返回
/// 返回函数执行结果
fn plan_token(plan: &str) -> String {
    let header = encode_base64url(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = encode_base64url(
        serde_json::json!({
            "sub": format!("acc-{plan}"),
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": plan
            }
        })
        .to_string()
        .as_bytes(),
    );
    format!("{header}.{payload}.sig")
}

/// 函数 `candidate`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - id: 参数 id
/// - sort: 参数 sort
/// - plan: 参数 plan
///
/// # 返回
/// 返回函数执行结果
fn candidate(id: &str, sort: i64, plan: &str) -> (Account, Token) {
    let now = now_ts();
    let token = plan_token(plan);
    (
        Account {
            id: id.to_string(),
            label: id.to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        },
        Token {
            account_id: id.to_string(),
            id_token: token.clone(),
            access_token: token,
            refresh_token: "refresh".to_string(),
            api_key_access_token: None,
            last_refresh: now,
        },
    )
}

fn candidate_without_plan(id: &str, sort: i64) -> (Account, Token) {
    let now = now_ts();
    (
        Account {
            id: id.to_string(),
            label: id.to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        },
        Token {
            account_id: id.to_string(),
            id_token: "header.payload.sig".to_string(),
            access_token: "header.payload.sig".to_string(),
            refresh_token: "refresh".to_string(),
            api_key_access_token: None,
            last_refresh: now,
        },
    )
}

fn insert_usage_snapshot_with_plan(
    storage: &Storage,
    account_id: &str,
    captured_at: i64,
    plan: &str,
) {
    storage
        .insert_usage_snapshot(&UsageSnapshotRecord {
            account_id: account_id.to_string(),
            used_percent: Some(10.0),
            window_minutes: Some(300),
            resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            credits_json: Some(serde_json::json!({ "planType": plan }).to_string()),
            captured_at,
        })
        .expect("insert usage snapshot");
}

/// 函数 `fallback_retry_matches_stable_html_and_challenge_summaries`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn fallback_retry_matches_stable_html_and_challenge_summaries() {
    assert!(should_retry_models_with_openai_fallback(
        "models upstream failed: status=403 body=Cloudflare 安全验证页（title=Just a moment...）"
    ));
    assert!(should_retry_models_with_openai_fallback(
        "models upstream failed: status=502 body=<html><head><title>502 Bad Gateway</title></head></html>"
    ));
    assert!(!should_retry_models_with_openai_fallback(
        "models upstream failed: status=401 body=missing_authorization_header"
    ));
}

/// 函数 `sort_model_picker_candidates_prefers_plan_tier_priority`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn sort_model_picker_candidates_prefers_plan_tier_priority() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut candidates = vec![
        candidate("acc-free", 0, "free"),
        candidate("acc-team-a", 1, "team"),
        candidate("acc-plus", 2, "plus"),
        candidate("acc-pro", 3, "pro"),
        candidate("acc-go", 4, "go"),
        candidate("acc-team-b", 5, "business"),
    ];

    sort_model_picker_candidates(&storage, &mut candidates);

    let ids = candidates
        .iter()
        .map(|(account, _)| account.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "acc-pro",
            "acc-team-a",
            "acc-team-b",
            "acc-plus",
            "acc-go",
            "acc-free",
        ]
    );
}

#[test]
fn sort_model_picker_candidates_uses_latest_usage_snapshots_when_token_has_no_plan() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = now_ts();
    insert_usage_snapshot_with_plan(&storage, "acc-free-snapshot", now, "free");
    insert_usage_snapshot_with_plan(&storage, "acc-pro-snapshot", now, "free");
    insert_usage_snapshot_with_plan(&storage, "acc-pro-snapshot", now + 1, "pro");
    insert_usage_snapshot_with_plan(&storage, "acc-plus-snapshot", now, "plus");
    let mut candidates = vec![
        candidate_without_plan("acc-free-snapshot", 0),
        candidate_without_plan("acc-pro-snapshot", 1),
        candidate_without_plan("acc-plus-snapshot", 2),
    ];

    sort_model_picker_candidates(&storage, &mut candidates);

    let ids = candidates
        .iter()
        .map(|(account, _)| account.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["acc-pro-snapshot", "acc-plus-snapshot", "acc-free-snapshot"]
    );
}
