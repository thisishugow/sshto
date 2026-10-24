use std::ffi::OsString;

#[cfg(unix)]
pub const HELP: &str = "Usage: sshto [SSH arguments...]\n\
Without arguments: open the menu using ~/.ssh/config.\n\
Groups: ${XDG_CONFIG_HOME:-~/.config}/sshto/config; override with SSHTO_CONFIG.\n\
Controls: 0 All, 1-9 groups, Tab/arrows to navigate, Enter/double-click to connect, q to quit.\n";

#[cfg(windows)]
pub const HELP: &str = "Usage: sshto [SSH arguments...]\n\
Without arguments: open the menu using %USERPROFILE%\\.ssh\\config.\n\
Groups: %APPDATA%\\sshto\\config; override with SSHTO_CONFIG.\n\
Controls: 0 All, 1-9 groups, Tab/arrows to navigate, Enter/double-click to connect, q to quit.\n";

#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    Help,
    Version,
    Passthrough(Vec<OsString>),
    Menu,
}

pub fn parse(args: Vec<OsString>) -> Invocation {
    if args.is_empty() {
        Invocation::Menu
    } else if args.len() == 1 && args[0] == "--help" {
        Invocation::Help
    } else if args.len() == 1 && args[0] == "--version" {
        Invocation::Version
    } else {
        Invocation::Passthrough(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_local_invocations_only() {
        assert_eq!(parse(vec![]), Invocation::Menu);
        assert_eq!(parse(vec!["--help".into()]), Invocation::Help);
        assert_eq!(parse(vec!["--version".into()]), Invocation::Version);
        for args in [
            vec!["--help", "host"],
            vec!["--version", "--help"],
            vec![""],
            vec!["-h"],
            vec![
                "-p",
                "2222",
                "my-host",
                "printf",
                "space value",
                "literal;$HOME",
            ],
        ] {
            let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
            assert_eq!(parse(args.clone()), Invocation::Passthrough(args));
        }
    }

    #[cfg(unix)]
    #[test]
    fn passthrough_preserves_non_utf8_arguments() {
        use std::os::unix::ffi::OsStringExt;
        let args = vec![OsString::from_vec(vec![0xff, b'a'])];
        assert_eq!(parse(args.clone()), Invocation::Passthrough(args));
    }
}
