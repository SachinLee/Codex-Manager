use bytes::Bytes;
use codexmanager_core::storage::{
    now_ts, GatewayCapabilityObservationRecord, GatewayCapabilityScope, Storage,
};

use super::{
    intent::{inspect_responses_capabilities, CapabilityRequirement},
    resolver::{CapabilityState, EffectiveCapability},
    transforms::{apply_transform, TransformCode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CandidatePlanPhase {
    Native,
    Downgrade,
    Incompatible,
}

impl CandidatePlanPhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Downgrade => "downgrade",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CandidatePlan {
    pub phase: CandidatePlanPhase,
    pub effective_body: Bytes,
    pub transform_codes: Vec<&'static str>,
}

pub(crate) fn plan_candidate_request(
    path: &str,
    original_body: &[u8],
    image_generation_declared_required: bool,
    image_generation: &EffectiveCapability,
    enforce: bool,
) -> CandidatePlan {
    let requirement =
        inspect_responses_capabilities(path, original_body, image_generation_declared_required)
            .image_generation;
    if image_generation.state != CapabilityState::Unsupported
        || requirement == CapabilityRequirement::Absent
        || !enforce
    {
        return CandidatePlan {
            phase: CandidatePlanPhase::Native,
            effective_body: Bytes::copy_from_slice(original_body),
            transform_codes: Vec::new(),
        };
    }
    if requirement == CapabilityRequirement::Required {
        return CandidatePlan {
            phase: CandidatePlanPhase::Incompatible,
            effective_body: Bytes::copy_from_slice(original_body),
            transform_codes: Vec::new(),
        };
    }
    let transform = TransformCode::DropOptionalImageGeneration;
    if let Some(effective_body) = apply_transform(original_body, transform) {
        CandidatePlan {
            phase: CandidatePlanPhase::Downgrade,
            effective_body,
            transform_codes: vec![transform.as_str()],
        }
    } else {
        CandidatePlan {
            phase: CandidatePlanPhase::Incompatible,
            effective_body: Bytes::copy_from_slice(original_body),
            transform_codes: Vec::new(),
        }
    }
}

pub(crate) fn resolve_persisted_candidate_plan(
    storage: &Storage,
    source_id: &str,
    upstream_model: &str,
    protocol: &str,
    path: &str,
    original_body: &[u8],
    image_generation_declared_required: bool,
    mode: super::runtime::CapabilityRoutingMode,
) -> Result<CandidatePlan, String> {
    let now = now_ts();
    let overrides = storage
        .list_gateway_capability_overrides("aggregate_api", source_id)
        .map_err(|err| format!("list capability overrides failed: {err}"))?;
    let observations = storage
        .list_gateway_capability_observations("aggregate_api", source_id, now)
        .map_err(|err| format!("list capability observations failed: {err}"))?;
    let effective = super::resolver::resolve_capability(
        &overrides,
        &observations,
        upstream_model,
        protocol,
        super::intent::IMAGE_GENERATION_CAPABILITY,
        now,
    );
    let mut plan = plan_candidate_request(
        path,
        original_body,
        image_generation_declared_required,
        &effective,
        true,
    );
    if mode == super::runtime::CapabilityRoutingMode::Observe {
        plan.effective_body = Bytes::copy_from_slice(original_body);
    }
    Ok(plan)
}

pub(crate) fn record_runtime_capability_rejection(
    storage: &Storage,
    source_id: &str,
    upstream_model: &str,
    protocol: &str,
    capability_key: &str,
    evidence_code: &str,
) -> Result<(), String> {
    let observed_at = now_ts();
    storage
        .upsert_gateway_capability_observation(&GatewayCapabilityObservationRecord {
            scope: GatewayCapabilityScope {
                source_kind: "aggregate_api".to_string(),
                source_id: source_id.to_string(),
                upstream_model_pattern: upstream_model.to_string(),
                protocol: protocol.to_string(),
                capability_key: capability_key.to_string(),
            },
            state: "unsupported".to_string(),
            observation_source: "runtime".to_string(),
            confidence: "high".to_string(),
            evidence_code: evidence_code.to_string(),
            first_observed_at: observed_at,
            last_observed_at: observed_at,
            expires_at: observed_at.saturating_add(7 * 86_400),
            occurrence_count: 1,
            ..Default::default()
        })
        .map_err(|err| format!("record capability observation failed: {err}"))
}
