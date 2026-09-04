use crate::gateway::settings::aggregate_api_session_affinity_enabled;
use codexmanager_core::storage::{AggregateApi, Storage};
use sha2::{Digest, Sha256};

/// 函数 `should_apply_aggregate_api_affinity`
///
/// 作者: AI Assistant
///
/// 时间: 2026-09-03
///
/// # 参数
/// - storage: Storage instance
/// - explicit_aggregate_api_id: Explicitly specified Aggregate API ID
///
/// # 返回
/// true if affinity should be applied
pub(crate) fn should_apply_aggregate_api_affinity(
    storage: &Storage,
    explicit_aggregate_api_id: Option<&str>,
) -> bool {
    explicit_aggregate_api_id.is_none() && aggregate_api_session_affinity_enabled(storage)
}

/// 函数 `derive_affinity_route_hash`
///
/// 作者: AI Assistant
///
/// 时间: 2026-09-03
///
/// # 参数
/// - route_id: Cache affinity route ID string
///
/// # 返回
/// SHA256 hash of the route ID as hex string
pub(crate) fn derive_affinity_route_hash(route_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(route_id.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 函数 `reorder_candidates_with_affinity`
///
/// 作者: AI Assistant
///
/// 时间: 2026-09-03
///
/// # 参数
/// - storage: Storage instance
/// - platform_key_hash: Platform key hash for partition
/// - protocol_type: Protocol type (e.g., "openai_compat")
/// - model: Model identifier (None becomes empty string)
/// - cache_affinity_route_id_hash: Hashed cache affinity route ID
/// - candidates: List of Aggregate API candidates
///
/// # 返回
/// Reordered candidates with bound source at position 0 if found
pub(crate) fn reorder_candidates_with_affinity(
    storage: &Storage,
    platform_key_hash: &str,
    protocol_type: &str,
    model: Option<&str>,
    cache_affinity_route_id_hash: &str,
    mut candidates: Vec<AggregateApi>,
) -> Result<Vec<AggregateApi>, String> {
    let model = model.unwrap_or("");
    let binding = storage
        .get_aggregate_api_binding(
            platform_key_hash,
            protocol_type,
            model,
            cache_affinity_route_id_hash,
        )
        .map_err(|e| format!("affinity binding lookup failed: {e}"))?;

    if let Some(binding) = binding {
        if let Some(pos) = candidates
            .iter()
            .position(|c| c.id == binding.bound_aggregate_api_id)
        {
            let bound_candidate = candidates.remove(pos);
            candidates.insert(0, bound_candidate);
        }
    }
    Ok(candidates)
}

#[cfg(test)]
#[path = "tests/aggregate_api_affinity_tests.rs"]
mod aggregate_api_affinity_tests;
