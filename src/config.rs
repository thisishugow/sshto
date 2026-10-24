use glob::Pattern;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Alias,
    Hostname,
    Either,
}

#[derive(Debug)]
pub struct Rule {
    pub group: usize,
    pub field: Field,
    pub pattern: Pattern,
}

#[derive(Debug)]
pub struct Config {
    pub labels: Vec<String>,
    pub rules: Vec<Rule>,
    pub exclusions: Vec<Pattern>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.message)
    }
}

impl std::error::Error for ConfigError {}

pub enum ConfigEnvironment<'a> {
    Unix {
        home: &'a Path,
        xdg_config_home: Option<&'a OsStr>,
    },
    Windows {
        appdata: &'a Path,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ConfigPath {
    pub path: PathBuf,
    pub explicit: bool,
}

pub fn resolve_path(
    sshto_config: Option<&OsStr>,
    environment: ConfigEnvironment<'_>,
) -> ConfigPath {
    if let Some(path) = sshto_config.filter(|value| !value.is_empty()) {
        return ConfigPath {
            path: path.into(),
            explicit: true,
        };
    }
    let base = match environment {
        ConfigEnvironment::Unix {
            home,
            xdg_config_home,
        } => xdg_config_home
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config")),
        ConfigEnvironment::Windows { appdata } => appdata.into(),
    };
    ConfigPath {
        path: base.join("sshto").join("config"),
        explicit: false,
    }
}

pub fn load(location: &ConfigPath) -> Result<Config, ConfigError> {
    // A dangling symlink is an unreadable file, not a missing default.
    if !location.explicit
        && fs::symlink_metadata(&location.path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return parse(&location.path, "");
    }
    let text = fs::read_to_string(&location.path).map_err(|_| ConfigError {
        path: location.path.clone(),
        line: 0,
        message: "Cannot read configuration file".into(),
    })?;
    parse(&location.path, &text)
}

fn compile_glob(pattern: &str) -> Result<Pattern, String> {
    let mut translated = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        translated.push(ch);
        if ch != '[' {
            continue;
        }
        if chars.peek() == Some(&'^') {
            chars.next();
            translated.push('!');
        } else if chars.peek() == Some(&'!') {
            translated.push(chars.next().unwrap());
        }
        // A closing bracket at the start of a class is a literal member.
        if chars.peek() == Some(&']') {
            translated.push(chars.next().unwrap());
        }
        for member in chars.by_ref() {
            translated.push(member);
            if member == ']' {
                break;
            }
        }
    }
    Pattern::new(&translated.to_ascii_lowercase()).map_err(|error| format!("Invalid glob: {error}"))
}

