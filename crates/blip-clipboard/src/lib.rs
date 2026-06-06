#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPlatform {
    Linux,
    MacOs,
    Windows,
    Unknown,
}

pub fn current_platform() -> ClipboardPlatform {
    if cfg!(target_os = "linux") {
        ClipboardPlatform::Linux
    } else if cfg!(target_os = "macos") {
        ClipboardPlatform::MacOs
    } else if cfg!(target_os = "windows") {
        ClipboardPlatform::Windows
    } else {
        ClipboardPlatform::Unknown
    }
}

pub fn watcher_strategy_hint() -> &'static str {
    match current_platform() {
        ClipboardPlatform::Linux => "linux clipboard watcher pending: x11/wayland abstraction",
        ClipboardPlatform::MacOs => "macos clipboard watcher pending: pasteboard integration",
        ClipboardPlatform::Windows => "windows clipboard watcher pending: win32 listener",
        ClipboardPlatform::Unknown => "unsupported platform",
    }
}
