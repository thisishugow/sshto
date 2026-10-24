use std::ffi::OsString;
use std::io;
use std::process::Command;

#[cfg(unix)]
pub fn run(args: &[OsString]) -> io::Result<i32> {
    use std::os::unix::process::CommandExt;

    Err(Command::new("ssh").args(args).exec())
}

#[cfg(windows)]
pub fn run(args: &[OsString]) -> io::Result<i32> {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    let mut child = Command::new("ssh").args(args).spawn()?;
    // 在啟動 SSH 後才忽略 Ctrl+C，避免子程序繼承忽略設定。
    let ignored = unsafe { SetConsoleCtrlHandler(None, 1) } != 0;
    let control_error = (!ignored).then(io::Error::last_os_error);
    let status = child.wait();
    if ignored && unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if let Some(error) = control_error {
        return Err(error);
    }
    Ok(status?.code().unwrap_or(1))
}
