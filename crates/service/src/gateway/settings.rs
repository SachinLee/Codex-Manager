use crate::app_settings::{
    parse_bool_with_default, APP_SETTING_AGGREGATE_API_SESSION_AFFINITY_ENABLED_KEY,
};

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
        .get_app_setting(APP_SETTING_AGGREGATE_API_SESSION_AFFINITY_ENABLED_KEY)
        .ok()
        .flatten()
        .map(|value| parse_bool_with_default(&value, false))
        .unwrap_or(false)
}
