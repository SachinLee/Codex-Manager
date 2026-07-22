mod classifier;
mod intent;
mod planner;
mod resolver;
mod runtime;
mod transforms;

pub(crate) use classifier::classify_capability_error;
pub(crate) use intent::{
    parse_required_capabilities, structural_contract_signature, IMAGE_GENERATION_CAPABILITY,
    REQUIRED_CAPABILITIES_HEADER,
};
pub(crate) use planner::{
    record_runtime_capability_rejection, resolve_persisted_candidate_plan, CandidatePlan,
    CandidatePlanPhase,
};
pub(crate) use resolver::resolve_capability;
pub(crate) use runtime::{
    current_capability_routing_mode, set_capability_routing_mode, CapabilityRoutingMode,
};
pub(crate) use transforms::{apply_transform, TransformCode};

#[cfg(test)]
mod tests;
