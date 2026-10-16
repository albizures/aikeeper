use std::{
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aikeeper_domain::{AgentRuntime, AgentSession};
use chrono::DateTime;
use rusqlite::{Connection, OptionalExtension, params};
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
    env::var_os("HOME").map(PathBuf::from)
}

/// SQLite-backed user-owned Session Metadata.
pub struct MetadataStore {
    connection: Connection,
}

impl MetadataStore {
    /// Opens the current user's metadata database, creating and migrating it if needed.
    pub fn open_default() -> Result<Self, String> {
        Self::open(&metadata_database_path()?)
    }

    /// Opens a metadata database at `path`, creating and migrating it if needed.
    pub fn open(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .ok_or_else(|| format!("metadata database path {} has no parent", path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create metadata directory {}: {error}", parent.display()))?;
        let connection = Connection::open(path)
            .map_err(|error| format!("open metadata database {}: {error}", path.display()))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| format!("configure metadata database: {error}"))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS session_metadata (
                    runtime TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    title TEXT,
                    PRIMARY KEY (runtime, session_id)
                );
                CREATE TABLE IF NOT EXISTS session_tags (
                    runtime TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    tag TEXT NOT NULL,
                    PRIMARY KEY (runtime, session_id, tag)
                );",
            )
            .map_err(|error| format!("migrate metadata database: {error}"))?;
        Ok(Self { connection })
    }

    /// Adds stored metadata to currently discovered Agent Sessions.
    pub fn hydrate_sessions(&self, sessions: &mut [AgentSession]) -> Result<(), String> {
        for session in sessions {
            let runtime = session.runtime.to_string();
            session.metadata.title = self
                .connection
                .query_row(
                    "SELECT title FROM session_metadata WHERE runtime = ?1 AND session_id = ?2",
                    params![runtime, session.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| format!("read session title: {error}"))?;
            let mut statement = self
                .connection
                .prepare(
                    "SELECT tag FROM session_tags WHERE runtime = ?1 AND session_id = ?2 ORDER BY tag",
                )
                .map_err(|error| format!("read session tags: {error}"))?;
            session.metadata.tags = statement
                .query_map(params![session.runtime.to_string(), session.id], |row| {
                    row.get(0)
                })
                .map_err(|error| format!("read session tags: {error}"))?
                .collect::<Result<Vec<String>, _>>()
                .map_err(|error| format!("read session tags: {error}"))?;
        }
        Ok(())
    }

    pub fn set_title(&self, runtime: AgentRuntime, id: &str, title: &str) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO session_metadata (runtime, session_id, title) VALUES (?1, ?2, ?3)
                 ON CONFLICT(runtime, session_id) DO UPDATE SET title = excluded.title",
                params![runtime.to_string(), id, title],
            )
            .map_err(|error| format!("store session title: {error}"))?;
        Ok(())
    }

    pub fn clear_title(&self, runtime: AgentRuntime, id: &str) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM session_metadata WHERE runtime = ?1 AND session_id = ?2",
                params![runtime.to_string(), id],
            )
            .map_err(|error| format!("clear session title: {error}"))?;
        Ok(())
    }

    pub fn add_tag(&self, runtime: AgentRuntime, id: &str, tag: &str) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT OR IGNORE INTO session_tags (runtime, session_id, tag) VALUES (?1, ?2, ?3)",
                params![runtime.to_string(), id, tag],
            )
            .map_err(|error| format!("store session tag: {error}"))?;
        Ok(())
    }

    pub fn remove_tag(&self, runtime: AgentRuntime, id: &str, tag: &str) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM session_tags WHERE runtime = ?1 AND session_id = ?2 AND tag = ?3",
                params![runtime.to_string(), id, tag],
            )
            .map_err(|error| format!("remove session tag: {error}"))?;
        Ok(())
    }
}

fn metadata_database_path() -> Result<PathBuf, String> {
    match env::var_os("XDG_DATA_HOME") {
        Some(value) if value.is_empty() => Err("XDG_DATA_HOME must not be empty".to_owned()),
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err("XDG_DATA_HOME must be an absolute path".to_owned());
            }
            Ok(path.join("aikeeper/aikeeper.sqlite3"))
        }
        None => home_directory()
            .map(|home| home.join(".local/share/aikeeper/aikeeper.sqlite3"))
            .ok_or_else(|| "HOME is required when XDG_DATA_HOME is unset".to_owned()),
    }
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
        metadata: Default::default(),
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
        metadata: Default::default(),
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

    use aikeeper_domain::{AgentRuntime, AgentSession};

    use super::{MetadataStore, discover};

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
    fn hydrates_titles_and_sorted_tags_for_matching_runtime_and_id() {
        let home = test_home("metadata");
        let store = MetadataStore::open(&home.join("metadata.sqlite3")).expect("open metadata");
        store
            .set_title(AgentRuntime::Pi, "shared-id", "Session title")
            .expect("set title");
        store
            .add_tag(AgentRuntime::Pi, "shared-id", "zebra")
            .expect("add tag");
        store
            .add_tag(AgentRuntime::Pi, "shared-id", "alpha")
            .expect("add tag");
        let mut sessions = vec![AgentSession {
            runtime: AgentRuntime::Pi,
            id: "shared-id".to_owned(),
            cwd: None,
            last_activity: None,
            metadata: Default::default(),
            discovery_order: 0,
        }];

        store
            .hydrate_sessions(&mut sessions)
            .expect("hydrate sessions");

        assert_eq!(sessions[0].metadata.title.as_deref(), Some("Session title"));
        assert_eq!(sessions[0].metadata.tags, ["alpha", "zebra"]);
        fs::remove_dir_all(home).expect("remove temporary home");
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
