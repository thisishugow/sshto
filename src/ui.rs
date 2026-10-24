use crate::menu::{self, Action, DisplayRow, Header, Key, Menu, MousePhase};
#[cfg(windows)]
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Once,
    },
    time::{Duration, Instant},
};

// Crossterm enables VT output and restores cooked input bits rather than the
// exact original modes on Windows. Preserve both console buffers ourselves.
#[cfg(windows)]
mod console_modes {
    use std::{
        fs::{File, OpenOptions},
        io,
        os::windows::io::AsRawHandle,
        sync::Mutex,
    };
    use windows_sys::Win32::System::Console::{GetConsoleMode, SetConsoleMode};

    struct Mode {
        handle: File,
        value: u32,
    }

    impl Mode {
        fn capture(name: &str) -> io::Result<Self> {
            let handle = OpenOptions::new().read(true).write(true).open(name)?;
            let mut value = 0;
            if unsafe { GetConsoleMode(handle.as_raw_handle(), &mut value) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { handle, value })
        }

        fn restore(self) -> io::Result<()> {
            if unsafe { SetConsoleMode(self.handle.as_raw_handle(), self.value) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    static ORIGINAL: Mutex<Option<(Mode, Mode)>> = Mutex::new(None);

    pub fn capture() -> io::Result<()> {
        let input = Mode::capture("CONIN$")?;
        let output = Mode::capture("CONOUT$")?;
        *ORIGINAL.lock().unwrap() = Some((input, output));
        Ok(())
    }

    pub fn restore() -> io::Result<()> {
        let modes = ORIGINAL.lock().unwrap().take();
        if let Some((input, output)) = modes {
            // Attempt both restorations even if one fails.
            let input = input.restore();
            let output = output.restore();
            return input.and(output);
        }
        Ok(())
    }
}

// Nonblocking reads keep incomplete CSI sequences from blocking crossterm polling,
// so termination signals and resize events can still be handled.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod input_flags {
    use std::{
        io,
        os::raw::c_int,
        sync::atomic::{AtomicI32, Ordering},
    };

    const F_GETFL: c_int = 3;
    const F_SETFL: c_int = 4;
    #[cfg(target_os = "macos")]
    const O_NONBLOCK: c_int = 0x0004;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: c_int = 0x0800;
    static ORIGINAL: AtomicI32 = AtomicI32::new(-1);

    extern "C" {
        fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    }

    pub fn enable_nonblocking() -> io::Result<()> {
        // Startup checks ensure stdin is a terminal; fcntl does not take ownership.
        let original = unsafe { fcntl(0, F_GETFL) };
        if original == -1 || unsafe { fcntl(0, F_SETFL, original | O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error());
        }
        ORIGINAL.store(original, Ordering::SeqCst);
        Ok(())
    }

    pub fn restore() -> io::Result<()> {
        let original = ORIGINAL.swap(-1, Ordering::SeqCst);
        if original != -1 && unsafe { fcntl(0, F_SETFL, original) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

fn enable_mouse(output: &mut impl Write) -> io::Result<()> {
    #[cfg(unix)]
    output.write_all(b"\x1b[?1000h\x1b[?1006h")?;
    #[cfg(windows)]
    execute!(output, EnableMouseCapture)?;
    output.flush()
}

fn disable_mouse(output: &mut impl Write) -> io::Result<()> {
    #[cfg(unix)]
    output.write_all(b"\x1b[?1000l\x1b[?1006l")?;
    #[cfg(windows)]
    execute!(output, DisableMouseCapture)?;
    output.flush()
}

fn restore_terminal() -> io::Result<()> {
    if !TERMINAL_ACTIVE.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    // PTY stdin and stdout may share an open file description. Restore flags first
    // to prevent WouldBlock errors while clearing the display.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let flags = input_flags::restore();
    let mut output = io::stdout();
    let display = (|| {
        disable_mouse(&mut output)?;
        output.write_all(b"\x1b[0m")?;
        execute!(output, Show, LeaveAlternateScreen)
    })();
    // Restore terminal mode even if display cleanup fails.
    let mode = terminal::disable_raw_mode();
    #[cfg(windows)]
    let mode = mode.and(console_modes::restore());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let mode = flags.and(mode);
    display.and(mode)
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        PANIC_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = restore_terminal();
                previous(info);
            }));
        });
        #[cfg(windows)]
        console_modes::capture()?;
        TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
        let guard = Self;
        terminal::enable_raw_mode()?;
        // Send Hide last so the test readiness marker signals completed initialization.
        let mut output = io::stdout();
        execute!(output, EnterAlternateScreen)?;
        enable_mouse(&mut output)?;
        execute!(output, Hide)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

struct Termination {
    exit_code: Arc<AtomicUsize>,
    #[cfg(unix)]
    registrations: Vec<signal_hook::SigId>,
}

impl Termination {
    fn new() -> io::Result<Self> {
        let termination = Self {
            exit_code: Arc::new(AtomicUsize::new(0)),
            #[cfg(unix)]
            registrations: Vec::new(),
        };
        #[cfg(unix)]
        {
            let mut termination = termination;
            for signal in [
                signal_hook::consts::SIGTERM,
                signal_hook::consts::SIGINT,
                signal_hook::consts::SIGHUP,
            ] {
                termination
                    .registrations
                    .push(signal_hook::flag::register_usize(
                        signal,
                        termination.exit_code.clone(),
                        128 + signal as usize,
                    )?);
            }
            Ok(termination)
        }
        #[cfg(not(unix))]
        Ok(termination)
    }

