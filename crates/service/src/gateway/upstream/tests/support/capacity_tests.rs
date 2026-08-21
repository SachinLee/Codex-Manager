use super::{capacity_wait_delay, parse_retry_after, sleep_capacity_wait};
use std::time::{Duration, Instant};

/// 函数 `retry_after_accepts_valid_integer_seconds_up_to_two_seconds`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn retry_after_accepts_valid_integer_seconds_up_to_two_seconds() {
    assert_eq!(parse_retry_after(Some("0")), Some(Duration::from_secs(0)));
    assert_eq!(parse_retry_after(Some("1")), Some(Duration::from_secs(1)));
    assert_eq!(parse_retry_after(Some("2")), Some(Duration::from_secs(2)));
    assert_eq!(parse_retry_after(Some(" 2 ")), Some(Duration::from_secs(2)));
}

/// 函数 `retry_after_rejects_missing_invalid_and_too_long_values`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn retry_after_rejects_missing_invalid_and_too_long_values() {
    assert_eq!(parse_retry_after(None), None);
    assert_eq!(parse_retry_after(Some("")), None);
    assert_eq!(parse_retry_after(Some("abc")), None);
    assert_eq!(parse_retry_after(Some("-1")), None);
    assert_eq!(parse_retry_after(Some("1.5")), None);
    assert_eq!(parse_retry_after(Some("3")), None);
    assert_eq!(parse_retry_after(Some("60")), None);
}

/// 函数 `capacity_wait_prefers_valid_retry_after`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn capacity_wait_prefers_valid_retry_after() {
    assert_eq!(capacity_wait_delay(Some("2"), 0), Duration::from_secs(2));
    assert_eq!(capacity_wait_delay(Some("1"), 7), Duration::from_secs(1));
}

/// 函数 `capacity_wait_falls_back_to_bounded_jitter_without_retry_after`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn capacity_wait_falls_back_to_bounded_jitter_without_retry_after() {
    let base_ms = super::CAPACITY_RETRY_BACKOFF_BASE.as_millis() as u64;
    let cap_ms = super::CAPACITY_RETRY_BACKOFF_CAP.as_millis() as u64;
    for attempt in [0u32, 1, 2, 3] {
        let delay = capacity_wait_delay(None, attempt);
        // 与 `exponential_jitter_delay` 的确定性上限对比，而不是与另一次
        // 随机抽样对比（后者每次都在 50% 概率失败）。
        let max_ms = (base_ms << attempt.min(10)).min(cap_ms).max(1);
        assert!(
            delay.as_millis() as u64 <= max_ms,
            "fallback jitter must stay within the deterministic ceiling for attempt {attempt}: {delay:?} > {max_ms}ms"
        );
    }
    assert!(capacity_wait_delay(Some("60"), 0) != Duration::from_secs(60));
    assert_eq!(
        capacity_wait_delay(Some("60"), 0) <= super::CAPACITY_RETRY_BACKOFF_CAP,
        true,
        "too-long Retry-After must fall back to bounded jitter"
    );
}

/// 函数 `sleep_capacity_wait_refuses_expired_deadline`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn sleep_capacity_wait_refuses_expired_deadline() {
    let expired = Instant::now().checked_sub(Duration::from_secs(5));
    assert!(!sleep_capacity_wait(Some("1"), 0, expired));
    assert!(!sleep_capacity_wait(None, 0, expired));
}

/// 函数 `sleep_capacity_wait_succeeds_without_deadline_or_with_headroom`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn sleep_capacity_wait_succeeds_without_deadline_or_with_headroom() {
    assert!(sleep_capacity_wait(Some("0"), 0, None));
    let headroom = Instant::now() + Duration::from_secs(60);
    assert!(sleep_capacity_wait(Some("0"), 0, Some(headroom)));
    assert!(sleep_capacity_wait(None, 0, Some(headroom)));
}

/// 函数 `max_retry_after_matches_contract`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - 无
///
/// # 返回
/// 无
#[test]
fn max_retry_after_matches_contract() {
    assert_eq!(super::CAPACITY_RETRY_AFTER_MAX, Duration::from_millis(2000));
    assert_eq!(super::MAX_UPSTREAM_CAPACITY_RETRIES, 2);
}
