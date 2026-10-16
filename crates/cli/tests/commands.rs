use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn test_home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "aikeeper-cli-commands-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&home).expect("create home");
    home
}

fn command(home: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aikeeper"));
    command
        .args(arguments)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"));
    command
}

#[test]
fn exposes_help_for_metadata_commands() {
    let home = test_home();
    for arguments in [
        ["title", "--help"].as_slice(),
        ["title", "set", "--help"].as_slice(),
        ["tag", "--help"].as_slice(),
        ["tag", "add", "--help"].as_slice(),
    ] {
        let output = command(&home, arguments)
            .output()
            .expect("run help command");
        assert!(output.status.success(), "{:?}", output);
        assert!(
            String::from_utf8(output.stdout)
                .expect("UTF-8 output")
                .contains("Usage: aikeeper")
        );
    }
    fs::remove_dir_all(home).expect("remove home");
}

#[test]
fn manages_discovered_session_titles_and_tags() {
    let home = test_home();
    let session = home.join(".pi/agent/sessions/work/session.jsonl");
    fs::create_dir_all(session.parent().expect("session parent")).expect("create session parent");
    fs::write(
        session,
        r#"{"type":"session","id":"pi-id","cwd":"/workspace"}"#,
    )
    .expect("write session");

    for arguments in [
        [
            "title",
            "set",
            "--agent",
            "pi",
            "--id",
            "pi-id",
            "--",
            "-Planning",
        ]
        .as_slice(),
        ["tag", "add", "--agent", "pi", "--id", "pi-id", "  Work  "].as_slice(),
        ["tag", "add", "--agent", "pi", "--id", "pi-id", "Zebra"].as_slice(),
        ["tag", "add", "--agent", "pi", "--id", "pi-id", "work"].as_slice(),
    ] {
        let output = command(&home, arguments).output().expect("run command");
        assert!(output.status.success(), "{:?}", output);
    }

    let output = command(&home, &["list"]).output().expect("list sessions");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        "AGENT\tFOLDER\tTITLE\tTAGS\tID\npi\t/workspace\t-Planning\twork,zebra\tpi-id\n"
    );

    for arguments in [
        ["tag", "remove", "--agent", "pi", "--id", "pi-id", "WORK"].as_slice(),
        ["title", "clear", "--agent", "pi", "--id", "pi-id"].as_slice(),
        ["tag", "remove", "--agent", "pi", "--id", "pi-id", "work"].as_slice(),
    ] {
        let output = command(&home, arguments).output().expect("run command");
        assert!(output.status.success(), "{:?}", output);
    }

    let output = command(&home, &["list"])
        .output()
        .expect("list sessions after removal");
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .expect("UTF-8 output")
            .contains("pi\t/workspace\t-\tzebra\tpi-id")
    );

    let output = command(
        &home,
        &["title", "set", "--agent", "pi", "--id", "missing", "title"],
    )
    .output()
    .expect("run unavailable-session command");
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .expect("UTF-8 error")
            .contains("not currently discoverable")
    );

    fs::remove_dir_all(home).expect("remove home");
}
