use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aikeeper_domain::{AgentRuntime, AgentSession};
use chrono::DateTime;
use serde_json::Value;

/// Discovers sessions in the current user's supported Agent Runtime stores.
pub fn discover_default() -> Vec<AgentSession> {
    let Some(home) = home_directory() else {
        return Vec::new();
    };
    discover(&home)
}

/// Discovers supported Agent Runtime sessions below a home directory.
///
/// Individual unavailable stores and invalid transcripts are deliberately ignored: these
/// files belong to the runtimes and discovery must not make them unavailable to their owners.
pub fn discover(home: &Path) -> Vec<AgentSession> {
    let mut sessions = discover_claude_code(&home.join(".claude/projects"));
    sessions.extend(discover_pi(&home.join(".pi/agent/sessions")));
    for (index, session) in sessions.iter_mut().enumerate() {
        session.discovery_order = index;
    }
    sessions
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn discover_claude_code(root: &Path) -> Vec<AgentSession> {
    let Ok(projects) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for project in projects.flatten() {
        let Ok(file_type) = project.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(transcripts) = fs::read_dir(project.path()) else {
            continue;
        };
        for transcript in transcripts.flatten() {
            let path = transcript.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Some(session) = parse_claude_transcript(&path, id) {
                sessions.push(session);
            }
        }
    }
    sessions
}

fn parse_claude_transcript(path: &Path, id: &str) -> Option<AgentSession> {
    let mut matching_primary_record = false;
    let mut cwd = None;
    let mut activity = None;
    for value in json_lines(path) {
        activity = latest(activity, value.get("timestamp").and_then(Value::as_str));
        if value.get("sessionId").and_then(Value::as_str) != Some(id)
            || value.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        matching_primary_record = true;
        if cwd.is_none() {
            cwd = value.get("cwd").and_then(Value::as_str).map(PathBuf::from);
        }
    }
    matching_primary_record.then(|| AgentSession {
        runtime: AgentRuntime::ClaudeCode,
        id: id.to_owned(),
        cwd,
        last_activity: activity.or_else(|| modified(path)),
        discovery_order: 0,
    })
}

fn discover_pi(root: &Path) -> Vec<AgentSession> {
    let Ok(directories) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for directory in directories.flatten() {
        let Ok(file_type) = directory.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(directory.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(session) = parse_pi_session(&path) {
                sessions.push(session);
            }
        }
    }
    sessions
}

fn parse_pi_session(path: &Path) -> Option<AgentSession> {
    let mut lines = json_lines(path);
    let header = lines.next()?;
    if header.get("type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    let id = header.get("id")?.as_str()?.to_owned();
    let cwd = header.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    let header_timestamp = header.get("timestamp").and_then(Value::as_str);
    let mut activity = None;
    for value in lines {
        if is_pi_message(&value) {
            let timestamp = value
                .get("message")
                .and_then(|message| message.get("timestamp"))
                .and_then(Value::as_str)
                .or_else(|| value.get("timestamp").and_then(Value::as_str));
            activity = latest(activity, timestamp);
        }
    }
    Some(AgentSession {
        runtime: AgentRuntime::Pi,
        id,
        cwd,
        last_activity: activity
            .or_else(|| header_timestamp.and_then(parse_timestamp))
            .or_else(|| modified(path)),
        discovery_order: 0,
    })
}

fn is_pi_message(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str) {
        Some("user") | Some("assistant") => true,
        Some("message") => matches!(
            value
                .get("message")
                .and_then(|message| message.get("role"))
                .and_then(Value::as_str),
            Some("user") | Some("assistant")
        ),
        _ => false,
    }
}

fn json_lines(path: &Path) -> impl Iterator<Item = Value> {
    File::open(path)
        .ok()
        .map(BufReader::new)
        .into_iter()
        .flat_map(|reader| reader.lines().map_while(Result::ok))
        .filter_map(|line| serde_json::from_str(&line).ok())
}

fn latest(current: Option<SystemTime>, timestamp: Option<&str>) -> Option<SystemTime> {
    match (current, timestamp.and_then(parse_timestamp)) {
        (Some(current), Some(candidate)) => Some(current.max(candidate)),
        (current, None) => current,
        (None, candidate) => candidate,
    }
}

fn parse_timestamp(value: &str) -> Option<SystemTime> {
    let timestamp = DateTime::parse_from_rfc3339(value)
        .ok()?
        .timestamp_nanos_opt()?;
    if timestamp >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_nanos(timestamp as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_nanos(timestamp.unsigned_abs()))
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use aikeeper_domain::AgentRuntime;

    use super::discover;

    fn test_home(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "aikeeper-store-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary home");
        root
    }

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().expect("fixture has parent"))
            .expect("create fixture parent");
        fs::write(path, body).expect("write fixture");
    }

    #[test]
    fn discovers_primary_claude_and_pi_sessions_without_subagents() {
        let home = test_home("primary");
        write(
            &home.join(".claude/projects/-workspace/claude-id.jsonl"),
            r#"{"sessionId":"claude-id","cwd":"/workspace","timestamp":"2026-01-02T00:00:00Z","isSidechain":false}
{"timestamp":"2026-01-03T00:00:00Z"}"#,
        );
        write(
            &home.join(".claude/projects/-workspace/claude-id/subagents/agent-child.jsonl"),
            r#"{"sessionId":"claude-id","cwd":"/workspace","timestamp":"2026-01-04T00:00:00Z","isSidechain":true}"#,
        );
        write(
            &home.join(".pi/agent/sessions/--workspace--/pi.jsonl"),
            r#"{"type":"session","id":"pi-id","timestamp":"2026-01-01T00:00:00Z","cwd":"/pi-workspace"}
{"type":"message","timestamp":"2026-01-05T00:00:00Z","message":{"role":"assistant"}}"#,
        );

        let sessions = discover(&home);
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|session| {
            session.runtime == AgentRuntime::ClaudeCode
                && session.id == "claude-id"
                && session.cwd.as_deref() == Some(Path::new("/workspace"))
        }));
        assert!(sessions.iter().any(|session| {
            session.runtime == AgentRuntime::Pi
                && session.id == "pi-id"
                && session.cwd.as_deref() == Some(Path::new("/pi-workspace"))
        }));
        fs::remove_dir_all(home).expect("clean temporary home");
    }

    #[test]
    fn skips_missing_stores_and_uses_file_metadata_when_dates_are_absent() {
        let home = test_home("missing");
        assert!(discover(&home).is_empty());

        write(
            &home.join(".claude/projects/-workspace/no-date.jsonl"),
            r#"{"sessionId":"no-date","cwd":"/workspace","isSidechain":false}"#,
        );
        write(
            &home.join(".pi/agent/sessions/--workspace--/no-date.jsonl"),
            r#"{"type":"session","id":"pi-no-date"}"#,
        );
        write(
            &home.join(".pi/agent/sessions/--workspace--/broken.jsonl"),
            "not json",
        );

        let sessions = discover(&home);
        assert_eq!(sessions.len(), 2);
        assert!(
            sessions
                .iter()
                .all(|session| session.last_activity.is_some())
        );
        fs::remove_dir_all(home).expect("clean temporary home");
    }
}
