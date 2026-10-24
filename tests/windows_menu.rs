#![cfg(windows)]

mod common;
#[path = "common/win_scenarios.rs"]
mod scenarios;

use common::Fixture;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use scenarios::{Input, Key, Terminal, COLS, ROWS, TIMEOUT};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

fn fixture() -> (Fixture, String) {
    let fixture = Fixture::new();
    fs::write(
        fixture.home.join(".ssh/config"),
        "Host loose home-b work-b home-a work-a\n HostName 192.0.2.10\n User tester\n",
    )
    .unwrap();
    let directory = fixture.appdata.join("sshto");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("config"),
        "group|Work\ngroup|Home\nmatch|Work|alias|work-*\nmatch|Home|alias|home-*\n",
    )
    .unwrap();
    let config: serde_json::Map<_, _> = ["home-a", "home-b", "loose", "work-a", "work-b"]
        .into_iter()
        .enumerate()
        .map(|(index, host)| {
            (
                host.to_owned(),
                serde_json::Value::String(format!(
                    "hostname 192.0.2.{}\r\nuser tester\r\n",
                    index + 10
                )),
            )
        })
        .collect();
    (fixture, serde_json::to_string(&config).unwrap())
}

fn conhost(shell: &str, scenario: &str) {
    static DRIVER: OnceLock<PathBuf> = OnceLock::new();
    let driver = DRIVER.get_or_init(|| common::build_example("win_console_driver"));
    let (fixture, config) = fixture();
    let stdout = fixture.root.path().join("driver-stdout.txt");
    let stderr = fixture.root.path().join("driver-stderr.txt");
    let mut command = Command::new(driver);
    for (key, value) in fixture.environment() {
        command.env(key, value);
    }
    let mut child = command
        .env_remove("SSHTO_CONFIG")
        .env("SSHTO_TEST_SSH_CONFIG", config)
        .current_dir(fixture.root.path())
        .arg("drive")
        .arg(shell)
        .arg(scenario)
        .arg(env!("CARGO_BIN_EXE_sshto"))
        .arg(fixture.root.path())
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + TIMEOUT * 5;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            // Killing the driver closes its job handle and terminates every descendant.
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "{shell}/{scenario}: driver timed out\n{}",
                fs::read_to_string(stderr).unwrap()
            );
        }
        scenarios::pause();
    };
    assert!(
        status.success(),
        "{shell}/{scenario}: {status}\nstdout:\n{}\nstderr:\n{}\nhost:\n{}",
        fs::read_to_string(stdout).unwrap(),
        fs::read_to_string(stderr).unwrap(),
        fs::read_to_string(fixture.root.path().join("host-errors.txt")).unwrap_or_default()
    );
}

#[derive(Default)]
struct Replies(Vec<u8>);

impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        command: char,
    ) {
        if i1.is_none() && i2.is_none() && params == [&[6]] && command == 'n' {
            let (row, col) = screen.cursor_position();
            self.0
                .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
        }
    }
}

#[test]
fn cursor_queries_are_answered_across_output_chunks() {
    let mut parser = vt100::Parser::new_with_callbacks(ROWS, COLS, 0, Replies::default());
    for chunk in [b"\x1b[4;9H\x1b[".as_slice(), b"6"] {
        parser.process(chunk);
        assert!(parser.callbacks().0.is_empty());
    }
    parser.process(b"n");
    assert_eq!(std::mem::take(&mut parser.callbacks_mut().0), b"\x1b[4;9R");
    parser.process(b"\x1b[H\x1b[6n\x1b[6n");
    assert_eq!(parser.callbacks().0, b"\x1b[1;1R\x1b[1;1R");
    assert_eq!(parser.screen().contents(), "");
}

struct Output {
    parser: vt100::Parser<Replies>,
    transcript: Vec<u8>,
    error: Option<io::Error>,
}

struct ConPty {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Option<Box<dyn MasterPty + Send>>,
    reader: Option<thread::JoinHandle<()>>,
    output: Arc<Mutex<Output>>,
}

