use codexmanager_core::storage::{now_ts, GatewayCapabilityOverrideRecord, GatewayCapabilityScope};
use serde_json::{json, Value};

fn open_storage() -> Result<crate::storage_helpers::StorageHandle, String> {
    crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())
}

fn validate_scope(
    api_id: &str,
    upstream_model_pattern: Option<&str>,
    protocol: Option<&str>,
    capability_key: Option<&str>,
) -> Result<GatewayCapabilityScope, String> {
    let api_id = api_id.trim();
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let model = upstream_model_pattern.unwrap_or("*").trim();
    let protocol = protocol.unwrap_or("responses").trim();
    let capability_key = capability_key
        .unwrap_or(crate::gateway::IMAGE_GENERATION_CAPABILITY)
        .trim();
    if model.is_empty() || protocol.is_empty() {
        return Err("capability scope model and protocol are required".to_string());
    }
    if capability_key != crate::gateway::IMAGE_GENERATION_CAPABILITY {
        return Err("unsupported capability key".to_string());
    }
    Ok(GatewayCapabilityScope {
        source_kind: "aggregate_api".to_string(),
        source_id: api_id.to_string(),
        upstream_model_pattern: model.to_string(),
        protocol: protocol.to_string(),
        capability_key: capability_key.to_string(),
    })
}

pub(crate) fn get_aggregate_api_capabilities(api_id: &str) -> Result<Value, String> {
    let storage = open_storage()?;
    let api = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| format!("read aggregate api failed: {err}"))?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    let now = now_ts();
    let overrides = storage
        .list_gateway_capability_overrides("aggregate_api", api_id)
        .map_err(|err| format!("list capability overrides failed: {err}"))?;
    let observations = storage
        .list_gateway_capability_observations("aggregate_api", api_id, now)
        .map_err(|err| format!("list capability observations failed: {err}"))?;
    let model = api
        .model_override
        .as_deref()
        .or_else(|| {
            observations
                .iter()
                .find(|item| {
                    item.scope.capability_key == crate::gateway::IMAGE_GENERATION_CAPABILITY
                })
                .map(|item| item.scope.upstream_model_pattern.as_str())
        })
        .unwrap_or("*");
    let effective = crate::gateway::resolve_capability(
        &overrides,
        &observations,
        model,
        "responses",
        crate::gateway::IMAGE_GENERATION_CAPABILITY,
        now,
    );
    let resolved_scope = effective.scope.as_ref();
    let matching_override = resolved_scope.and_then(|scope| {
        overrides.iter().find(|item| {
            item.scope.source_kind == scope.source_kind
                && item.scope.source_id == scope.source_id
                && item.scope.upstream_model_pattern == scope.upstream_model_pattern
                && item.scope.protocol == scope.protocol
                && item.scope.capability_key == scope.capability_key
        })
    });
    Ok(json!({
        "apiId": api.id,
        "routingMode": crate::gateway::current_capability_routing_mode().as_str(),
        "routingModeOptions": ["off", "observe", "enforce"],
        "items": [{
            "capabilityKey": crate::gateway::IMAGE_GENERATION_CAPABILITY,
            "effectiveState": effective.state.as_str(),
            "resolvedSource": effective.source,
            "confidence": effective.confidence,
            "expiresAt": effective.expires_at,
            "scope": {
                "sourceKind": resolved_scope.map(|scope| scope.source_kind.as_str()).unwrap_or("aggregate_api"),
                "sourceId": resolved_scope.map(|scope| scope.source_id.as_str()).unwrap_or(api_id),
                "upstreamModelPattern": resolved_scope.map(|scope| scope.upstream_model_pattern.as_str()).unwrap_or(model),
                "protocol": resolved_scope.map(|scope| scope.protocol.as_str()).unwrap_or("responses")
            },
            "overrideState": matching_override.map(|item| item.state.as_str()).unwrap_or("auto"),
            "observations": observations.iter()
                .filter(|item| item.scope.capability_key == crate::gateway::IMAGE_GENERATION_CAPABILITY)
                .map(|item| json!({
                    "state": item.state,
                    "source": item.observation_source,
                    "confidence": item.confidence,
                    "evidenceCode": item.evidence_code,
                    "lastObservedAt": item.last_observed_at,
                    "expiresAt": item.expires_at,
                    "occurrenceCount": item.occurrence_count,
                    "upstreamModelPattern": item.scope.upstream_model_pattern,
                    "protocol": item.scope.protocol
                }))
                .collect::<Vec<_>>()
        }]
    }))
}

