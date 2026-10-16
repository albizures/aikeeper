use std::{fmt, path::PathBuf, str::FromStr, time::SystemTime};

/// An external system that owns Agent Sessions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AgentRuntime {
    ClaudeCode,
    Pi,
}

impl AgentRuntime {
    pub const ALL: [Self; 2] = [Self::ClaudeCode, Self::Pi];
}

impl fmt::Display for AgentRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClaudeCode => formatter.write_str("claude-code"),
            Self::Pi => formatter.write_str("pi"),
        }
    }
}

impl FromStr for AgentRuntime {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "claude-code" => Ok(Self::ClaudeCode),
            "pi" => Ok(Self::Pi),
            _ => Err(format!(
                "unknown agent runtime {value:?}; expected claude-code or pi"
            )),
        }
    }
}

/// A read-only reference to a session owned by an Agent Runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentSession {
    pub runtime: AgentRuntime,
    pub id: String,
    pub cwd: Option<PathBuf>,
    pub last_activity: Option<SystemTime>,
    /// Stable order in which the session was discovered, used for undated rows.
    pub discovery_order: usize,
}
