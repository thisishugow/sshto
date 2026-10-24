pub mod cli;
pub mod config;
pub mod hosts;
pub mod menu;
pub mod ssh;
pub mod ui;

use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

fn connect(args: &[OsString]) -> i32 {
    match ssh::run(args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ssh: {error}");
            match error.kind() {
                io::ErrorKind::NotFound => 127,
                _ => 126,
            }
        }
    }
}

fn run_menu() -> Result<i32, String> {
    #[cfg(unix)]
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    #[cfg(windows)]
    let home = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default());
    let ssh_config = home.join(".ssh").join("config");
    let text = std::fs::read_to_string(&ssh_config)
        .map_err(|_| format!("Cannot read {}", ssh_config.display()))?;

    let explicit = std::env::var_os("SSHTO_CONFIG");
    #[cfg(unix)]
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME");
    #[cfg(unix)]
    let environment = config::ConfigEnvironment::Unix {
        home: &home,
        xdg_config_home: xdg_config_home.as_deref(),
    };
    #[cfg(windows)]
    let appdata = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default());
    #[cfg(windows)]
    let environment = config::ConfigEnvironment::Windows { appdata: &appdata };
    let location = config::resolve_path(explicit.as_deref(), environment);
    let config = config::load(&location).map_err(|error| error.to_string())?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "The menu requires a terminal; use sshto <Host> for a direct connection.".into(),
        );
    }
    let mut hosts = Vec::new();
    for alias in hosts::extract_aliases(&text) {
        if hosts::is_excluded(&config, &alias) {
            continue;
        }
        let info = hosts::resolve(&alias)?;
        let group = hosts::classify(&config, &alias, &info.hostname);
        hosts.push(hosts::Host { alias, info, group });
    }
    if hosts.is_empty() {
        return Err(format!(
            "No explicit Host entries found in {}",
            ssh_config.display()
        ));
    }

    let mut menu = menu::Menu::new(config.labels, hosts);
    match ui::run(&mut menu).map_err(|error| error.to_string())? {
        menu::Action::Connect(index) => {
            let alias = &menu.hosts()[index].alias;
            println!("Connecting to {alias} ...");
            io::stdout().flush().map_err(|error| error.to_string())?;
            Ok(connect(&[alias.into()]))
        }
        menu::Action::Exit(code) => Ok(i32::from(code)),
        menu::Action::None => Ok(0),
    }
}

fn run() -> i32 {
    match cli::parse(std::env::args_os().skip(1).collect()) {
        cli::Invocation::Help => print!("{}", cli::HELP),
        cli::Invocation::Version => println!("sshto {}", env!("CARGO_PKG_VERSION")),
        cli::Invocation::Passthrough(args) => {
            return connect(&args);
        }
        cli::Invocation::Menu => {
            return match run_menu() {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            };
        }
    }
    0
}

fn main() {
    // The UI is restored before run returns; preserve the full Windows child exit code.
    std::process::exit(run());
}
