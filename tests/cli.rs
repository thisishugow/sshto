mod common;

use common::Fixture;
use std::fs;

#[test]
fn help_and_version_are_exact_and_do_not_invoke_ssh() {
    let fixture = Fixture::new();
    #[cfg(unix)]
    let paths = "Without arguments: open the menu using ~/.ssh/config.\n\
Groups: ${XDG_CONFIG_HOME:-~/.config}/sshto/config; override with SSHTO_CONFIG.\n";
    #[cfg(windows)]
    let paths = "Without arguments: open the menu using %USERPROFILE%\\.ssh\\config.\n\
Groups: %APPDATA%\\sshto\\config; override with SSHTO_CONFIG.\n";
    let help = format!(
        "Usage: sshto [SSH arguments...]\n{paths}\
Controls: 0 All, 1-9 groups, Tab/arrows to navigate, Enter/double-click to connect, q to quit.\n"
    );
    let version = format!("sshto {}\n", env!("CARGO_PKG_VERSION"));
    for (arg, expected) in [("--help", help.as_str()), ("--version", version.as_str())] {
        let output = fixture.command().arg(arg).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert_eq!(output.stderr, b"");
    }
    assert!(!fixture.log.exists());
}

#[test]
fn passthrough_preserves_arguments_and_exit_codes() {
    let fixture = Fixture::new();
    let cases = [
        (
            vec![
                "-p",
                "2222",
                "demo",
                "printf",
                "space value",
                ";",
                "$HOME",
                "literal;$HOME",
                "\"quoted\"",
                "'single'",
                "",
                "back\\slash\\",
            ],
            0,
        ),
        (vec!["--help", "demo"], 37),
        (vec!["--version", "demo"], 23),
    ];
    for (args, code) in &cases {
        let output = fixture
            .command()
            .args(args)
            .env("SSHTO_TEST_EXIT", code.to_string())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(*code), "{:?}", output.stderr);
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, b"");
    }
    assert_eq!(
        fixture.calls(),
        cases
            .iter()
            .map(|(args, _)| args.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn ssh_stub_answers_configuration_queries_with_crlf() {
    let fixture = Fixture::new();
    for args in [vec!["-G", "demo"], vec!["-G", "-T", "demo"]] {
        let output = fixture.command().args(&args).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hostname 192.0.2.10\r\nuser tester\r\n");
        assert_eq!(output.stderr, b"");
    }
    assert_eq!(
        fixture.calls(),
        [vec!["-G", "demo"], vec!["-G", "-T", "demo"]]
    );
}

#[test]
fn invalid_explicit_config_reports_path_line_and_message() {
    let fixture = Fixture::new();
    let config = fixture.root.path().join("custom config");
    fs::write(&config, "# fixture\ngroup|Work\ngroup|Work\n").unwrap();
    fixture.assert_error(
        fixture
            .command()
            .env("SSHTO_CONFIG", &config)
            .output()
            .unwrap(),
        &format!("{}:3: Duplicate group", config.display()),
    );
}

#[test]
fn missing_explicit_config_is_an_error() {
    let fixture = Fixture::new();
    let config = fixture.root.path().join("missing config");
    fixture.assert_error(
        fixture
            .command()
            .env("SSHTO_CONFIG", &config)
            .output()
            .unwrap(),
        &format!("{}:0: Cannot read configuration file", config.display()),
    );
}

#[test]
fn default_config_uses_the_platform_directory_and_explicit_config_overrides_it() {
    let fixture = Fixture::new();
    #[cfg(unix)]
    let base = &fixture.xdg;
    #[cfg(windows)]
    let base = &fixture.appdata;
    let config = base.join("sshto").join("config");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "unknown\n").unwrap();
    fixture.assert_error(
        fixture.command().output().unwrap(),
        &format!(
            "{}:1: Unknown directive; expected group, match or exclude",
            config.display()
        ),
    );

    let explicit = fixture.root.path().join("valid config");
    fs::write(&explicit, "group|Work\n").unwrap();
    fixture.assert_error(
        fixture
            .command()
            .env("SSHTO_CONFIG", explicit)
            .output()
            .unwrap(),
        "The menu requires a terminal; use sshto <Host> for a direct connection.",
    );
}

#[test]
fn redirected_streams_require_a_terminal() {
    let fixture = Fixture::new();
    fixture.assert_error(
        fixture.command().output().unwrap(),
        "The menu requires a terminal; use sshto <Host> for a direct connection.",
    );
}

#[test]
fn missing_or_unreadable_ssh_config_reports_the_home_path() {
    let fixture = Fixture::new();
    let config = fixture.home.join(".ssh").join("config");
    fs::remove_file(&config).unwrap();
    fixture.assert_error(
        fixture.command().output().unwrap(),
        &format!("Cannot read {}", config.display()),
    );
    // A directory cannot be read as a file, even when tests run as an administrator.
    fs::create_dir(&config).unwrap();
    fixture.assert_error(
        fixture.command().output().unwrap(),
        &format!("Cannot read {}", config.display()),
    );
}
