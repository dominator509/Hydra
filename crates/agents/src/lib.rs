//! layer L3 agents — orchestration, deduplication, and notification.

pub mod bridge_engineer;
pub mod comms;
pub mod data_steward;
pub mod skills;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapabilityAvailability {
    Available,
    Experimental,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentCapabilityDescriptor {
    pub name: String,
    pub availability: AgentCapabilityAvailability,
    pub envelope_only: bool,
    pub reason: Option<String>,
}
