//! Read native accessibility preferences at startup/activation, never per frame.

#[cfg(target_os = "macos")]
pub(crate) fn reduced_motion() -> Option<bool> {
    use objc2::{
        MainThreadMarker, msg_send,
        runtime::{AnyClass, AnyObject},
    };
    let _main_thread = MainThreadMarker::new()?;
    let class = AnyClass::get(c"NSWorkspace")?;
    // SAFETY: AppKit is loaded; NSWorkspace is a process-owned singleton. These
    // documented, read-only methods run on the GPUI/main thread and return BOOL.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class, sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        Some(msg_send![workspace, accessibilityDisplayShouldReduceMotion])
    }
}

#[cfg(windows)]
pub(crate) fn reduced_motion() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut enabled: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to this live buffer.
    let read = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut enabled as *mut i32).cast(), 0)
    };
    (read != 0).then_some(enabled == 0)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn reduced_motion() -> Option<bool> {
    // Preserve GPUI's accessibility preference; no extra desktop/portal service.
    None
}
