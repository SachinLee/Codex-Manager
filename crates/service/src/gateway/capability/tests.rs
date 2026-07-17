use bytes::Bytes;

use super::{
    classifier::classify_capability_error,
    intent::{
        inspect_responses_capabilities, parse_required_capabilities, CapabilityRequirement,
        IMAGE_GENERATION_CAPABILITY,
    },
    planner::{plan_candidate_request, CandidatePlanPhase},
    resolver::{resolve_capability, CapabilityState},
    runtime::{
        current_capability_routing_mode, reset_capability_routing_mode_for_test,
        set_capability_routing_mode, CapabilityRoutingMode, CAPABILITY_ROUTING_MODE_ENV,
    },
    transforms::{apply_transform, TransformCode},
};
use codexmanager_core::storage::{
    GatewayCapabilityObservationRecord, GatewayCapabilityOverrideRecord, GatewayCapabilityScope,
};

#[test]
fn auto_image_tool_is_optional_and_forced_image_tool_is_required() {
    let auto = br#"{
        "model":"grok-4.5",
        "input":[],
        "tools":[{"type":"image_generation"}],
        "tool_choice":"auto"
    }"#;
    let forced = br#"{
        "model":"grok-4.5",
        "input":[],
        "tools":[{"type":"image_generation"}],
        "tool_choice":{"type":"image_generation"}
    }"#;

    assert_eq!(
        inspect_responses_capabilities("/v1/responses", auto, false).image_generation,
        CapabilityRequirement::Optional
    );
    assert_eq!(
        inspect_responses_capabilities("/v1/responses", forced, false).image_generation,
        CapabilityRequirement::Required
    );
}

#[test]
fn private_required_capability_is_validated_and_makes_image_required() {
    let body = br#"{"input":[],"tools":[{"type":"image_generation"}],"tool_choice":"auto"}"#;
    let declared =
        parse_required_capabilities(IMAGE_GENERATION_CAPABILITY).expect("known capability");
    assert_eq!(declared, vec![IMAGE_GENERATION_CAPABILITY]);
    assert_eq!(
        inspect_responses_capabilities("/v1/responses", body, true).image_generation,
        CapabilityRequirement::Required
    );
    assert!(parse_required_capabilities("responses.unknown").is_err());
}

#[test]
fn optional_image_transform_returns_a_new_body_without_mutating_original() {
    let original = Bytes::from_static(
        br#"{"input":[],"tools":[{"type":"function","name":"shell"},{"type":"image_generation"}],"tool_choice":"auto"}"#,
    );

    let transformed = apply_transform(
        original.as_ref(),
        TransformCode::DropOptionalImageGeneration,
    )
    .expect("safe transform");
    let transformed_json: serde_json::Value =
        serde_json::from_slice(transformed.as_ref()).expect("transformed json");
    let original_json: serde_json::Value =
        serde_json::from_slice(original.as_ref()).expect("original json");

    assert_eq!(original_json["tools"].as_array().expect("tools").len(), 2);
    assert_eq!(
        transformed_json["tools"].as_array().expect("tools").len(),
        1
    );
    assert_eq!(transformed_json["tools"][0]["type"], "function");
}

#[test]
fn image_transform_rejects_forced_tool_choice() {
    let body = br#"{
        "input":[],
        "tools":[{"type":"image_generation"}],
        "tool_choice":{"type":"image_generation"}
    }"#;

    assert!(apply_transform(body, TransformCode::DropOptionalImageGeneration).is_none());
}

#[test]
fn exact_image_entitlement_error_is_classified_but_generic_502_is_not() {
    let exact = br#"{"error":{"message":"Image generation is not enabled for this group","type":"permission_error"}}"#;
    let generic = br#"{"error":{"message":"Upstream request failed","type":"upstream_error"}}"#;

    let classified = classify_capability_error(403, exact).expect("capability error");
    assert_eq!(classified.code, "capability.image_generation_not_enabled");
    assert_eq!(
        classified.capability_key,
        "responses.hosted_tool.image_generation"
    );
    assert!(classify_capability_error(502, generic).is_none());
}

