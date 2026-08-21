use std::time::{Duration, Instant};

/// 首次请求之外允许的同一上游容量重放次数（Aggregate API 与账号池共用）。
pub(in super::super) const MAX_UPSTREAM_CAPACITY_RETRIES: usize = 2;

/// 容量等待回退参数（无合法 `Retry-After` 时使用有界全抖动）。
pub(in super::super) const CAPACITY_RETRY_BACKOFF_BASE: Duration = Duration::from_millis(500);
pub(in super::super) const CAPACITY_RETRY_BACKOFF_CAP: Duration = Duration::from_secs(1);

/// 上游 `Retry-After` 可接受的最大等待（约 2 秒）。
pub(in super::super) const CAPACITY_RETRY_AFTER_MAX: Duration = Duration::from_millis(2000);

/// 函数 `parse_retry_after`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - header: 参数 header
///
/// # 返回
/// 返回函数执行结果
pub(in super::super) fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    let value = header.map(str::trim).filter(|value| !value.is_empty())?;
    let seconds: u64 = value.parse().ok()?;
    let wait = Duration::from_secs(seconds);
    (wait <= CAPACITY_RETRY_AFTER_MAX).then_some(wait)
}

/// 函数 `capacity_wait_delay`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - retry_after: 参数 retry_after
/// - attempt: 参数 attempt
///
/// # 返回
/// 返回函数执行结果
pub(in super::super) fn capacity_wait_delay(retry_after: Option<&str>, attempt: u32) -> Duration {
    parse_retry_after(retry_after).unwrap_or_else(|| {
        super::backoff::exponential_jitter_delay(
            CAPACITY_RETRY_BACKOFF_BASE,
            CAPACITY_RETRY_BACKOFF_CAP,
            attempt,
        )
    })
}

/// 函数 `sleep_capacity_wait`
///
/// 作者: gaohongshun
///
/// 时间: 2026-08-11
///
/// # 参数
/// - retry_after: 参数 retry_after
/// - attempt: 参数 attempt
/// - deadline: 参数 deadline
///
/// # 返回
/// 返回函数执行结果
pub(in super::super) fn sleep_capacity_wait(
    retry_after: Option<&str>,
    attempt: u32,
    deadline: Option<Instant>,
) -> bool {
    let delay = capacity_wait_delay(retry_after, attempt);
    let Some(delay) = super::deadline::cap_wait(delay, deadline) else {
        return false;
    };
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    !super::deadline::is_expired(deadline)
}

#[cfg(test)]
#[path = "../tests/support/capacity_tests.rs"]
mod tests;
