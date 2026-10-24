#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

pub fn build_example(name: &str) -> PathBuf {
    let build = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--example", name, "--message-format=json"])
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
            (message["reason"] == "compiler-artifact" && message["target"]["name"] == name)
                .then(|| message["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .expect("Cargo must report the example executable")
}

fn ssh_stub() -> &'static Path {
    static STUB: OnceLock<PathBuf> = OnceLock::new();
    STUB.get_or_init(|| build_example("ssh_stub"))
}

pub struct Fixture {
    pub root: TempDir,
    pub home: PathBuf,
    pub appdata: PathBuf,
    pub xdg: PathBuf,
    bin: PathBuf,
    pub log: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
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

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sshto"));
        command.current_dir(self.root.path());
        for (key, value) in self.environment() {
            command.env(key, value);
        }
        command
            .env_remove("SSHTO_CONFIG")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    pub fn environment(&self) -> Vec<(&'static str, std::ffi::OsString)> {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut values = vec![
            ("HOME", self.home.clone().into_os_string()),
            ("USERPROFILE", self.home.clone().into_os_string()),
            ("APPDATA", self.appdata.clone().into_os_string()),
            ("XDG_CONFIG_HOME", self.xdg.clone().into_os_string()),
            ("PATH", std::env::join_paths(paths).unwrap()),
            ("SSHTO_TEST_LOG", self.log.clone().into_os_string()),
            ("SSHTO_TEST_EXIT", "0".into()),
            (
                "SSHTO_TEST_SSH_CONFIG",
                r#"{"demo":"hostname 192.0.2.10\r\nuser tester\r\n"}"#.into(),
            ),
        ];
        #[cfg(windows)]
        {
            values[0].1 = self.root.path().join("unused-unix-home").into_os_string();
        }
        #[cfg(unix)]
        {
            values[1].1 = self
                .root
                .path()
                .join("unused-windows-home")
                .into_os_string();
        }
        values
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    pub fn assert_error(&self, output: Output, message: &str) {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("{message}\n")
        );
        assert!(!self.log.exists(), "menu validation must not invoke SSH");
    }
}
