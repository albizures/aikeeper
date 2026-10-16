use std::str::FromStr;

use aikeeper_domain::{AgentRuntime, AgentSession};

#[derive(Clone, Copy, Debug, Default)]
pub struct ListOptions {
    pub agent: Option<AgentRuntime>,
    pub limit: Option<usize>,
}

pub fn parse_agent(value: &str) -> Result<AgentRuntime, String> {
    AgentRuntime::from_str(value)
}

pub fn parse_limit(value: &str) -> Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| format!("--limit must be a positive integer, got {value:?}"))?;
    if limit == 0 {
        return Err("--limit must be a positive integer".to_owned());
    }
    Ok(limit)
}

pub fn list_sessions(mut sessions: Vec<AgentSession>, options: ListOptions) -> Vec<AgentSession> {
    if let Some(runtime) = options.agent {
        sessions.retain(|session| session.runtime == runtime);
    }

    sessions.sort_by(
        |left, right| match (left.last_activity, right.last_activity) {
            (Some(left), Some(right)) => right.cmp(&left),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.discovery_order.cmp(&right.discovery_order),
        },
    );

    if let Some(limit) = options.limit {
        sessions.truncate(limit);
    }
    sessions
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use aikeeper_domain::{AgentRuntime, AgentSession};

    use super::{ListOptions, list_sessions, parse_agent, parse_limit};

    fn session(
        runtime: AgentRuntime,
        id: &str,
        activity: Option<u64>,
        order: usize,
    ) -> AgentSession {
        AgentSession {
            runtime,
            id: id.to_owned(),
            cwd: None,
            last_activity: activity.map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
            discovery_order: order,
        }
    }

    #[test]
    fn lists_dated_sessions_newest_first_then_undated_discovery_order() {
        let sessions = list_sessions(
            vec![
                session(AgentRuntime::Pi, "old", Some(1), 2),
                session(AgentRuntime::ClaudeCode, "undated-later", None, 4),
                session(AgentRuntime::Pi, "new", Some(3), 3),
                session(AgentRuntime::Pi, "undated-first", None, 1),
            ],
            ListOptions::default(),
        );

        assert_eq!(
            sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["new", "old", "undated-first", "undated-later"]
        );
    }

    #[test]
    fn filters_and_limits_sessions() {
        let sessions = list_sessions(
            vec![
                session(AgentRuntime::Pi, "pi", Some(1), 0),
                session(AgentRuntime::ClaudeCode, "claude", Some(2), 1),
            ],
            ListOptions {
                agent: Some(AgentRuntime::Pi),
                limit: Some(1),
            },
        );

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "pi");
    }

    #[test]
    fn rejects_unknown_agent_and_non_positive_limit() {
        assert!(parse_agent("other").is_err());
        assert!(parse_limit("0").is_err());
        assert!(parse_limit("-1").is_err());
        assert_eq!(parse_limit("2"), Ok(2));
    }
}