pub fn parse(path: &Path, text: &str) -> Result<Config, ConfigError> {
    let mut config = Config {
        labels: Vec::new(),
        rules: Vec::new(),
        exclusions: Vec::new(),
    };
    for (index, line) in text.split_terminator('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let error = |message: &str| ConfigError {
            path: path.into(),
            line: index + 1,
            message: message.into(),
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.chars().any(|ch| ch.is_ascii_control()) || line.ends_with('|') {
            return Err(error(
                "Control characters and trailing separators are not allowed",
            ));
        }
        let mut fields = line.splitn(5, '|');
        let kind = fields.next().unwrap_or("");
        let a = fields.next().unwrap_or("");
        let b = fields.next().unwrap_or("");
        let c = fields.next().unwrap_or("");
        let extra = fields.next().unwrap_or("");
        match kind {
            "group" => {
                if a.is_empty()
                    || !b.is_empty()
                    || !c.is_empty()
                    || !extra.is_empty()
                    || matches!(a, "All" | "Other")
                {
                    return Err(error("Expected group|name (All and Other are reserved)"));
                }
                if config.labels.iter().any(|label| label == a) {
                    return Err(error("Duplicate group"));
                }
                config.labels.push(a.into());
            }
            "match" => {
                if a.is_empty() || c.is_empty() || !extra.is_empty() {
                    return Err(error("Expected match|group|alias, hostname or either|glob"));
                }
                let field = match b {
                    "alias" => Field::Alias,
                    "hostname" => Field::Hostname,
                    "either" => Field::Either,
                    _ => return Err(error("Match field must be alias, hostname or either")),
                };
                let group = config
                    .labels
                    .iter()
                    .position(|label| label == a)
                    .ok_or_else(|| {
                        error("Declare a group before referencing it in a match rule")
                    })?;
                config.rules.push(Rule {
                    group,
                    field,
                    pattern: compile_glob(c).map_err(|message| error(&message))?,
                });
            }
            "exclude" => {
                if a.is_empty() || !b.is_empty() || !c.is_empty() || !extra.is_empty() {
                    return Err(error("Expected exclude|Host alias glob"));
                }
                config
                    .exclusions
                    .push(compile_glob(a).map_err(|message| error(&message))?);
            }
            _ => return Err(error("Unknown directive; expected group, match or exclude")),
        }
    }
    config.labels.push("Other".into());
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_config_messages_and_line_numbers() {
        for (text, line, message) in [
            (
                "group|All\n",
                1,
                "Expected group|name (All and Other are reserved)",
            ),
            (
                "group|Other\n",
                1,
                "Expected group|name (All and Other are reserved)",
            ),
            ("group|X\ngroup|X\n", 2, "Duplicate group"),
            (
                "match|Missing|alias|*\n",
                1,
                "Declare a group before referencing it in a match rule",
            ),
            (
                "group|X\nmatch|X|unknown|*\n",
                2,
                "Match field must be alias, hostname or either",
            ),
            (
                "group|X|bad\n",
                1,
                "Expected group|name (All and Other are reserved)",
            ),
            (
                "group|X\x1b\n",
                1,
                "Control characters and trailing separators are not allowed",
            ),
            (
                "group|X|\n",
                1,
                "Control characters and trailing separators are not allowed",
            ),
            (
                "group|\tX\n",
                1,
                "Control characters and trailing separators are not allowed",
            ),
            (
                "group",
                1,
                "Expected group|name (All and Other are reserved)",
            ),
            (
                "match|X|alias",
                1,
                "Expected match|group|alias, hostname or either|glob",
            ),
            (
                "match||alias|*",
                1,
                "Expected match|group|alias, hostname or either|glob",
            ),
            (
                "group|X\nmatch|X|alias|*|extra",
                2,
                "Expected match|group|alias, hostname or either|glob",
            ),
            ("exclude", 1, "Expected exclude|Host alias glob"),
            ("exclude|x|extra", 1, "Expected exclude|Host alias glob"),
            (
                " group|X",
                1,
                "Unknown directive; expected group, match or exclude",
            ),
            (
                "# comment\n\nunknown",
                3,
                "Unknown directive; expected group, match or exclude",
            ),
        ] {
            let error = parse(Path::new("rules"), text).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("rules:{line}: {message}"),
                "{text:?}"
            );
        }
    }

    #[test]
    fn names_are_data_and_whitespace_is_preserved() {
        let directory = std::env::temp_dir().join(format!("sshto-data-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let marker = directory.join("executed");
        let name = format!("$(touch {})", marker.display());
        let config = parse(
            Path::new("rules"),
            &format!("# ignored\x1b\r\n\r\ngroup|{name}\r\nmatch|{name}|alias|*\r\ngroup| spaced "),
        )
        .unwrap();
        assert_eq!(config.labels, [name, " spaced ".into(), "Other".into()]);
        assert_eq!(config.rules[0].group, 0);
        assert!(!marker.exists());
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn negated_brackets_and_invalid_globs() {
        assert_eq!(compile_glob("[^A]*").unwrap().as_str(), "[!a]*");
        assert_eq!(compile_glob("[!A]*").unwrap().as_str(), "[!a]*");
        assert_eq!(compile_glob("[[^]x").unwrap().as_str(), "[[^]x");
        for text in ["exclude|[", "group|X\nmatch|X|alias|["] {
            let error = parse(Path::new("rules"), text).unwrap_err();
            assert_eq!(error.line, text.split('\n').count());
            assert_eq!(
                error.message,
                format!("Invalid glob: {}", Pattern::new("[").unwrap_err())
            );
        }
    }

    #[test]
    fn config_paths_use_injected_environment() {
        let home = Path::new("home");
        for explicit in [None, Some(OsStr::new(""))] {
            for xdg in [None, Some(OsStr::new("")), Some(OsStr::new("xdg"))] {
                let result = resolve_path(
                    explicit,
                    ConfigEnvironment::Unix {
                        home,
                        xdg_config_home: xdg,
                    },
                );
                let base = if xdg == Some(OsStr::new("xdg")) {
                    PathBuf::from("xdg")
                } else {
                    home.join(".config")
                };
                assert_eq!(
                    result,
                    ConfigPath {
                        path: base.join("sshto/config"),
                        explicit: false
                    }
                );
            }
            let result = resolve_path(
                explicit,
                ConfigEnvironment::Windows {
                    appdata: Path::new("AppData/Roaming"),
                },
            );
            assert_eq!(
                result,
                ConfigPath {
                    path: PathBuf::from("AppData/Roaming/sshto/config"),
                    explicit: false
                }
            );
        }
        for environment in [
            ConfigEnvironment::Unix {
                home,
                xdg_config_home: Some(OsStr::new("xdg")),
            },
            ConfigEnvironment::Windows {
                appdata: Path::new("AppData/Roaming"),
            },
        ] {
            assert_eq!(
                resolve_path(Some(OsStr::new("custom")), environment),
                ConfigPath {
                    path: "custom".into(),
                    explicit: true
                }
            );
        }
    }

    #[test]
    fn missing_default_is_empty_but_explicit_or_dangling_file_is_an_error() {
        let directory = std::env::temp_dir().join(format!("sshto-load-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let mut location = ConfigPath {
            path: directory.join("config"),
            explicit: false,
        };
        assert_eq!(load(&location).unwrap().labels, ["Other"]);
        location.explicit = true;
        assert_eq!(
            load(&location).unwrap_err().to_string(),
            format!(
                "{}:0: Cannot read configuration file",
                location.path.display()
            )
        );
        fs::write(&location.path, "group|X").unwrap();
        assert_eq!(load(&location).unwrap().labels, ["X", "Other"]);
        fs::remove_file(&location.path).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.join("missing"), &location.path).unwrap();
            location.explicit = false;
            assert_eq!(
                load(&location).unwrap_err().message,
                "Cannot read configuration file"
            );
            fs::remove_file(&location.path).unwrap();
        }
        location.path = directory.clone();
        assert_eq!(
            load(&location).unwrap_err().message,
            "Cannot read configuration file"
        );
        fs::remove_dir(directory).unwrap();
    }
}
