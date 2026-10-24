use crate::config::{Config, Field};
use glob::{MatchOptions, Pattern};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshInfo {
    pub user: String,
    pub hostname: String,
}

impl SshInfo {
    pub fn display(&self) -> String {
        format!("{}@{}", self.user, self.hostname)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub alias: String,
    pub info: SshInfo,
    pub group: usize,
}

pub fn extract_aliases(text: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for line in text.lines() {
        let mut words = line.split_ascii_whitespace();
        if !words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("host"))
        {
            continue;
        }
        for alias in words.take_while(|word| !word.starts_with('#')) {
            if !alias.contains(['*', '?', '!']) {
                aliases.push(alias.to_owned());
            }
        }
    }
    aliases.sort_unstable();
    aliases.dedup();
    aliases
}

pub fn parse_ssh_output(text: &str) -> SshInfo {
    let mut info = SshInfo {
        user: String::new(),
        hostname: String::new(),
    };
    for line in text.lines() {
        let value = line.split_ascii_whitespace().nth(1).unwrap_or("");
        if line.starts_with("hostname ") {
            info.hostname = value.into();
        } else if line.starts_with("user ") {
            info.user = value.into();
        }
    }
    info
}

pub fn resolve(alias: &str) -> Result<SshInfo, String> {
    let output = Command::new("ssh")
        .args(["-G", "-T", alias])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|_| format!("Cannot resolve SSH configuration for {alias}"))?;
    if !output.status.success() {
        return Err(format!("Cannot resolve SSH configuration for {alias}"));
    }
    Ok(parse_ssh_output(&String::from_utf8_lossy(&output.stdout)))
}

fn matches(pattern: &Pattern, value: &str) -> bool {
    pattern.matches_with(
        value,
        MatchOptions {
            case_sensitive: false,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        },
    )
}

pub fn is_excluded(config: &Config, alias: &str) -> bool {
    config
        .exclusions
        .iter()
        .any(|pattern| matches(pattern, alias))
}

pub fn classify(config: &Config, alias: &str, hostname: &str) -> usize {
    config
        .rules
        .iter()
        .find(|rule| {
            (matches!(rule.field, Field::Alias | Field::Either) && matches(&rule.pattern, alias))
                || (matches!(rule.field, Field::Hostname | Field::Either)
                    && matches(&rule.pattern, hostname))
        })
        .map_or(config.labels.len() - 1, |rule| rule.group)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config;
    use std::path::Path;

    pub(crate) fn fixture() -> (Config, Vec<Host>) {
        let config = config::parse(
            Path::new("rules"),
            "group|Tailscale\ngroup|工作\ngroup|Personal\n\
            match|工作|hostname|192.0.2.*\nmatch|Tailscale|alias|*-TS\n\
            match|Personal|either|home-*\nexclude|github.*\n",
        )
        .unwrap();
        let inventory = [
            ("office-ts", "192.0.2.10"),
            ("cloud-ts", "198.51.100.1"),
            ("home-b", "203.0.113.1"),
            ("loose", "203.0.113.2"),
            ("work-a", "192.0.2.11"),
            ("work-b", "192.0.2.11"),
            ("github.example", "example.com"),
        ];
        let mut ssh_config = String::new();
        for (alias, hostname) in inventory {
            ssh_config.push_str(&format!(
                "Host {alias}\n HostName {hostname}\n User tester\n"
            ));
        }
        ssh_config.push_str("  hOsT work-a work-b *.example !blocked # comment\n");
        let aliases = extract_aliases(&ssh_config);
        assert_eq!(
            aliases,
            [
                "cloud-ts",
                "github.example",
                "home-b",
                "loose",
                "office-ts",
                "work-a",
                "work-b"
            ]
        );
        let hosts = aliases
            .into_iter()
            .filter(|alias| !is_excluded(&config, alias))
            .map(|alias| {
                let hostname = inventory.iter().find(|entry| entry.0 == alias).unwrap().1;
                let info = parse_ssh_output(&format!("hostname {hostname}\nuser tester\n"));
                let group = classify(&config, &alias, &info.hostname);
                Host { alias, info, group }
            })
            .collect();
        (config, hosts)
    }

    #[test]
    fn fixture_classification_respects_first_match_and_exclusion() {
        let (config, hosts) = fixture();
        assert_eq!(config.labels, ["Tailscale", "工作", "Personal", "Other"]);
        assert_eq!(
            hosts
                .iter()
                .map(|host| (host.alias.as_str(), host.group))
                .collect::<Vec<_>>(),
            [
                ("cloud-ts", 0),
                ("home-b", 2),
                ("loose", 3),
                ("office-ts", 1),
                ("work-a", 1),
                ("work-b", 1),
            ]
        );
        assert!(hosts.iter().all(|host| host.info.user == "tester"));
        assert_eq!(hosts[3].info.display(), "tester@192.0.2.10");
    }

    #[test]
    fn aliases_are_explicit_deduplicated_and_byte_sorted() {
        assert_eq!(
            extract_aliases(
                "Include other\nHost z A a [literal] a? b!c * x#y #stop ignored\n\
            hOsT z a é\nHostName not-an-alias\nHost=ignored\n"
            ),
            ["A", "[literal]", "a", "x#y", "z", "é"]
        );
    }

    #[test]
    fn ssh_output_accepts_crlf() {
        assert_eq!(
            parse_ssh_output("hostname 192.0.2.10\r\nuser tester\r\n").display(),
            "tester@192.0.2.10"
        );
    }

    #[test]
    fn ssh_output_uses_last_exact_lowercase_key_with_space() {
        assert_eq!(
            parse_ssh_output(
                "hostname first\nuser first\nHostName ignored\n hostname ignored\n\
            hostname\tignored\nuser\tignored\nhostname final extra\nuser tester extra\n"
            )
            .display(),
            "tester@final"
        );
        assert_eq!(parse_ssh_output("").display(), "@");
        assert_eq!(
            parse_ssh_output("hostname example.com\n").display(),
            "@example.com"
        );
    }

    #[test]
    fn glob_fields_are_case_insensitive_and_match_slashes_and_leading_dots() {
        let config = config::parse(Path::new("rules"), "group|A\ngroup|B\ngroup|C\n\
            match|A|alias|[^X]OST-?\nmatch|B|hostname|*.EXAMPLE\nmatch|C|either|foo*\nexclude|GITHUB.*").unwrap();
        assert_eq!(classify(&config, "HOST-a", "server.example"), 0);
        assert_eq!(classify(&config, "xost-a", ".EXAMPLE"), 1);
        assert_eq!(classify(&config, "unmatched", "foo/bar"), 2);
        assert_eq!(classify(&config, "FOObar", "unmatched"), 2);
        assert_eq!(classify(&config, "unmatched", "unmatched"), 3);
        assert!(is_excluded(&config, "GitHub.Example"));
        assert!(!is_excluded(&config, "example.com"));
    }
}
