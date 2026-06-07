use std::fmt;
use std::time::Duration;
use thiserror::Error;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPlatform {
    Linux,
    MacOs,
    Windows,
    Unknown,
}

impl fmt::Display for ClipboardPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Unknown => "unknown",
        };

        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardWatcherConfig {
    /// Delay between polls for polling-based watchers.
    pub poll_interval: Duration,
}

impl Default for ClipboardWatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEvent {
    /// Text observed from the clipboard.
    pub text: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClipboardError {
    #[error(
        "clipboard watcher is not available on {platform}: {reason}. \
         blip-clipboard keeps platform integration isolated behind this boundary"
    )]
    Unavailable {
        platform: ClipboardPlatform,
        reason: &'static str,
    },
}

pub trait ClipboardReader {
    /// Reads the current clipboard text.
    ///
    /// `Ok(None)` means the clipboard currently has no readable text, such as
    /// after being cleared or when it contains non-text content.
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError>;
}

pub trait ClipboardWatcher {
    /// Returns the next observed clipboard event when one is available.
    fn poll_next(&mut self) -> Result<Option<ClipboardEvent>, ClipboardError>;
}

/// Polling watcher that emits text changes observed through a [`ClipboardReader`].
///
/// Duplicate suppression is based on the last readable text value. A `None` read
/// resets that state, so the same text is emitted again if it reappears. This
/// does not detect consecutive copies of identical text unless a future platform
/// reader exposes those copies as distinct readable states.
#[derive(Debug)]
pub struct PollingClipboardWatcher<R> {
    reader: R,
    poll_interval: Duration,
    last_text: Option<String>,
}

impl<R> PollingClipboardWatcher<R>
where
    R: ClipboardReader,
{
    pub fn new(reader: R, config: ClipboardWatcherConfig) -> Self {
        Self {
            reader,
            poll_interval: config.poll_interval,
            last_text: None,
        }
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

impl<R> ClipboardWatcher for PollingClipboardWatcher<R>
where
    R: ClipboardReader,
{
    fn poll_next(&mut self) -> Result<Option<ClipboardEvent>, ClipboardError> {
        let Some(text) = self.reader.read_text()? else {
            self.last_text = None;
            return Ok(None);
        };

        if self.last_text.as_ref() == Some(&text) {
            return Ok(None);
        }

        self.last_text = Some(text.clone());
        Ok(Some(ClipboardEvent { text }))
    }
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

pub fn system_watcher(
    config: ClipboardWatcherConfig,
) -> Result<impl ClipboardWatcher, ClipboardError> {
    Ok(PollingClipboardWatcher::new(
        PlatformClipboardReader::new()?,
        config,
    ))
}

#[derive(Debug)]
struct PlatformClipboardReader;

impl PlatformClipboardReader {
    fn new() -> Result<Self, ClipboardError> {
        Err(platform_unavailable_error())
    }
}

impl ClipboardReader for PlatformClipboardReader {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Err(platform_unavailable_error())
    }
}

fn platform_unavailable_error() -> ClipboardError {
    ClipboardError::Unavailable {
        platform: current_platform(),
        reason: platform_unavailable_reason(),
    }
}

#[cfg(target_os = "linux")]
fn platform_unavailable_reason() -> &'static str {
    "x11/wayland clipboard reader is not implemented yet"
}

#[cfg(target_os = "macos")]
fn platform_unavailable_reason() -> &'static str {
    "pasteboard clipboard reader is not implemented yet"
}

#[cfg(target_os = "windows")]
fn platform_unavailable_reason() -> &'static str {
    "win32 clipboard reader is not implemented yet"
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_unavailable_reason() -> &'static str {
    "this target does not have a supported clipboard backend"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StubClipboardReader {
        reads: Vec<Option<&'static str>>,
        index: usize,
    }

    impl ClipboardReader for StubClipboardReader {
        fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
            let value = self.reads.get(self.index).copied().flatten();
            self.index += 1;
            Ok(value.map(std::string::ToString::to_string))
        }
    }

    #[test]
    fn polling_watcher_emits_only_when_text_changes() {
        let reader = StubClipboardReader {
            reads: vec![Some("first"), Some("first"), Some("second"), None],
            index: 0,
        };
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "first".to_string(),
            })
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "second".to_string(),
            })
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
    }

    #[test]
    fn polling_watcher_treats_absent_text_as_state_change() {
        let reader = StubClipboardReader {
            reads: vec![Some("first"), None, Some("first")],
            index: 0,
        };
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "first".to_string(),
            })
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "first".to_string(),
            })
        );
    }

    #[test]
    fn polling_watcher_preserves_configured_interval() {
        let watcher = PollingClipboardWatcher::new(
            StubClipboardReader {
                reads: vec![None],
                index: 0,
            },
            ClipboardWatcherConfig {
                poll_interval: Duration::from_secs(2),
            },
        );

        assert_eq!(watcher.poll_interval(), Duration::from_secs(2));
    }

    #[test]
    fn system_watcher_returns_clear_platform_error() {
        let error = match system_watcher(ClipboardWatcherConfig::default()) {
            Ok(_) => panic!("platform backend should remain explicit until implemented"),
            Err(error) => error,
        };

        match error {
            ClipboardError::Unavailable { platform, reason } => {
                assert_eq!(platform, current_platform());
                assert!(!reason.is_empty());
            }
        }
    }
}
