/// 函数 `aggregate_api_session_affinity_enabled`
///
/// 作者: AI Assistant
///
/// 时间: 2026-09-03
///
/// # 参数
/// - storage: Storage instance
///
/// # 返回
/// true if aggregate API session affinity is enabled
pub(crate) fn aggregate_api_session_affinity_enabled(
    storage: &codexmanager_core::storage::Storage,
) -> bool {
    storage
        .get_app_setting("aggregate_api_session_affinity_enabled")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i32>().ok())
        .map(|v| v != 0)
        .unwrap_or(false)
}
