use std::{env, process};

use aikeeper_app::{
    ListOptions, list_sessions, parse_agent, parse_folder, parse_limit, parse_tag,
    parse_title_filter,
};
use aikeeper_store::{MetadataStore, discover_default};

fn main() {
    if let Err(error) = run(env::args().skip(1)) {
        eprintln!("error: {error}");
        process::exit(2);
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        Some("list") => list(arguments),
        Some("--help") | Some("-h") | None => {
            print_usage();
            Ok(())
        }
        Some(command) => Err(format!("unknown command {command:?}; expected list")),
    }
}

fn list(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if let Some(options) = parse_list_options(arguments)? {
        let mut sessions = discover_default();
        MetadataStore::open_default()?.hydrate_sessions(&mut sessions)?;
        print!("{}", format_sessions(&list_sessions(sessions, options)));
    }
    Ok(())
}

fn parse_list_options(
    arguments: impl Iterator<Item = String>,
) -> Result<Option<ListOptions>, String> {
    let mut options = ListOptions::default();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--agent" => {
                let value = arguments.next().ok_or("--agent requires a value")?;
                options.agent = Some(parse_agent(&value)?);
            }
            "--title" => {
                let value = arguments.next().ok_or("--title requires a value")?;
                if options.title.is_some() {
                    return Err("--title may only be specified once".to_owned());
                }
                options.title = Some(parse_title_filter(&value)?);
            }
            "--tag" => {
                let value = arguments.next().ok_or("--tag requires a value")?;
                options.tags.push(parse_tag(&value)?);
            }
            "--folder" => {
                let value = arguments.next().ok_or("--folder requires a value")?;
                if options.folder.is_some() {
                    return Err("--folder may only be specified once".to_owned());
                }
                options.folder = Some(parse_folder(&value)?);
            }
            "--sort" => {
                let value = arguments.next().ok_or("--sort requires a value")?;
                if value != "date" {
                    return Err(format!("unknown sort {value:?}; expected date"));
                }
            }
            "--limit" => {
                let value = arguments.next().ok_or("--limit requires a value")?;
                options.limit = Some(parse_limit(&value)?);
            }
            "--help" | "-h" => {
                print_list_usage();
                return Ok(None);
            }
            _ => return Err(format!("unknown list option {argument:?}")),
        }
    }
    Ok(Some(options))
}

fn format_sessions(sessions: &[aikeeper_domain::AgentSession]) -> String {
    if sessions.is_empty() {
        return "No Agent Sessions found.\n".to_owned();
    }

    let mut output = "AGENT\tFOLDER\tTITLE\tTAGS\tID\n".to_owned();
    for session in sessions {
        let folder = session
            .cwd
            .as_deref()
            .map(|path| path.display().to_string())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| "-".to_owned());
        let title = session.metadata.title.as_deref().unwrap_or("-");
        let tags = if session.metadata.tags.is_empty() {
            "-".to_owned()
        } else {
            session.metadata.tags.join(",")
        };
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            session.runtime, folder, title, tags, session.id
        ));
    }
    output
}

const LIST_USAGE: &str = "Usage: aikeeper list [--agent claude-code|pi] [--folder PATH] [--title TEXT] [--tag TAG]... [--sort date] [--limit POSITIVE_INTEGER]";

fn print_usage() {
    println!("{LIST_USAGE}");
}

fn print_list_usage() {
    print_usage();
}

#[cfg(test)]
mod tests {
    use aikeeper_domain::{AgentRuntime, AgentSession};

    use super::{LIST_USAGE, format_sessions, parse_list_options};

    #[test]
    fn formats_columns_and_empty_state() {
        assert_eq!(format_sessions(&[]), "No Agent Sessions found.\n");
        let session = AgentSession {
            runtime: AgentRuntime::Pi,
            id: "session-id".to_owned(),
            cwd: None,
            last_activity: None,
            metadata: Default::default(),
            discovery_order: 0,
        };
        assert_eq!(
            format_sessions(&[session]),
            "AGENT\tFOLDER\tTITLE\tTAGS\tID\npi\t-\t-\t-\tsession-id\n"
        );
    }

    #[test]
    fn parses_metadata_filters_and_rejects_repeated_title() {
        let options = parse_list_options(
            [
                "--title".to_owned(),
                "Résumé".to_owned(),
                "--tag".to_owned(),
                "one".to_owned(),
                "--tag".to_owned(),
                "two".to_owned(),
            ]
            .into_iter(),
        )
        .expect("valid options")
        .expect("not help");
        assert_eq!(options.title.as_deref(), Some("résumé"));
        assert_eq!(options.tags, ["one", "two"]);
        assert!(
            parse_list_options(
                [
                    "--title".to_owned(),
                    "one".to_owned(),
                    "--title".to_owned(),
                    "two".to_owned()
                ]
                .into_iter()
            )
            .is_err()
        );
    }

    #[test]
    fn parses_folder_once_and_advertises_it_in_help() {
        assert!(
            parse_list_options(["--folder".to_owned(), "projects/aikeeper".to_owned()].into_iter())
                .is_ok()
        );
        assert!(
            parse_list_options(
                [
                    "--folder".to_owned(),
                    "one".to_owned(),
                    "--folder".to_owned(),
                    "two".to_owned(),
                ]
                .into_iter(),
            )
            .is_err()
        );
        assert!(LIST_USAGE.contains("[--folder PATH]"));
    }
}
