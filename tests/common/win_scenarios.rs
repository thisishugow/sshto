//! Shared behavioral assertions for real conhost records and ConPTY VT input.
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

pub const COLS: u16 = 120;
pub const ROWS: u16 = 30;
pub const TIMEOUT: Duration = Duration::from_secs(20);
pub const SCENARIOS: &[&str] = &[
    "appearance",
    "down",
    "up",
    "right",
    "left",
    "tab",
    "shortcut",
    "enter",
    "escape",
    "interrupt",
    "mouse-tab",
    "mouse-single",
    "mouse-double",
    "mouse-double-motion",
    "mouse-heading",
    "wheel-down",
    "wheel-up",
];
const ALL: &[&str] = &["work-a", "work-b", "home-a", "home-b", "loose"];

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Down,
    Up,
    Right,
    Left,
    Tab,
    Digit(u8),
    Enter,
    Quit,
    Escape,
    Interrupt,
}

#[derive(Clone, Copy, Debug)]
pub enum Input {
    Key(Key),
    Click { x: u16, y: u16, double: bool },
    DoubleClickWithMotion { x: u16, y: u16 },
    Wheel { down: bool },
}

pub trait Terminal {
    fn send(&mut self, input: Input);
    fn screen(&mut self) -> String;
    fn exited(&mut self) -> Option<u32>;
}

pub fn pause() {
    thread::sleep(Duration::from_millis(10));
}

pub fn wait_menu(terminal: &mut impl Terminal, tab: usize, selected: usize) -> String {
    let visible = match tab {
        0 => ALL,
        1 => &ALL[..2],
        2 => &ALL[2..4],
        3 => &ALL[4..],
        _ => unreachable!(),
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let screen = terminal.screen();
        let hosts: Vec<_> = screen
            .lines()
            .filter_map(|line| line.split_whitespace().find(|word| ALL.contains(word)))
            .collect();
        let selection: Vec<_> = screen
            .lines()
            .filter(|line| line.contains('>'))
            .filter_map(|line| line.split_whitespace().find(|word| ALL.contains(word)))
            .collect();
        let headings: Vec<_> = screen
            .lines()
            .map(str::trim)
            .filter(|line| matches!(*line, "Work (2)" | "Home (2)" | "Other (1)"))
            .collect();
        let expected_headings: &[&str] = if tab == 0 {
            &["Work (2)", "Home (2)", "Other (1)"]
        } else {
            &[]
        };
        let tabs = "0:All(5)  1:Work(2)  2:Home(2)  3:Other(1)";
        if screen
            .lines()
            .next()
            .is_some_and(|line| line.trim() == tabs)
            && hosts == visible
            && selection == [visible[selected]]
            && headings == expected_headings
            && screen.contains(&format!(
                "sshto  {}/{}  Click: select  Double-click: connect  Wheel: move",
                selected + 1,
                visible.len()
            ))
        {
            return screen;
        }
        assert!(
            terminal.exited().is_none(),
            "process exited before tab {tab}, selection {selected}; screen:\n{screen}"
        );
        assert!(
            Instant::now() < deadline,
            "timed out waiting for tab {tab}, selection {selected}; screen:\n{screen}"
        );
        pause();
    }
}

fn key(terminal: &mut impl Terminal, key: Key, tab: usize, selected: usize) {
    terminal.send(Input::Key(key));
    wait_menu(terminal, tab, selected);
}

pub fn run(terminal: &mut impl Terminal, scenario: &str, log: &Path) {
    let initial = wait_menu(terminal, 0, 0);
    let mut alias = Some("work-a");
    let mut exit_key = Key::Enter;
    let mut exit_code = 0;
    match scenario {
        "appearance" => {
            alias = None;
            exit_key = Key::Quit;
        }
        "down" => {
            key(terminal, Key::Down, 0, 1);
            alias = Some("work-b");
        }
        "up" => {
            key(terminal, Key::Down, 0, 1);
            key(terminal, Key::Down, 0, 2);
            key(terminal, Key::Up, 0, 1);
            alias = Some("work-b");
        }
        "right" => {
            key(terminal, Key::Right, 1, 0);
        }
        "left" => {
            key(terminal, Key::Digit(3), 3, 0);
            key(terminal, Key::Left, 2, 0);
            alias = Some("home-a");
        }
        "tab" => {
            key(terminal, Key::Digit(1), 1, 0);
            key(terminal, Key::Tab, 2, 0);
            alias = Some("home-a");
        }
        "shortcut" => {
            key(terminal, Key::Digit(2), 2, 0);
            alias = Some("home-a");
        }
        "enter" => {}
        "escape" => {
            alias = None;
            exit_key = Key::Escape;
        }
        "interrupt" => {
            alias = None;
            exit_key = Key::Interrupt;
            exit_code = 130;
        }
        "mouse-tab" => {
            let x = initial.lines().next().unwrap().find("2:Home").unwrap() as u16 + 1;
            terminal.send(Input::Click {
                x,
                y: 1,
                double: false,
            });
            wait_menu(terminal, 2, 0);
            alias = Some("home-a");
        }
        "mouse-single" => {
            terminal.send(Input::Click {
                x: 8,
                y: 5,
                double: false,
            });
            wait_menu(terminal, 0, 1);
            // A subsequent observable action proves that the click did not connect.
            key(terminal, Key::Down, 0, 2);
            alias = Some("home-a");
        }
        "mouse-double" => {
            terminal.send(Input::Click {
                x: 8,
                y: 5,
                double: true,
            });
            alias = Some("work-b");
        }
        "mouse-double-motion" => {
            terminal.send(Input::DoubleClickWithMotion { x: 8, y: 5 });
            alias = Some("work-b");
        }
        "mouse-heading" => {
            terminal.send(Input::Click {
                x: 5,
                y: 3,
                double: true,
            });
            key(terminal, Key::Down, 0, 1);
            alias = Some("work-b");
        }
        "wheel-down" => {
            terminal.send(Input::Wheel { down: true });
            wait_menu(terminal, 0, 1);
            alias = Some("work-b");
        }
        "wheel-up" => {
            key(terminal, Key::Down, 0, 1);
            key(terminal, Key::Down, 0, 2);
            terminal.send(Input::Wheel { down: false });
            wait_menu(terminal, 0, 1);
            alias = Some("work-b");
        }
        _ => panic!("unknown scenario {scenario}"),
    }
    if !matches!(scenario, "mouse-double" | "mouse-double-motion") {
        terminal.send(Input::Key(exit_key));
    }
    let deadline = Instant::now() + TIMEOUT;
    let code = loop {
        if let Some(code) = terminal.exited() {
            break code;
        }
        assert!(
            Instant::now() < deadline,
            "{scenario}: timed out waiting for exit; screen:\n{}",
            terminal.screen()
        );
        pause();
    };
    assert_eq!(
        code,
        exit_code,
        "{scenario}: exit status; screen:\n{}",
        terminal.screen()
    );
    let calls: Vec<Vec<String>> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut expected: Vec<Vec<String>> = ["home-a", "home-b", "loose", "work-a", "work-b"]
        .into_iter()
        .map(|host| vec!["-G".into(), "-T".into(), host.into()])
        .collect();
    if let Some(alias) = alias {
        expected.push(vec![alias.into()]);
    }
    assert_eq!(
        calls, expected,
        "{scenario}: SSH calls must match exactly (including one connection at most)"
    );
}
