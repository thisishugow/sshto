use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("SSHTO_TEST_LOG").expect("SSHTO_TEST_LOG must be set"))
        .unwrap();
    serde_json::to_writer(&mut log, &args).unwrap();
    writeln!(log).unwrap();

    if args.first().is_some_and(|arg| arg == "-G") {
        let fixtures: HashMap<String, String> = serde_json::from_str(
            &std::env::var("SSHTO_TEST_SSH_CONFIG").expect("SSHTO_TEST_SSH_CONFIG must be set"),
        )
        .unwrap();
        let output = fixtures
            .get(args.last().expect("-G requires an alias"))
            .expect("alias must exist in the fixture");
        io::stdout().write_all(output.as_bytes()).unwrap();
    }
    std::process::exit(
        std::env::var("SSHTO_TEST_EXIT")
            .unwrap_or_else(|_| "0".into())
            .parse()
            .unwrap(),
    );
}
