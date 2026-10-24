// Console attachment is process-wide: this executable keeps it out of cargo's test harness.
#[cfg(windows)]
#[path = "../tests/common/win_scenarios.rs"]
mod scenarios;

#[cfg(windows)]
mod windows {
    use super::scenarios::{self, Input, Key, Terminal, COLS, ROWS, TIMEOUT};
    use std::fs::{self, File, OpenOptions};
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Console::*;
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    fn check(ok: i32) {
        assert_ne!(ok, 0, "Win32: {}", io::Error::last_os_error());
    }

    fn console(name: &str) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .unwrap()
    }

    fn mode(file: &File) -> u32 {
        let mut mode = 0;
        check(unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) });
        mode
    }

    fn info(file: &File) -> CONSOLE_SCREEN_BUFFER_INFO {
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        check(unsafe { GetConsoleScreenBufferInfo(file.as_raw_handle(), &mut info) });
        info
    }

    // A job owns the entire shell/helper/sshto/stub tree, including on assertion failure.
    struct Job(OwnedHandle);
    impl Job {
        fn new() -> Self {
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            assert!(
                !handle.is_null(),
                "CreateJobObjectW: {}",
                io::Error::last_os_error()
            );
            let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            check(unsafe {
                SetInformationJobObject(
                    job.0.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                )
            });
            job
        }
        fn assign(&self, child: &Child) {
            check(unsafe {
                AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle())
            });
        }
    }

    fn wait_file(path: &Path, child: &mut Child) {
        let deadline = Instant::now() + TIMEOUT;
        while !path.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "helper exited before {}",
                path.display()
            );
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {}",
                path.display()
            );
            scenarios::pause();
        }
    }

    fn wait_child(child: &mut Child) -> std::process::ExitStatus {
        let deadline = Instant::now() + TIMEOUT * 4;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "child did not exit");
            scenarios::pause();
        }
    }

    fn write_result(root: &Path, result: serde_json::Value) {
        fs::write(
            root.join("result.tmp"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        fs::rename(root.join("result.tmp"), root.join("result.json")).unwrap();
    }

    // Invoked by the actual user shell, so mode snapshots bracket sshto itself.
    fn run_product(binary: &Path, root: &Path) {
        let input = console("CONIN$");
        let output = console("CONOUT$");
        // Exercise both ordinary cooked input and an existing nondefault echo setting.
        let mut input_mode = ENABLE_PROCESSED_INPUT
            | ENABLE_LINE_INPUT
            | ENABLE_ECHO_INPUT
            | ENABLE_EXTENDED_FLAGS
            | ENABLE_INSERT_MODE
            | ENABLE_QUICK_EDIT_MODE;
        if std::env::var("SSHTO_CONSOLE_SCENARIO").unwrap() == "escape" {
            input_mode &= !ENABLE_ECHO_INPUT;
        }
        check(unsafe { SetConsoleMode(input.as_raw_handle(), input_mode) });
        check(unsafe {
            SetConsoleMode(
                output.as_raw_handle(),
                ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT,
            )
        });
        let before = [mode(&input), mode(&output)];
        let mut child = Command::new(binary).spawn().unwrap();
        let status = wait_child(&mut child);
        let after = [mode(&input), mode(&output)];
        write_result(
            root,
            serde_json::json!({"code": status.code(), "before": before, "after": after}),
        );
    }

    fn host(shell: &str, binary: &Path, root: &Path) {
        let output = console("CONOUT$");
        let input = console("CONIN$");
        let handle = output.as_raw_handle();
        let old = info(&output);
        let small = SMALL_RECT {
            Left: 0,
            Top: 0,
            Right: (COLS as i16 - 1).min(old.srWindow.Right - old.srWindow.Left),
            Bottom: (ROWS as i16 - 1).min(old.srWindow.Bottom - old.srWindow.Top),
        };
        check(unsafe { SetConsoleWindowInfo(handle, 1, &small) });
        check(unsafe {
            SetConsoleScreenBufferSize(
                handle,
                COORD {
                    X: COLS as i16,
                    Y: ROWS as i16,
                },
            )
        });
        check(unsafe {
            SetConsoleWindowInfo(
                handle,
                1,
                &SMALL_RECT {
                    Left: 0,
                    Top: 0,
                    Right: COLS as i16 - 1,
                    Bottom: ROWS as i16 - 1,
                },
            )
        });
        fs::write(root.join("host-ready"), b"").unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while !root.join("attached").exists() {
            assert!(Instant::now() < deadline, "driver did not attach");
            scenarios::pause();
        }
        let helper = std::env::current_exe().unwrap();
        let mut command = match shell {
            "cmd" => {
                let mut command = Command::new("cmd.exe");
                // cmd /s strips the outer pair; raw_arg preserves the nested path quotes.
                command.args(["/d", "/s", "/c"]).raw_arg(format!(
                    "\"\"{}\" run \"{}\" \"{}\"\"",
                    helper.display(),
                    binary.display(),
                    root.display()
                ));
                command
            }
            "powershell" => {
                let quote =
                    |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
                let mut command = Command::new("powershell.exe");
                command.args(["-NoProfile", "-Command"]).arg(format!(
                    "& {} run {} {}; exit $LASTEXITCODE",
                    quote(&helper),
                    quote(binary),
                    quote(root)
                ));
                command
            }
            _ => panic!("unknown shell {shell}"),
        };
        let mut child = command
            .stdin(input)
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        assert!(wait_child(&mut child).success(), "shell/helper failed");
    }

    struct Console {
        input: File,
        root: PathBuf,
        host: Child,
    }
    impl Console {
        fn records(&self, records: &[INPUT_RECORD]) {
            let mut written = 0;
            check(unsafe {
                WriteConsoleInputW(
                    self.input.as_raw_handle(),
                    records.as_ptr(),
                    records.len() as u32,
                    &mut written,
                )
            });
            assert_eq!(written as usize, records.len());
        }
        fn mouse(x: u16, y: u16, buttons: u32, flags: u32) -> INPUT_RECORD {
            let output = console("CONOUT$");
            let window = info(&output).srWindow;
            INPUT_RECORD {
                EventType: MOUSE_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    MouseEvent: MOUSE_EVENT_RECORD {
                        dwMousePosition: COORD {
                            X: window.Left + x as i16 - 1,
                            Y: window.Top + y as i16 - 1,
                        },
                        dwButtonState: buttons,
                        dwControlKeyState: 0,
                        dwEventFlags: flags,
                    },
                },
            }
        }
    }
    impl Terminal for Console {
        fn send(&mut self, input: Input) {
            match input {
                Input::Key(key) => {
                    let (vk, ch, modifiers) = match key {
                        Key::Down => (VK_DOWN, 0, ENHANCED_KEY),
                        Key::Up => (VK_UP, 0, ENHANCED_KEY),
                        Key::Right => (VK_RIGHT, 0, ENHANCED_KEY),
                        Key::Left => (VK_LEFT, 0, ENHANCED_KEY),
                        Key::Tab => (VK_TAB, 9, 0),
                        Key::Digit(digit) => (u16::from(b'0' + digit), u16::from(b'0' + digit), 0),
                        Key::Enter => (VK_RETURN, 13, 0),
                        Key::Quit => (VK_Q, u16::from(b'q'), 0),
                        Key::Escape => (VK_ESCAPE, 27, 0),
                        Key::Interrupt => (VK_C, 3, LEFT_CTRL_PRESSED),
                    };
                    let records = [1, 0].map(|down| INPUT_RECORD {
                        EventType: KEY_EVENT as u16,
                        Event: INPUT_RECORD_0 {
                            KeyEvent: KEY_EVENT_RECORD {
                                bKeyDown: down,
                                wRepeatCount: 1,
                                wVirtualKeyCode: vk,
                                wVirtualScanCode: unsafe {
                                    MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) as u16
                                },
                                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: ch },
                                dwControlKeyState: modifiers,
                            },
                        },
                    });
                    self.records(&records);
                }
                Input::Click { x, y, double } => {
                    let mut records = vec![
                        Self::mouse(x, y, FROM_LEFT_1ST_BUTTON_PRESSED, 0),
                        Self::mouse(x, y, 0, 0),
                    ];
                    if double {
                        records.push(Self::mouse(
                            x,
                            y,
                            FROM_LEFT_1ST_BUTTON_PRESSED,
                            DOUBLE_CLICK,
                        ));
                        records.push(Self::mouse(x, y, 0, 0));
                    }
                    self.records(&records);
                }
                Input::Wheel { down } => {
                    let delta: i16 = if down { -120 } else { 120 };
                    self.records(&[Self::mouse(
                        8,
                        5,
                        u32::from(delta as u16) << 16,
                        MOUSE_WHEELED,
                    )]);
                }
            }
        }
        fn screen(&mut self) -> String {
            // Reopen each time: an old handle may still point at the inactive main buffer.
            let output = console("CONOUT$");
            let info = info(&output);
            let width = (info.srWindow.Right - info.srWindow.Left + 1) as u32;
            let mut screen = String::new();
            for row in info.srWindow.Top..=info.srWindow.Bottom {
                let mut text = vec![0; width as usize];
                let mut read = 0;
                check(unsafe {
                    ReadConsoleOutputCharacterW(
                        output.as_raw_handle(),
                        text.as_mut_ptr(),
                        width,
                        COORD {
                            X: info.srWindow.Left,
                            Y: row,
                        },
                        &mut read,
                    )
                });
                assert_eq!(read, width);
                screen.push_str(String::from_utf16(&text).unwrap().trim_end());
                screen.push('\n');
            }
            screen
        }
        fn exited(&mut self) -> Option<u32> {
            let path = self.root.join("result.json");
            if path.exists() {
                let result: serde_json::Value =
                    serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                assert_eq!(
                    result["before"], result["after"],
                    "console input/output modes were not restored: {result}"
                );
                Some(result["code"].as_u64().expect("sshto must exit normally") as u32)
            } else {
                assert!(
                    self.host.try_wait().unwrap().is_none(),
                    "host exited without a result; screen:\n{}",
                    self.screen()
                );
                None
            }
        }
    }

    fn drive(shell: &str, scenario: &str, binary: &Path, root: &Path) {
        assert!(scenarios::SCENARIOS.contains(&scenario));
        let job = Job::new();
        let mut host = Command::new(std::env::current_exe().unwrap())
            .arg("host")
            .arg(shell)
            .arg(binary)
            .arg(root)
            .env("SSHTO_CONSOLE_SCENARIO", scenario)
            .creation_flags(CREATE_NEW_CONSOLE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(root.join("host-errors.txt")).unwrap())
            .spawn()
            .unwrap();
        job.assign(&host);
        wait_file(&root.join("host-ready"), &mut host);
        // Preserve the harness pipes while changing this helper's console attachment.
        unsafe {
            let stdout: HANDLE = GetStdHandle(STD_OUTPUT_HANDLE);
            let stderr: HANDLE = GetStdHandle(STD_ERROR_HANDLE);
            FreeConsole();
            check(AttachConsole(host.id()));
            check(SetStdHandle(STD_OUTPUT_HANDLE, stdout));
            check(SetStdHandle(STD_ERROR_HANDLE, stderr));
        }
        let mut terminal = Console {
            input: console("CONIN$"),
            root: root.into(),
            host,
        };
        fs::write(root.join("attached"), b"").unwrap();
        scenarios::run(
            &mut terminal,
            scenario,
            &PathBuf::from(std::env::var_os("SSHTO_TEST_LOG").unwrap()),
        );
        assert!(wait_child(&mut terminal.host).success());
        // Keep the console and job alive through all assertions, then release the tree.
        drop(terminal);
        unsafe {
            FreeConsole();
        }
        drop(job);
    }

    pub fn main() {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        match args[0].to_str().unwrap() {
            "drive" => drive(
                args[1].to_str().unwrap(),
                args[2].to_str().unwrap(),
                Path::new(&args[3]),
                Path::new(&args[4]),
            ),
            "host" => host(
                args[1].to_str().unwrap(),
                Path::new(&args[2]),
                Path::new(&args[3]),
            ),
            "run" => run_product(Path::new(&args[1]), Path::new(&args[2])),
            _ => panic!("expected drive, host or run"),
        }
    }
}

#[cfg(windows)]
fn main() {
    windows::main();
}
#[cfg(not(windows))]
fn main() {}
