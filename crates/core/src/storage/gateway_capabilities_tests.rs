use super::*;

fn scope(capability_key: &str) -> GatewayCapabilityScope {
    GatewayCapabilityScope {
        source_kind: "aggregate_api".to_string(),
        source_id: "agg-capability".to_string(),
        upstream_model_pattern: "grok-4.5".to_string(),
        protocol: "responses".to_string(),
        capability_key: capability_key.to_string(),
    }
}

#[test]
fn override_round_trip_and_reset_are_scoped() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let value = GatewayCapabilityOverrideRecord {
        scope: scope("responses.hosted_tool.image_generation"),
        state: "unsupported".to_string(),
        created_at: 10,
        updated_at: 10,
    };

    storage
        .upsert_gateway_capability_override(&value)
        .expect("upsert");
    let values = storage
        .list_gateway_capability_overrides("aggregate_api", "agg-capability")
        .expect("list");
    assert_eq!(values, vec![value.clone()]);
    assert_eq!(
        storage
            .delete_gateway_capability_override(&value.scope)
            .expect("delete"),
        1
    );
    assert!(storage
        .list_gateway_capability_overrides("aggregate_api", "agg-capability")
        .expect("list after delete")
        .is_empty());
}

#[test]
fn repeated_observation_is_coalesced_and_expired_values_are_hidden() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let mut value = GatewayCapabilityObservationRecord {
        scope: scope("responses.hosted_tool.image_generation"),
        state: "unsupported".to_string(),
        observation_source: "runtime".to_string(),
        confidence: "high".to_string(),
        evidence_code: "capability.image_generation_not_enabled".to_string(),
        first_observed_at: 10,
        last_observed_at: 10,
        expires_at: 100,
        occurrence_count: 1,
        ..Default::default()
    };
    storage
        .upsert_gateway_capability_observation(&value)
        .expect("first upsert");
    value.last_observed_at = 20;
    value.expires_at = 200;
    storage
        .upsert_gateway_capability_observation(&value)
        .expect("second upsert");

    let values = storage
        .list_gateway_capability_observations("aggregate_api", "agg-capability", 50)
        .expect("list");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].occurrence_count, 2);
    assert_eq!(values[0].last_observed_at, 20);
    assert_eq!(values[0].expires_at, 200);
    assert!(storage
        .list_gateway_capability_observations("aggregate_api", "agg-capability", 201)
        .expect("expired list")
        .is_empty());
}

#[test]
fn attempt_events_store_only_explicit_redacted_contract_fields() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let event = GatewayUpstreamAttemptEvent {
        trace_id: "trace-1".to_string(),
        attempt_index: 1,
        phase: "downgrade".to_string(),
        source_kind: "aggregate_api".to_string(),
        source_id: "agg-capability".to_string(),
        protocol: "responses".to_string(),
        request_path: "/v1/responses".to_string(),
        contract_signature: "tools:image_generation=1".to_string(),
        capability_decisions_json: "[\"image_generation:optional\"]".to_string(),
        transform_codes_json: "[\"drop_optional_image_generation\"]".to_string(),
        error_class: Some("capability".to_string()),
        error_code: Some("capability.image_generation_not_enabled".to_string()),
        http_status: Some(403),
        outcome: "retry".to_string(),
        created_at: 10,
        ..Default::default()
    };
    storage
        .insert_gateway_upstream_attempt_event(&event)
        .expect("insert");
    let values = storage
        .list_gateway_upstream_attempt_events("aggregate_api", "agg-capability", 10)
        .expect("list");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].transform_codes_json, event.transform_codes_json);
    assert_eq!(values[0].error_code, event.error_code);
}

#[test]
fn capability_maintenance_prunes_expired_facts_and_old_attempts() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");
    let now = 2_000_000;
    storage
        .upsert_gateway_capability_observation(&GatewayCapabilityObservationRecord {
            scope: scope("responses.hosted_tool.image_generation"),
            state: "unsupported".to_string(),
            observation_source: "runtime".to_string(),
            confidence: "high".to_string(),
            evidence_code: "expired".to_string(),
            first_observed_at: now - 20,
            last_observed_at: now - 20,
            expires_at: now - 1,
            occurrence_count: 1,
            ..Default::default()
        })
        .expect("insert observation");
    storage
        .insert_gateway_upstream_attempt_event(&GatewayUpstreamAttemptEvent {
            trace_id: "trace-old".to_string(),
            attempt_index: 0,
            phase: "native".to_string(),
            source_kind: "aggregate_api".to_string(),
            source_id: "agg-capability".to_string(),
            protocol: "responses".to_string(),
            request_path: "/v1/responses".to_string(),
            contract_signature: "v1".to_string(),
            outcome: "failed".to_string(),
            created_at: now - 15 * 86_400,
            ..Default::default()
        })
        .expect("insert old attempt");

    assert_eq!(
        storage
            .prune_expired_gateway_capability_observations(now)
            .expect("prune observations"),
        1
    );
    assert_eq!(
        storage
            .prune_gateway_upstream_attempt_events_by_retention(now)
            .expect("prune attempts"),
        1
    );
}