impl ConPty {
    fn new(fixture: &Fixture, config: &str) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let output = Arc::new(Mutex::new(Output {
            parser: vt100::Parser::new_with_callbacks(ROWS, COLS, 0, Replies::default()),
            transcript: Vec::new(),
            error: None,
        }));
        let reader_output = output.clone();
        let reply_writer = writer.clone();
        // portable-pty requests cursor inheritance. Service DSR on this thread even
        // during CreateProcess/ClosePseudoConsole, which can block awaiting the reply.
        let reader = thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(length) => {
                        let replies = {
                            let mut output = reader_output.lock().unwrap();
                            output.parser.process(&buffer[..length]);
                            output.transcript.extend_from_slice(&buffer[..length]);
                            std::mem::take(&mut output.parser.callbacks_mut().0)
                        };
                        if !replies.is_empty() {
                            let mut writer = reply_writer.lock().unwrap();
                            if let Err(error) =
                                writer.write_all(&replies).and_then(|()| writer.flush())
                            {
                                reader_output.lock().unwrap().error = Some(error);
                            }
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::BrokenPipe => break,
                    Err(error) => {
                        reader_output.lock().unwrap().error = Some(error);
                        break;
                    }
                }
            }
        });
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sshto"));
        command.cwd(fixture.root.path());
        command.env_remove("SSHTO_CONFIG");
        for (key, value) in fixture.environment() {
            command.env(key, value);
        }
        command.env("SSHTO_TEST_SSH_CONFIG", config);
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        Self {
            child,
            writer,
            master: Some(pair.master),
            reader: Some(reader),
            output,
        }
    }
}

impl Terminal for ConPty {
    fn send(&mut self, input: Input) {
        let text = match input {
            Input::Key(key) => match key {
                Key::Down => "\x1b[B".into(),
                Key::Up => "\x1b[A".into(),
                Key::Right => "\x1b[C".into(),
                Key::Left => "\x1b[D".into(),
                Key::Tab => "\t".into(),
                Key::Digit(digit) => char::from(b'0' + digit).to_string(),
                Key::Enter => "\r".into(),
                Key::Quit => "q".into(),
                Key::Escape => "\x1b".into(),
                Key::Interrupt => "\x03".into(),
            },
            Input::Click { x, y, double } => {
                let click = format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m");
                if double {
                    click.repeat(2)
                } else {
                    click
                }
            }
            Input::Wheel { down } => format!("\x1b[<{};8;5M", if down { 65 } else { 64 }),
        };
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(text.as_bytes()).unwrap();
        writer.flush().unwrap();
    }

    fn screen(&mut self) -> String {
        let (screen, error) = {
            let mut output = self.output.lock().unwrap();
            (output.parser.screen().contents(), output.error.take())
        };
        assert!(error.is_none(), "ConPTY I/O failed: {error:?}");
        screen
    }

    fn exited(&mut self) -> Option<u32> {
        self.child
            .try_wait()
            .unwrap()
            .map(|status| status.exit_code())
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!(
                "ConPTY VT transcript: {:?}",
                String::from_utf8_lossy(&self.output.lock().unwrap().transcript)
            );
        }
        // Keep draining on the reader thread while ClosePseudoConsole flushes output.
        let _ = self.child.kill();
        let _ = self.child.wait();
        drop(self.master.take());
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn conpty(scenario: &str) {
    assert!(scenarios::SCENARIOS.contains(&scenario));
    let (fixture, config) = fixture();
    let mut terminal = ConPty::new(&fixture, &config);
    scenarios::run(&mut terminal, scenario, &fixture.log);
}

macro_rules! scenario {
    ($name:ident, $scenario:literal) => {
        mod $name {
            #[test]
            fn cmd() {
                super::conhost("cmd", $scenario);
            }
            #[test]
            fn powershell() {
                super::conhost("powershell", $scenario);
            }
            #[test]
            fn conpty() {
                super::conpty($scenario);
            }
        }
    };
}

scenario!(appearance_and_quit, "appearance");
scenario!(down_once, "down");
scenario!(up_once, "up");
scenario!(right_once, "right");
scenario!(left_once, "left");
scenario!(tab_once, "tab");
scenario!(numeric_shortcut, "shortcut");
scenario!(enter_connects, "enter");
scenario!(escape_exits_zero, "escape");
scenario!(ctrl_c_exits_130, "interrupt");
scenario!(click_tab, "mouse-tab");
scenario!(single_click_only_selects, "mouse-single");
scenario!(double_click_connects, "mouse-double");
scenario!(heading_does_not_connect, "mouse-heading");
scenario!(wheel_down_once, "wheel-down");
scenario!(wheel_up_once, "wheel-up");