    fn exit_code(&self) -> Option<u8> {
        let code = self.exit_code.load(Ordering::Relaxed);
        (code != 0).then_some(code as u8)
    }
}

impl Drop for Termination {
    fn drop(&mut self) {
        #[cfg(unix)]
        for registration in &self.registrations {
            signal_hook::low_level::unregister(*registration);
        }
    }
}

fn pressed_key(event: KeyEvent) -> Option<Key> {
    (event.kind == KeyEventKind::Press).then(|| key(event))
}

fn key(event: KeyEvent) -> Key {
    if event.modifiers.contains(KeyModifiers::ALT) {
        return Key::Quit;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        return match event.code {
            KeyCode::Char('c') => Key::Interrupt,
            KeyCode::Char('d') => Key::Quit,
            KeyCode::Char('j' | 'm') => Key::Enter,
            _ => Key::Other,
        };
    }
    match event.code {
        KeyCode::Up if event.modifiers.is_empty() => Key::Up,
        KeyCode::Down if event.modifiers.is_empty() => Key::Down,
        KeyCode::Left if event.modifiers.is_empty() => Key::Left,
        KeyCode::Right if event.modifiers.is_empty() => Key::Right,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::ShiftTab,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Key::Quit,
        KeyCode::Char(digit @ '0'..='9') => Key::Digit(digit as u8 - b'0'),
        _ => Key::Other,
    }
}

fn mouse(event: event::MouseEvent) -> Option<menu::MouseEvent> {
    let button_code = |button| match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let (mut button, phase) = match event.kind {
        MouseEventKind::Down(button) => (button_code(button), MousePhase::Press),
        MouseEventKind::Up(button) => (button_code(button), MousePhase::Release),
        MouseEventKind::Drag(_) | MouseEventKind::Moved => return None,
        MouseEventKind::ScrollUp => (64, MousePhase::Press),
        MouseEventKind::ScrollDown => (65, MousePhase::Press),
        MouseEventKind::ScrollLeft => (66, MousePhase::Press),
        MouseEventKind::ScrollRight => (67, MousePhase::Press),
    };
    for (modifier, bit) in [
        (KeyModifiers::SHIFT, 4),
        (KeyModifiers::ALT, 8),
        (KeyModifiers::CONTROL, 16),
    ] {
        if event.modifiers.contains(modifier) {
            button |= bit;
        }
    }
    Some(menu::MouseEvent {
        button,
        x: usize::from(event.column) + 1,
        y: usize::from(event.row) + 1,
        phase,
    })
}

fn render(menu: &mut Menu, rows: u16, cols: u16) -> Vec<u8> {
    let layout = menu.layout(usize::from(rows), usize::from(cols));
    let mut frame = String::from("\x1b[H\x1b[J");
    match &layout.header {
        Header::Hidden => {}
        Header::Tabs => {
            for (index, tab) in menu.tabs().iter().enumerate() {
                frame.push_str(if index == menu.tab() {
                    "\x1b[1;36m"
                } else {
                    "\x1b[2m"
                });
                frame.push_str(&tab.text);
                frame.push_str("\x1b[0m");
            }
        }
        Header::Compact(text) => {
            frame.push_str("\x1b[1;36m");
            frame.push_str(text);
            frame.push_str("\x1b[0m");
        }
    }
    if layout.header_rows > 0 {
        frame.push_str("\r\n");
    }
    if let Some(controls) = &layout.controls {
        frame.push_str(controls);
        frame.push_str("\r\n");
    }
    if menu.visible().is_empty() {
        frame.push_str(&menu::truncate_cells(
            if layout.cols >= 24 {
                "  No hosts in this group"
            } else {
                "(empty)"
            },
            layout.width,
        ));
    } else {
        let end = (layout.offset + layout.slots).min(menu.display_rows().len());
        for row in layout.offset..end {
            let text = match menu.display_rows()[row] {
                DisplayRow::Heading(group) => {
                    frame.push_str("\x1b[1;36m");
                    let tab = &menu.tabs()[group + 1];
                    format!("{} ({})", tab.label, tab.count)
                }
                DisplayRow::Host(index) => {
                    let marker = if row == layout.selected_row {
                        frame.push_str("\x1b[1;97;44m");
                        '>'
                    } else {
                        ' '
                    };
                    let indent = " ".repeat(layout.indent);
                    let host = &menu.hosts()[index];
                    if layout.cols >= 78 {
                        let alias: String = host
                            .alias
                            .chars()
                            .take(layout.alias_width as usize)
                            .collect();
                        let info: String = host
                            .info
                            .display()
                            .chars()
                            .take(layout.info_width as usize)
                            .collect();
                        format!(
                            "{indent} {marker} {alias:width$}  {info}",
                            width = layout.alias_width as usize
                        )
                    } else if layout.width >= 3 {
                        format!("{indent}{marker} {}", host.alias)
                    } else {
                        host.alias.clone()
                    }
                }
            };
            frame.push_str(&menu::truncate_cells(&text, layout.width));
            frame.push_str("\x1b[0m");
            if row + 1 < end {
                frame.push_str("\r\n");
            }
        }
    }
    if let Some(footer) = &layout.footer {
        frame.push_str(&format!("\x1b[{};1H\x1b[2m{footer}\x1b[0m", layout.rows));
    }
    frame.into_bytes()
}

fn draw(menu: &mut Menu, rows: u16, cols: u16) -> io::Result<()> {
    let frame = render(menu, rows, cols);
    let mut output = io::stdout();
    output.write_all(&frame)?;
    output.flush()
}

fn read_event(timeout: Duration) -> io::Result<Option<Event>> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    input_flags::enable_nonblocking()?;
    let result = (|| {
        if event::poll(timeout)? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    })();
    // Use nonblocking mode only while reading events; drawing and SSH keep the original flags.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let restored = input_flags::restore();
        result.and_then(|event| restored.map(|()| event))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    result
}

