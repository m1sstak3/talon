// Secret to the clipboard (owner 09.10: the 12 words must be copyable). Marked so Windows keeps it out of clipboard
// history (Win+V) and cloud sync, and wiped after CLEAR_S unless something else was copied meanwhile.
// ponytail: the HGLOBAL handed to Windows cannot be zeroed by us; EmptyClipboard frees it.

pub const CLEAR_S: u64 = 30;

#[cfg(windows)]
pub fn put(hwnd: isize, text: &str) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
    use windows_sys::Win32::System::DataExchange::*;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    let fail = || "не удалось скопировать".to_string();
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    // global memory with `bytes` copied in; the clipboard owns it after SetClipboardData succeeds
    let mem = |bytes: &[u8]| -> Result<HANDLE, String> {
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if h.is_null() { return Err(fail()) }
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() { GlobalFree(h); return Err(fail()) }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            GlobalUnlock(h);
            Ok(h as HANDLE)
        }
    };
    let mut utf16 = zeroize::Zeroizing::new(text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect::<Vec<u8>>());
    unsafe {
        let mut opened = false;
        for _ in 0..10 { if OpenClipboard(hwnd as _) != 0 { opened = true; break } std::thread::sleep(std::time::Duration::from_millis(20)) }
        if !opened { return Err(fail()) }
        let r = (|| {
            if EmptyClipboard() == 0 { return Err(fail()) }
            let h = mem(&utf16)?;
            if SetClipboardData(CF_UNICODETEXT as u32, h).is_null() { GlobalFree(h as _); return Err(fail()) }
            // documented opt-outs: no clipboard history, no cloud clipboard, no clipboard monitors
            for (name, val) in [("CanIncludeInClipboardHistory", Some(0u32)), ("CanUploadToCloudClipboard", Some(0)), ("ExcludeClipboardContentFromMonitorProcessing", None)] {
                let f = RegisterClipboardFormatW(wide(name).as_ptr());
                let h = mem(&val.unwrap_or(0).to_le_bytes())?;
                if f == 0 || SetClipboardData(f, h).is_null() { GlobalFree(h as _); }
            }
            Ok(())
        })();
        utf16.iter_mut().for_each(|b| *b = 0);
        CloseClipboard();
        r?;
        let seq = GetClipboardSequenceNumber();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(CLEAR_S));
            if GetClipboardSequenceNumber() == seq && OpenClipboard(hwnd as _) != 0 { EmptyClipboard(); CloseClipboard(); }
        });
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn put(_hwnd: isize, _text: &str) -> Result<(), String> { Err("копирование секретов пока только в Windows".into()) }

#[cfg(all(test, windows))]
mod tests {
    // touches the real clipboard: cargo test clip -- --ignored
    #[test]
    #[ignore]
    fn put_marks_and_clears() {
        use windows_sys::Win32::System::DataExchange::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetDesktopWindow;
        let hwnd = unsafe { GetDesktopWindow() } as isize;
        super::put(hwnd, "orbit velvet canyon").unwrap();
        let fmt = unsafe { RegisterClipboardFormatW("CanIncludeInClipboardHistory\0".encode_utf16().collect::<Vec<_>>().as_ptr()) };
        assert!(unsafe { IsClipboardFormatAvailable(13) } != 0, "text is there");
        assert!(unsafe { IsClipboardFormatAvailable(fmt) } != 0, "history opt-out is there");
        std::thread::sleep(std::time::Duration::from_secs(super::CLEAR_S + 2));
        assert!(unsafe { IsClipboardFormatAvailable(13) } == 0, "wiped after CLEAR_S");
    }
}