pub(crate) fn set_aggregate_api_capability_override(
    api_id: &str,
    upstream_model_pattern: Option<&str>,
    protocol: Option<&str>,
    capability_key: Option<&str>,
    state: &str,
) -> Result<Value, String> {
    let scope = validate_scope(api_id, upstream_model_pattern, protocol, capability_key)?;
    let state = state.trim().to_ascii_lowercase();
    if state == "auto" {
        return reset_aggregate_api_capability_override(
            api_id,
            Some(scope.upstream_model_pattern.as_str()),
            Some(scope.protocol.as_str()),
            Some(scope.capability_key.as_str()),
        );
    }
    if !matches!(state.as_str(), "supported" | "unsupported") {
        return Err(
            "capability override state must be auto, supported, or unsupported".to_string(),
        );
    }
    let storage = open_storage()?;
    let now = now_ts();
    storage
        .upsert_gateway_capability_override(&GatewayCapabilityOverrideRecord {
            scope,
            state,
            created_at: now,
            updated_at: now,
        })
        .map_err(|err| format!("save capability override failed: {err}"))?;
    get_aggregate_api_capabilities(api_id)
}

pub(crate) fn reset_aggregate_api_capability_override(
    api_id: &str,
    upstream_model_pattern: Option<&str>,
    protocol: Option<&str>,
    capability_key: Option<&str>,
) -> Result<Value, String> {
    let scope = validate_scope(api_id, upstream_model_pattern, protocol, capability_key)?;
    open_storage()?
        .delete_gateway_capability_override(&scope)
        .map_err(|err| format!("reset capability override failed: {err}"))?;
    get_aggregate_api_capabilities(api_id)
}

pub(crate) fn clear_aggregate_api_capability_observation(
    api_id: &str,
    upstream_model_pattern: Option<&str>,
    protocol: Option<&str>,
    capability_key: Option<&str>,
) -> Result<Value, String> {
    let scope = validate_scope(api_id, upstream_model_pattern, protocol, capability_key)?;
    open_storage()?
        .clear_gateway_capability_observations(&scope)
        .map_err(|err| format!("clear capability observation failed: {err}"))?;
    get_aggregate_api_capabilities(api_id)
}

pub(crate) fn list_recent_aggregate_api_capability_attempts(
    api_id: &str,
    limit: i64,
) -> Result<Value, String> {
    let items = open_storage()?
        .list_gateway_upstream_attempt_events("aggregate_api", api_id, limit)
        .map_err(|err| format!("list capability attempts failed: {err}"))?;
    Ok(json!({ "items": items.into_iter().map(|item| json!({
        "id": item.id,
        "traceId": item.trace_id,
        "attemptIndex": item.attempt_index,
        "phase": item.phase,
        "supplierName": item.supplier_name,
        "upstreamModel": item.upstream_model,
        "protocol": item.protocol,
        "requestPath": item.request_path,
        "contractSignature": item.contract_signature,
        "capabilityDecisionsJson": item.capability_decisions_json,
        "transformCodesJson": item.transform_codes_json,
        "errorClass": item.error_class,
        "errorCode": item.error_code,
        "httpStatus": item.http_status,
        "durationMs": item.duration_ms,
        "outcome": item.outcome,
        "deliveryStarted": item.delivery_started,
        "createdAt": item.created_at
    })).collect::<Vec<_>>() }))
}

pub(crate) fn set_aggregate_api_capability_routing_mode(mode: &str) -> Result<Value, String> {
    let applied = crate::app_settings::set_gateway_capability_routing_mode(mode)?;
    Ok(json!({ "routingMode": applied }))
}
