//! Windows clipboard file list (CF_HDROP), used by Ctrl+V.

use std::path::PathBuf;

#[cfg(windows)]
pub fn clipboard_files() -> Vec<PathBuf> {
    unsafe { windows_impl() }
}

#[cfg(not(windows))]
pub fn clipboard_files() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
unsafe fn windows_impl() -> Vec<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::UI::Shell::DragQueryFileW;

    const CF_HDROP: u32 = 15;

    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return Vec::new();
    }
    let handle = GetClipboardData(CF_HDROP);
    let mut files = Vec::new();
    if !handle.is_null() {
        let hdrop = handle as _;
        let count = DragQueryFileW(hdrop, u32::MAX, std::ptr::null_mut(), 0);
        for index in 0..count {
            let len = DragQueryFileW(hdrop, index, std::ptr::null_mut(), 0);
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let written = DragQueryFileW(hdrop, index, buf.as_mut_ptr(), buf.len() as u32);
            if written == 0 {
                continue;
            }
            files.push(PathBuf::from(OsString::from_wide(
                &buf[..written as usize],
            )));
        }
    }
    CloseClipboard();
    files
}
