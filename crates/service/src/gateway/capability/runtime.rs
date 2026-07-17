use std::sync::{OnceLock, RwLock};

pub(crate) const CAPABILITY_ROUTING_MODE_ENV: &str = "CODEXMANAGER_CAPABILITY_ROUTING_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityRoutingMode {
    Off,
    Observe,
    Enforce,
}

impl CapabilityRoutingMode {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "observe" => Ok(Self::Observe),
            "enforce" => Ok(Self::Enforce),
            _ => Err("capability routing mode must be off, observe, or enforce".to_string()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Enforce => "enforce",
        }
    }
}

fn configured_mode() -> &'static RwLock<CapabilityRoutingMode> {
    static MODE: OnceLock<RwLock<CapabilityRoutingMode>> = OnceLock::new();
    MODE.get_or_init(|| RwLock::new(CapabilityRoutingMode::Enforce))
}

pub(crate) fn current_capability_routing_mode() -> CapabilityRoutingMode {
    if let Ok(raw) = std::env::var(CAPABILITY_ROUTING_MODE_ENV) {
        if !raw.trim().is_empty() {
            return CapabilityRoutingMode::parse(&raw).unwrap_or_else(|err| {
                log::warn!("invalid {CAPABILITY_ROUTING_MODE_ENV}: {err}; using enforce");
                CapabilityRoutingMode::Enforce
            });
        }
    }
    configured_mode()
        .read()
        .map(|guard| *guard)
        .unwrap_or(CapabilityRoutingMode::Enforce)
}

pub(crate) fn set_capability_routing_mode(value: &str) -> Result<CapabilityRoutingMode, String> {
    let mode = CapabilityRoutingMode::parse(value)?;
    *configured_mode()
        .write()
        .map_err(|_| "capability routing mode lock poisoned".to_string())? = mode;
    Ok(mode)
}

#[cfg(test)]
pub(crate) fn reset_capability_routing_mode_for_test() {
    if let Ok(mut guard) = configured_mode().write() {
        *guard = CapabilityRoutingMode::Enforce;
    }
}
