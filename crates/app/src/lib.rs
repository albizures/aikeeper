use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path},
    str::FromStr,
};

use aikeeper_domain::{AgentRuntime, AgentSession};

#[derive(Clone, Debug)]
pub struct FolderFilter(Vec<OsString>);

impl FolderFilter {
    fn matches(&self, path: &Path) -> bool {
        let components = path_components(path);
        components
            .windows(self.0.len())
            .any(|window| window == self.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ListOptions {
    pub agent: Option<AgentRuntime>,
    pub folder: Option<FolderFilter>,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub limit: Option<usize>,
}

pub fn parse_agent(value: &str) -> Result<AgentRuntime, String> {
    AgentRuntime::from_str(value)
}

pub fn parse_folder(value: &str) -> Result<FolderFilter, String> {
    let components = path_components(Path::new(value));
    if components.is_empty() {
        return Err("--folder must contain a path component".to_owned());
    }
    Ok(FolderFilter(components))
}

pub fn parse_title_filter(value: &str) -> Result<String, String> {
    normalize_text(value, "--title", 256, true)
}

pub fn parse_tag(value: &str) -> Result<String, String> {
    normalize_text(value, "--tag", 64, false)
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
    if let Some(folder) = options.folder {
        sessions.retain(|session| {
            session
                .cwd
                .as_deref()
                .is_some_and(|cwd| folder.matches(cwd))
        });
    }
    if let Some(title) = options.title {
        sessions.retain(|session| {
            session
                .metadata
                .title
                .as_deref()
                .is_some_and(|candidate| candidate.to_lowercase().contains(&title))
        });
    }
    if !options.tags.is_empty() {
        sessions.retain(|session| {
            options.tags.iter().all(|tag| {
                session
                    .metadata
                    .tags
                    .iter()
                    .any(|candidate| candidate == tag)
            })
        });
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

fn normalize_text(
    value: &str,
    option: &str,
    maximum_length: usize,
    allows_comma: bool,
) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{option} must not be empty"));
    }
    if value.chars().count() > maximum_length {
        return Err(format!(
            "{option} must be at most {maximum_length} characters"
        ));
    }
    if value.contains(['\t', '\r', '\n']) || (!allows_comma && value.contains(',')) {
        return Err(format!("{option} contains unsupported characters"));
    }
    Ok(value.to_lowercase())
}

fn path_components(path: &Path) -> Vec<OsString> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(component) => Some(component.to_owned()),
            Component::ParentDir => Some(OsStr::new("..").to_owned()),
            Component::CurDir | Component::RootDir | Component::Prefix(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{Duration, UNIX_EPOCH},
    };

    use aikeeper_domain::{AgentRuntime, AgentSession};

    use super::{
        ListOptions, list_sessions, parse_agent, parse_folder, parse_limit, parse_tag,
        parse_title_filter,
    };

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
            metadata: Default::default(),
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
                ..ListOptions::default()
            },
        );

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "pi");
    }

    #[test]
    fn filters_by_normalized_title_and_requires_every_tag() {
        let mut matching = session(AgentRuntime::Pi, "matching", Some(1), 0);
        matching.metadata.title = Some("RÉSUMÉ notes".to_owned());
        matching.metadata.tags = vec!["alpha".to_owned(), "beta".to_owned()];
        let mut missing_tag = session(AgentRuntime::Pi, "missing", Some(2), 1);
        missing_tag.metadata.title = Some("résumé draft".to_owned());
        missing_tag.metadata.tags = vec!["alpha".to_owned()];

        let sessions = list_sessions(
            vec![matching, missing_tag],
            ListOptions {
                title: Some(parse_title_filter("RÉSUMÉ").expect("valid title filter")),
                tags: vec![
                    parse_tag("ALPHA").expect("valid tag"),
                    parse_tag("beta").expect("valid tag"),
                ],
                ..ListOptions::default()
            },
        );

        assert_eq!(
            sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["matching"]
        );
    }

    #[test]
    fn filters_by_a_contiguous_working_directory_component_sequence() {
        let mut aikeeper = session(AgentRuntime::Pi, "aikeeper", Some(1), 0);
        aikeeper.cwd = Some(PathBuf::from("/home/a/projects/aikeeper"));
        let mut keeper = session(AgentRuntime::Pi, "keeper", Some(2), 1);
        keeper.cwd = Some(PathBuf::from("/home/a/projects/keeper"));
        let mut undirected = session(AgentRuntime::Pi, "undirected", Some(3), 2);
        undirected.cwd = None;

        let sessions = list_sessions(
            vec![aikeeper, keeper, undirected],
            ListOptions {
                folder: Some(parse_folder("projects//./aikeeper/").expect("valid filter")),
                ..ListOptions::default()
            },
        );

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "aikeeper");
        let mut partial_component = session(AgentRuntime::Pi, "partial", Some(1), 0);
        partial_component.cwd = Some(PathBuf::from("/home/a/projects/aikeeper"));
        assert!(
            list_sessions(
                vec![partial_component],
                ListOptions {
                    folder: Some(parse_folder("keep").expect("valid filter")),
                    ..ListOptions::default()
                },
            )
            .is_empty()
        );
    }

    #[test]
    fn normalizes_title_filters_and_rejects_blank_values() {
        assert_eq!(parse_title_filter("  RÉSUMÉ  "), Ok("résumé".to_owned()));
        assert!(parse_title_filter(" \t ").is_err());
    }

    #[test]
    fn rejects_unknown_agent_non_positive_limit_and_empty_folder() {
        assert!(parse_agent("other").is_err());
        assert!(parse_limit("0").is_err());
        assert!(parse_limit("-1").is_err());
        assert_eq!(parse_limit("2"), Ok(2));
        assert!(parse_folder("").is_err());
        assert!(parse_folder("//./").is_err());
    }
}
