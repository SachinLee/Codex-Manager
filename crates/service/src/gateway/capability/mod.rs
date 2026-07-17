mod classifier;
mod intent;
mod planner;
mod resolver;
mod runtime;
mod transforms;

pub(super) use classifier::classify_capability_error;
pub(super) use intent::{parse_required_capabilities, REQUIRED_CAPABILITIES_HEADER};
pub(crate) use intent::{structural_contract_signature, IMAGE_GENERATION_CAPABILITY};
pub(super) use planner::{
    record_runtime_capability_rejection, resolve_persisted_candidate_plan, CandidatePlan,
    CandidatePlanPhase,
};
pub(crate) use resolver::resolve_capability;
pub(crate) use runtime::{
    current_capability_routing_mode, set_capability_routing_mode, CapabilityRoutingMode,
};
pub(super) use transforms::{apply_transform, TransformCode};

#[cfg(test)]
mod tests;
