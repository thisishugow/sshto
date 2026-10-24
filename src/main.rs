pub mod cli;
pub mod config;
pub mod hosts;
pub mod menu;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match cli::parse(std::env::args_os().skip(1).collect()) {
        cli::Invocation::Help => print!("{}", cli::HELP),
        cli::Invocation::Version => println!("sshto 0.1.0"),
        cli::Invocation::Passthrough(args) => {
            return match Command::new("ssh").args(args).status() {
                Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
                Err(error) => {
                    eprintln!("ssh: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        cli::Invocation::Menu => {
            eprintln!("The Rust menu is not implemented yet.");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