fn scope(model: &str, protocol: &str) -> GatewayCapabilityScope {
    GatewayCapabilityScope {
        source_kind: "aggregate_api".to_string(),
        source_id: "agg".to_string(),
        upstream_model_pattern: model.to_string(),
        protocol: protocol.to_string(),
        capability_key: "responses.hosted_tool.image_generation".to_string(),
    }
}

#[test]
fn operator_override_wins_over_runtime_and_runtime_wins_over_probe() {
    let overrides = vec![GatewayCapabilityOverrideRecord {
        scope: scope("*", "*"),
        state: "supported".to_string(),
        created_at: 1,
        updated_at: 1,
    }];
    let observations = vec![
        GatewayCapabilityObservationRecord {
            scope: scope("grok-4.5", "responses"),
            state: "unsupported".to_string(),
            observation_source: "runtime".to_string(),
            confidence: "high".to_string(),
            evidence_code: "runtime".to_string(),
            last_observed_at: 20,
            expires_at: 100,
            ..Default::default()
        },
        GatewayCapabilityObservationRecord {
            scope: scope("grok-4.5", "responses"),
            state: "supported".to_string(),
            observation_source: "probe".to_string(),
            confidence: "high".to_string(),
            evidence_code: "probe".to_string(),
            last_observed_at: 30,
            expires_at: 100,
            ..Default::default()
        },
    ];

    let operator = resolve_capability(
        &overrides,
        &observations,
        "grok-4.5",
        "responses",
        "responses.hosted_tool.image_generation",
        50,
    );
    assert_eq!(operator.state, CapabilityState::Supported);
    assert_eq!(operator.source, "operator");

    let runtime = resolve_capability(
        &[],
        &observations,
        "grok-4.5",
        "responses",
        "responses.hosted_tool.image_generation",
        50,
    );
    assert_eq!(runtime.state, CapabilityState::Unsupported);
    assert_eq!(runtime.source, "runtime");
}

#[test]
fn expired_observation_resolves_unknown() {
    let observations = vec![GatewayCapabilityObservationRecord {
        scope: scope("grok-4.5", "responses"),
        state: "unsupported".to_string(),
        observation_source: "runtime".to_string(),
        expires_at: 10,
        ..Default::default()
    }];
    let effective = resolve_capability(
        &[],
        &observations,
        "grok-4.5",
        "responses",
        "responses.hosted_tool.image_generation",
        11,
    );
    assert_eq!(effective.state, CapabilityState::Unknown);
}

#[test]
fn unsupported_optional_image_is_downgraded_but_required_image_is_incompatible() {
    let optional = br#"{"input":[],"tools":[{"type":"image_generation"}],"tool_choice":"auto"}"#;
    let required = br#"{"input":[],"tools":[{"type":"image_generation"}],"tool_choice":{"type":"image_generation"}}"#;
    let effective = super::resolver::EffectiveCapability {
        state: CapabilityState::Unsupported,
        source: "runtime".to_string(),
        confidence: "high".to_string(),
        expires_at: Some(100),
        scope: None,
    };

    let optional_plan = plan_candidate_request("/v1/responses", optional, false, &effective, true);
    assert_eq!(optional_plan.phase, CandidatePlanPhase::Downgrade);
    assert_eq!(
        optional_plan.transform_codes,
        vec!["drop_optional_image_generation"]
    );
    let required_plan = plan_candidate_request("/v1/responses", required, false, &effective, true);
    assert_eq!(required_plan.phase, CandidatePlanPhase::Incompatible);
}

#[test]
fn capability_routing_mode_defaults_to_enforce_and_validates_values() {
    let _guard = crate::test_env_guard();
    std::env::remove_var(CAPABILITY_ROUTING_MODE_ENV);
    reset_capability_routing_mode_for_test();
    assert_eq!(
        current_capability_routing_mode(),
        CapabilityRoutingMode::Enforce
    );
    assert_eq!(
        set_capability_routing_mode("observe").expect("observe mode"),
        CapabilityRoutingMode::Observe
    );
    assert_eq!(
        current_capability_routing_mode(),
        CapabilityRoutingMode::Observe
    );
    assert!(set_capability_routing_mode("unsafe").is_err());
    reset_capability_routing_mode_for_test();
}
