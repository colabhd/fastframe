//! The dialog as a Win32 message box, which needs no window, no console and
//! no graphics driver.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
};

use crate::Dialog;

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

pub(crate) fn show(message: &Dialog) -> bool {
    let title = wide(&message.title);
    let body = wide(&message.body);
    let buttons = if message.link.is_some() {
        MB_YESNO
    } else {
        MB_OK
    };
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call;
    // a null owner makes the box a top-level window of its own.
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            buttons | MB_ICONERROR | MB_SETFOREGROUND,
        )
    };
    answer == IDYES
}

/// Whether Windows runs on an ARM64 processor, whatever this build's
/// architecture: an x64 app runs there emulated.
pub(crate) fn native_arm64() -> bool {
    use windows_sys::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_ARM64,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let mut process: IMAGE_FILE_MACHINE = 0;
    let mut native: IMAGE_FILE_MACHINE = 0;
    // SAFETY: the pseudo-handle of this process and two writable u16s.
    let known = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } != 0;
    if known {
        native == IMAGE_FILE_MACHINE_ARM64
    } else {
        cfg!(target_arch = "aarch64")
    }
}

pub(crate) fn open_url(url: &str) {
    let verb = wide("open");
    let url = wide(url);
    // SAFETY: the strings are NUL-terminated UTF-16 that outlive the call;
    // the null parameters and directory are allowed.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    if result as usize <= 32 {
        log::warn!(
            "could not open the help page (ShellExecuteW returned {})",
            result as usize
        );
    }
}
