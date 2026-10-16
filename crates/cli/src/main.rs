use std::{env, process};

use aikeeper_app::{ListOptions, list_sessions, parse_agent, parse_limit};
use aikeeper_store::discover_default;

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
    let mut options = ListOptions::default();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--agent" => {
                let value = arguments.next().ok_or("--agent requires a value")?;
                options.agent = Some(parse_agent(&value)?);
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
                return Ok(());
            }
            _ => return Err(format!("unknown list option {argument:?}")),
        }
    }

    print!(
        "{}",
        format_sessions(&list_sessions(discover_default(), options))
    );
    Ok(())
}

fn format_sessions(sessions: &[aikeeper_domain::AgentSession]) -> String {
    if sessions.is_empty() {
        return "No Agent Sessions found.\n".to_owned();
    }

    let mut output = "AGENT\tFOLDER\tID\n".to_owned();
    for session in sessions {
        let folder = session
            .cwd
            .as_deref()
            .map(|path| path.display().to_string())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| "-".to_owned());
        output.push_str(&format!(
            "{}\t{}\t{}\n",
            session.runtime, folder, session.id
        ));
    }
    output
}

fn print_usage() {
    println!(
        "Usage: aikeeper list [--agent claude-code|pi] [--sort date] [--limit POSITIVE_INTEGER]"
    );
}

fn print_list_usage() {
    print_usage();
}

#[cfg(test)]
mod tests {
    use aikeeper_domain::{AgentRuntime, AgentSession};

    use super::format_sessions;

    #[test]
    fn formats_columns_and_empty_state() {
        assert_eq!(format_sessions(&[]), "No Agent Sessions found.\n");
        let session = AgentSession {
            runtime: AgentRuntime::Pi,
            id: "session-id".to_owned(),
            cwd: None,
            last_activity: None,
            discovery_order: 0,
        };
        assert_eq!(
            format_sessions(&[session]),
            "AGENT\tFOLDER\tID\npi\t-\tsession-id\n"
        );
    }
}
