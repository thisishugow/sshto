use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

fn ssh_stub() -> &'static Path {
    static STUB: OnceLock<PathBuf> = OnceLock::new();
    STUB.get_or_init(|| {
        let build = Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["build", "--example", "ssh_stub", "--message-format=json"])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        String::from_utf8(build.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|message| {
                (message["reason"] == "compiler-artifact"
                    && message["target"]["name"] == "ssh_stub")
                    .then(|| message["executable"].as_str().map(PathBuf::from))
                    .flatten()
            })
            .expect("Cargo must report the ssh_stub executable")
    })
}

struct Fixture {
    root: TempDir,
    home: PathBuf,
    appdata: PathBuf,
    xdg: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("sshto test ")
            .tempdir()
            .unwrap();
        let home = root.path().join("home");
        let appdata = root.path().join("AppData").join("Roaming");
        let xdg = root.path().join("xdg");
        let bin = root.path().join("bin");
        let log = root.path().join("ssh.jsonl");
        for directory in [&home.join(".ssh"), &appdata, &xdg, &bin] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            home.join(".ssh").join("config"),
            "Host demo\n HostName 192.0.2.10\n User tester\n",
        )
        .unwrap();
        fs::copy(
            ssh_stub(),
            bin.join(format!("ssh{}", std::env::consts::EXE_SUFFIX)),
        )
        .unwrap();
        Self {
            root,
            home,
            appdata,
            xdg,
            bin,
            log,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sshto"));
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .current_dir(self.root.path())
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("APPDATA", &self.appdata)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .env_remove("SSHTO_CONFIG")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("SSHTO_TEST_LOG", &self.log)
            .env("SSHTO_TEST_EXIT", "0")
            .env(
                "SSHTO_TEST_SSH_CONFIG",
                r#"{"demo":"hostname 192.0.2.10\r\nuser tester\r\n"}"#,
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.env("HOME", self.root.path().join("unused-unix-home"));
        #[cfg(unix)]
        command.env("USERPROFILE", self.root.path().join("unused-windows-home"));
        command
    }

    fn calls(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn assert_error(&self, output: Output, message: &str) {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("{message}\n")
        );
        assert!(!self.log.exists(), "menu validation must not invoke SSH");
    }
}

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
    for (arg, expected) in [("--help", help.as_str()), ("--version", "sshto 0.1.0\n")] {
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
