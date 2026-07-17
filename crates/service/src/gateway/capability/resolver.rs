use codexmanager_core::storage::{
    GatewayCapabilityObservationRecord, GatewayCapabilityOverrideRecord, GatewayCapabilityScope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityState {
    Supported,
    Unsupported,
    Unknown,
}

impl CapabilityState {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "supported" => Self::Supported,
            "unsupported" => Self::Unsupported,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveCapability {
    pub state: CapabilityState,
    pub source: String,
    pub confidence: String,
    pub expires_at: Option<i64>,
    pub scope: Option<GatewayCapabilityScope>,
}

impl Default for EffectiveCapability {
    fn default() -> Self {
        Self {
            state: CapabilityState::Unknown,
            source: "unknown".to_string(),
            confidence: "low".to_string(),
            expires_at: None,
            scope: None,
        }
    }
}

fn scope_specificity(
    scope: &GatewayCapabilityScope,
    model: &str,
    protocol: &str,
    capability_key: &str,
) -> Option<u8> {
    if scope.capability_key != capability_key {
        return None;
    }
    let model_score = if scope.upstream_model_pattern == model {
        2
    } else if scope.upstream_model_pattern == "*" {
        0
    } else {
        return None;
    };
    let protocol_score = if scope.protocol == protocol {
        1
    } else if scope.protocol == "*" {
        0
    } else {
        return None;
    };
    Some(model_score + protocol_score)
}

pub(crate) fn resolve_capability(
    overrides: &[GatewayCapabilityOverrideRecord],
    observations: &[GatewayCapabilityObservationRecord],
    model: &str,
    protocol: &str,
    capability_key: &str,
    now: i64,
) -> EffectiveCapability {
    if let Some((value, _)) = overrides
        .iter()
        .filter_map(|value| {
            scope_specificity(&value.scope, model, protocol, capability_key)
                .map(|specificity| (value, specificity))
        })
        .max_by_key(|(value, specificity)| (*specificity, value.updated_at))
    {
        return EffectiveCapability {
            state: CapabilityState::parse(&value.state),
            source: "operator".to_string(),
            confidence: "high".to_string(),
            expires_at: None,
            scope: Some(value.scope.clone()),
        };
    }

    for observation_source in ["runtime", "probe"] {
        if let Some((value, _)) = observations
            .iter()
            .filter(|value| {
                value.observation_source == observation_source && value.expires_at > now
            })
            .filter_map(|value| {
                scope_specificity(&value.scope, model, protocol, capability_key)
                    .map(|specificity| (value, specificity))
            })
            .max_by_key(|(value, specificity)| (*specificity, value.last_observed_at))
        {
            return EffectiveCapability {
                state: CapabilityState::parse(&value.state),
                source: observation_source.to_string(),
                confidence: value.confidence.clone(),
                expires_at: Some(value.expires_at),
                scope: Some(value.scope.clone()),
            };
        }
    }
    EffectiveCapability::default()
}