pub fn run(menu: &mut Menu) -> io::Result<Action> {
    let termination = Termination::new()?;
    let _terminal = TerminalGuard::enter()?;
    let started = Instant::now();
    let (mut cols, mut rows) = terminal::size()?;
    draw(menu, rows, cols)?;
    let action = loop {
        if let Some(code) = termination.exit_code() {
            break Action::Exit(code);
        }
        let Some(event) = read_event(Duration::from_millis(50))? else {
            continue;
        };
        let action = match event {
            Event::Key(event) => match pressed_key(event) {
                Some(key) => menu.handle_key(key),
                None => continue,
            },
            Event::Mouse(event) => match mouse(event) {
                Some(event) => menu.handle_mouse(event, started.elapsed().as_millis() as u64),
                None => continue,
            },
            Event::Resize(new_cols, new_rows) => {
                menu.mark_resized();
                (cols, rows) = (new_cols, new_rows);
                Action::None
            }
            _ => continue,
        };
        if action != Action::None {
            break action;
        }
        draw(menu, rows, cols)?;
    };
    if matches!(action, Action::Connect(_)) {
        disable_mouse(&mut io::stdout())?;
        // Drain both parsed crossterm events and pending terminal input before SSH.
        while read_event(Duration::from_millis(20))?.is_some() {
            if termination.exit_code().is_some() {
                break;
            }
        }
    }
    restore_terminal()?;
    Ok(termination.exit_code().map_or(action, Action::Exit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_key_presses_produce_actions() {
        for (code, modifiers, expected) in [
            (KeyCode::Down, KeyModifiers::NONE, Key::Down),
            (KeyCode::Tab, KeyModifiers::NONE, Key::Tab),
            (KeyCode::Char('1'), KeyModifiers::NONE, Key::Digit(1)),
            (KeyCode::Enter, KeyModifiers::NONE, Key::Enter),
            (KeyCode::Char('q'), KeyModifiers::NONE, Key::Quit),
            (KeyCode::Char('c'), KeyModifiers::CONTROL, Key::Interrupt),
            (KeyCode::Char('d'), KeyModifiers::CONTROL, Key::Quit),
            (KeyCode::Char('x'), KeyModifiers::ALT, Key::Quit),
        ] {
            for kind in [
                KeyEventKind::Press,
                KeyEventKind::Release,
                KeyEventKind::Repeat,
            ] {
                assert_eq!(
                    pressed_key(KeyEvent::new_with_kind(code, modifiers, kind)),
                    (kind == KeyEventKind::Press).then_some(expected),
                    "{code:?} {modifiers:?} {kind:?}"
                );
            }
        }
    }

    #[test]
    fn keys_preserve_exit_codes_and_ignore_modified_arrows() {
        for (code, modifiers, expected) in [
            (KeyCode::Char('c'), KeyModifiers::CONTROL, Key::Interrupt),
            (KeyCode::Char('d'), KeyModifiers::CONTROL, Key::Quit),
            (KeyCode::Char('j'), KeyModifiers::CONTROL, Key::Enter),
            (KeyCode::Char('x'), KeyModifiers::ALT, Key::Quit),
            (KeyCode::Down, KeyModifiers::SHIFT, Key::Other),
            (KeyCode::Down, KeyModifiers::NONE, Key::Down),
            (KeyCode::BackTab, KeyModifiers::SHIFT, Key::ShiftTab),
        ] {
            assert_eq!(key(KeyEvent::new(code, modifiers)), expected);
        }
    }

    #[test]
    fn mouse_modifiers_do_not_activate_plain_clicks_or_wheels() {
        let event = event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 4,
            row: 3,
            modifiers: KeyModifiers::CONTROL,
        };
        assert_eq!(
            mouse(event),
            Some(menu::MouseEvent {
                button: 81,
                x: 5,
                y: 4,
                phase: MousePhase::Press,
            })
        );
    }

    #[test]
    fn mouse_motion_and_drag_do_not_produce_menu_events() {
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Middle),
            MouseEventKind::Drag(MouseButton::Right),
        ] {
            assert_eq!(
                mouse(event::MouseEvent {
                    kind,
                    column: 5,
                    row: 3,
                    modifiers: KeyModifiers::NONE,
                }),
                None,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn frames_keep_required_ansi_boundaries_and_host_rows() {
        let (config, hosts) = crate::hosts::tests::fixture();
        let mut menu = Menu::new(config.labels, hosts);
        let frame = String::from_utf8(render(&mut menu, 7, 40)).unwrap();
        assert!(frame.starts_with("\x1b[H\x1b[J"));
        assert!(frame.contains("\r\n\x1b[1;36mTailscale (1)\x1b[0m\r\n"));
        assert!(frame.contains("\x1b[1;97;44m  > cloud-ts\x1b[0m"));
        assert!(frame.contains("\x1b[7;1H\x1b[2msshto  1/6"));
        let single = String::from_utf8(render(&mut menu, 1, 2)).unwrap();
        assert_eq!(single, "\x1b[H\x1b[J\x1b[1;97;44mc\x1b[0m");
    }
}
