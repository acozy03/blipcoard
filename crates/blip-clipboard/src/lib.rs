use arboard::{Clipboard, Error as ArboardError, ImageData};
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use image::{ColorType, ExtendedColorType};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;
const NORMALIZED_IMAGE_MIME_TYPE: &str = "image/png";

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
    /// Maximum decoded image byte size accepted before PNG normalization.
    pub max_image_bytes: usize,
}

impl Default for ClipboardWatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEvent {
    pub payload: ClipboardPayload,
}

impl ClipboardEvent {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            payload: ClipboardPayload::Text(text.into()),
        }
    }

    pub fn image(image: ClipboardImage) -> Self {
        Self {
            payload: ClipboardPayload::Image(image),
        }
    }

    pub fn file_list(file_list: ClipboardFileList) -> Self {
        Self {
            payload: ClipboardPayload::FileList(file_list),
        }
    }

    pub fn html(rich_text: ClipboardRichText) -> Self {
        Self {
            payload: ClipboardPayload::Html(rich_text),
        }
    }

    pub fn rtf(rich_text: ClipboardRichText) -> Self {
        Self {
            payload: ClipboardPayload::Rtf(rich_text),
        }
    }

    pub fn unknown(unknown: ClipboardUnknown) -> Self {
        Self {
            payload: ClipboardPayload::Unknown(unknown),
        }
    }

    pub fn text_payload(&self) -> Option<&str> {
        match &self.payload {
            ClipboardPayload::Text(text) => Some(text.as_str()),
            ClipboardPayload::Image(_)
            | ClipboardPayload::FileList(_)
            | ClipboardPayload::Html(_)
            | ClipboardPayload::Rtf(_)
            | ClipboardPayload::Unknown(_) => None,
        }
    }

    pub fn image_payload(&self) -> Option<&ClipboardImage> {
        match &self.payload {
            ClipboardPayload::Text(_)
            | ClipboardPayload::FileList(_)
            | ClipboardPayload::Html(_)
            | ClipboardPayload::Rtf(_)
            | ClipboardPayload::Unknown(_) => None,
            ClipboardPayload::Image(image) => Some(image),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPayload {
    Text(String),
    Image(ClipboardImage),
    FileList(ClipboardFileList),
    Html(ClipboardRichText),
    Rtf(ClipboardRichText),
    Unknown(ClipboardUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    /// Normalized image bytes. Images are normalized to PNG for local storage.
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
    pub platform_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFileList {
    /// Referenced paths from the clipboard. File bytes are not imported by default.
    pub paths: Vec<PathBuf>,
    pub byte_size: usize,
    pub platform_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRichText {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub byte_size: usize,
    pub plain_text: Option<String>,
    pub platform_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardUnknown {
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub byte_size: usize,
    pub platform_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCapabilities {
    pub platform: ClipboardPlatform,
    pub text: bool,
    pub image: bool,
    pub file_list: bool,
    pub html: bool,
    pub rtf: bool,
    pub image_formats: Vec<&'static str>,
    pub rich_text_formats: Vec<&'static str>,
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

    #[error("clipboard is temporarily occupied on {platform}")]
    Occupied { platform: ClipboardPlatform },

    #[error("clipboard {payload_kind} payload is not supported on {platform}: {reason}")]
    Unsupported {
        platform: ClipboardPlatform,
        payload_kind: &'static str,
        reason: &'static str,
    },

    #[error("clipboard backend error on {platform}: {message}")]
    Backend {
        platform: ClipboardPlatform,
        message: String,
    },
}

pub trait ClipboardReader {
    /// Reads the current clipboard text.
    ///
    /// `Ok(None)` means the clipboard currently has no readable text, such as
    /// after being cleared or when it contains non-text content.
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError>;

    /// Reads the current clipboard image payload.
    ///
    /// `Ok(None)` means the clipboard currently has no readable image.
    fn read_image(&mut self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Ok(None)
    }

    fn read_file_list(&mut self) -> Result<Option<ClipboardFileList>, ClipboardError> {
        Ok(None)
    }

    fn read_html(&mut self) -> Result<Option<ClipboardRichText>, ClipboardError> {
        Ok(None)
    }

    fn read_rtf(&mut self) -> Result<Option<ClipboardRichText>, ClipboardError> {
        Ok(None)
    }

    fn read_unknown(&mut self) -> Result<Option<ClipboardUnknown>, ClipboardError> {
        Ok(None)
    }

    fn capabilities(&self) -> ClipboardCapabilities {
        platform_capabilities(current_platform())
    }
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
    last_payload: Option<ClipboardPayloadFingerprint>,
}

impl<R> PollingClipboardWatcher<R>
where
    R: ClipboardReader,
{
    pub fn new(reader: R, config: ClipboardWatcherConfig) -> Self {
        Self {
            reader,
            poll_interval: config.poll_interval,
            last_payload: None,
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
        if let Some(file_list) = self.reader.read_file_list()? {
            let fingerprint = ClipboardPayloadFingerprint::file_list(&file_list);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::file_list(file_list)));
        }

        if let Some(html) = self.reader.read_html()? {
            let fingerprint = ClipboardPayloadFingerprint::rich_text("html", &html);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::html(html)));
        }

        if let Some(rtf) = self.reader.read_rtf()? {
            let fingerprint = ClipboardPayloadFingerprint::rich_text("rtf", &rtf);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::rtf(rtf)));
        }

        if let Some(text) = self.reader.read_text()? {
            let fingerprint = ClipboardPayloadFingerprint::text(&text);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::text(text)));
        }

        if let Some(image) = self.reader.read_image()? {
            let fingerprint = ClipboardPayloadFingerprint::image(&image);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::image(image)));
        }

        if let Some(unknown) = self.reader.read_unknown()? {
            let fingerprint = ClipboardPayloadFingerprint::unknown(&unknown);
            if self.last_payload.as_ref() == Some(&fingerprint) {
                return Ok(None);
            }

            self.last_payload = Some(fingerprint);
            return Ok(Some(ClipboardEvent::unknown(unknown)));
        }

        self.last_payload = None;
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClipboardPayloadFingerprint {
    Text(String),
    FileList {
        paths: Vec<PathBuf>,
        byte_size: usize,
    },
    RichText {
        kind: &'static str,
        byte_size: usize,
        content_hash: u64,
    },
    Image {
        width: u32,
        height: u32,
        byte_size: usize,
        content_hash: u64,
    },
    Unknown {
        platform_format: String,
        byte_size: usize,
        content_hash: u64,
    },
}

impl ClipboardPayloadFingerprint {
    fn text(text: &str) -> Self {
        Self::Text(text.to_owned())
    }

    fn image(image: &ClipboardImage) -> Self {
        let mut hasher = DefaultHasher::new();
        image.bytes.hash(&mut hasher);
        Self::Image {
            width: image.width,
            height: image.height,
            byte_size: image.byte_size,
            content_hash: hasher.finish(),
        }
    }

    fn file_list(file_list: &ClipboardFileList) -> Self {
        Self::FileList {
            paths: file_list.paths.clone(),
            byte_size: file_list.byte_size,
        }
    }

    fn rich_text(kind: &'static str, rich_text: &ClipboardRichText) -> Self {
        let mut hasher = DefaultHasher::new();
        rich_text.bytes.hash(&mut hasher);
        Self::RichText {
            kind,
            byte_size: rich_text.byte_size,
            content_hash: hasher.finish(),
        }
    }

    fn unknown(unknown: &ClipboardUnknown) -> Self {
        let mut hasher = DefaultHasher::new();
        unknown.bytes.hash(&mut hasher);
        Self::Unknown {
            platform_format: unknown.platform_format.clone(),
            byte_size: unknown.byte_size,
            content_hash: hasher.finish(),
        }
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

pub fn clipboard_capabilities() -> ClipboardCapabilities {
    platform_capabilities(current_platform())
}

pub fn platform_capabilities(platform: ClipboardPlatform) -> ClipboardCapabilities {
    match platform {
        ClipboardPlatform::Linux => ClipboardCapabilities {
            platform,
            text: true,
            image: true,
            file_list: true,
            html: true,
            rtf: false,
            image_formats: vec!["image/png", "image/bmp", "image/tiff"],
            rich_text_formats: vec!["text/html", "text/uri-list"],
        },
        ClipboardPlatform::MacOs => ClipboardCapabilities {
            platform,
            text: true,
            image: true,
            file_list: true,
            html: true,
            rtf: false,
            image_formats: vec!["public.png", "public.tiff", "NSImage"],
            rich_text_formats: vec!["public.html", "public.file-url"],
        },
        ClipboardPlatform::Windows => ClipboardCapabilities {
            platform,
            text: true,
            image: true,
            file_list: true,
            html: true,
            rtf: false,
            image_formats: vec!["CF_DIB", "CF_BITMAP", "PNG"],
            rich_text_formats: vec!["HTML Format", "CF_HDROP"],
        },
        ClipboardPlatform::Unknown => ClipboardCapabilities {
            platform,
            text: false,
            image: false,
            file_list: false,
            html: false,
            rtf: false,
            image_formats: Vec::new(),
            rich_text_formats: Vec::new(),
        },
    }
}

pub fn system_watcher(
    config: ClipboardWatcherConfig,
) -> Result<impl ClipboardWatcher, ClipboardError> {
    Ok(PollingClipboardWatcher::new(
        PlatformClipboardReader::with_config(config.clone())?,
        config,
    ))
}

pub struct PlatformClipboardReader {
    clipboard: Clipboard,
    platform: ClipboardPlatform,
    max_image_bytes: usize,
}

impl PlatformClipboardReader {
    pub fn new() -> Result<Self, ClipboardError> {
        Self::with_config(ClipboardWatcherConfig::default())
    }

    pub fn with_config(config: ClipboardWatcherConfig) -> Result<Self, ClipboardError> {
        let platform = current_platform();
        let clipboard = Clipboard::new().map_err(|error| map_arboard_error(error, platform))?;
        Ok(Self {
            clipboard,
            platform,
            max_image_bytes: config.max_image_bytes,
        })
    }
}

impl ClipboardReader for PlatformClipboardReader {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        match self.clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(ArboardError::ContentNotAvailable) => Ok(None),
            Err(error) => Err(map_arboard_error(error, self.platform)),
        }
    }

    fn read_image(&mut self) -> Result<Option<ClipboardImage>, ClipboardError> {
        let image = match self.clipboard.get_image() {
            Ok(image) => image,
            Err(ArboardError::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(map_arboard_error(error, self.platform)),
        };

        if image.bytes.len() > self.max_image_bytes {
            return Err(ClipboardError::Unsupported {
                platform: self.platform,
                payload_kind: "image",
                reason: "decoded image exceeds configured byte limit",
            });
        }

        normalize_image(image, self.platform).map(Some)
    }

    fn read_file_list(&mut self) -> Result<Option<ClipboardFileList>, ClipboardError> {
        let paths = match self.clipboard.get().file_list() {
            Ok(paths) => paths,
            Err(ArboardError::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(map_arboard_error(error, self.platform)),
        };

        if paths.is_empty() {
            return Ok(None);
        }

        Ok(Some(normalize_file_list(paths, self.platform)))
    }

    fn read_html(&mut self) -> Result<Option<ClipboardRichText>, ClipboardError> {
        let html = match self.clipboard.get().html() {
            Ok(html) => html,
            Err(ArboardError::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(map_arboard_error(error, self.platform)),
        };

        let plain_text = match self.clipboard.get_text() {
            Ok(text) if !text.is_empty() => Some(text),
            Ok(_) | Err(ArboardError::ContentNotAvailable) => None,
            Err(_) => None,
        };

        Ok(Some(normalize_rich_text(
            html,
            "text/html",
            platform_html_format(self.platform),
            plain_text,
        )))
    }

    fn capabilities(&self) -> ClipboardCapabilities {
        platform_capabilities(self.platform)
    }
}

fn normalize_file_list(paths: Vec<PathBuf>, platform: ClipboardPlatform) -> ClipboardFileList {
    let byte_size = paths.iter().map(|path| path_byte_len(path)).sum();
    ClipboardFileList {
        paths,
        byte_size,
        platform_format: Some(platform_file_list_format(platform).to_owned()),
    }
}

#[cfg(unix)]
fn path_byte_len(path: &std::path::Path) -> usize {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().len()
}

#[cfg(windows)]
fn path_byte_len(path: &std::path::Path) -> usize {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().count() * 2
}

#[cfg(not(any(unix, windows)))]
fn path_byte_len(path: &std::path::Path) -> usize {
    path.as_os_str().to_string_lossy().len()
}

fn normalize_rich_text(
    content: String,
    mime_type: &str,
    platform_format: &str,
    plain_text: Option<String>,
) -> ClipboardRichText {
    let bytes = content.into_bytes();
    let byte_size = bytes.len();
    ClipboardRichText {
        bytes,
        mime_type: mime_type.to_owned(),
        byte_size,
        plain_text,
        platform_format: Some(platform_format.to_owned()),
    }
}

fn normalize_image(
    image: ImageData<'_>,
    platform: ClipboardPlatform,
) -> Result<ClipboardImage, ClipboardError> {
    let expected_len = image
        .width
        .checked_mul(image.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ClipboardError::Unsupported {
            platform,
            payload_kind: "image",
            reason: "image dimensions overflow byte-size calculation",
        })?;

    if image.bytes.len() != expected_len {
        return Err(ClipboardError::Unsupported {
            platform,
            payload_kind: "image",
            reason: "decoded image is not RGBA8",
        });
    }

    let width = u32::try_from(image.width).map_err(|_| ClipboardError::Unsupported {
        platform,
        payload_kind: "image",
        reason: "image width exceeds supported range",
    })?;
    let height = u32::try_from(image.height).map_err(|_| ClipboardError::Unsupported {
        platform,
        payload_kind: "image",
        reason: "image height exceeds supported range",
    })?;

    let mut bytes = Vec::new();
    let encoder = PngEncoder::new(&mut bytes);
    encoder
        .write_image(
            image.bytes.as_ref(),
            width,
            height,
            ExtendedColorType::from(ColorType::Rgba8),
        )
        .map_err(|_| ClipboardError::Unsupported {
            platform,
            payload_kind: "image",
            reason: "image could not be normalized to PNG",
        })?;

    let byte_size = bytes.len();
    Ok(ClipboardImage {
        bytes,
        mime_type: NORMALIZED_IMAGE_MIME_TYPE.to_owned(),
        width,
        height,
        byte_size,
        platform_format: Some(platform_image_format(platform).to_owned()),
    })
}

fn map_arboard_error(error: ArboardError, platform: ClipboardPlatform) -> ClipboardError {
    match error {
        ArboardError::ContentNotAvailable => ClipboardError::Unsupported {
            platform,
            payload_kind: "clipboard",
            reason: "requested clipboard content is not available",
        },
        ArboardError::ClipboardNotSupported => ClipboardError::Unavailable {
            platform,
            reason: platform_unavailable_reason(platform),
        },
        ArboardError::ClipboardOccupied => ClipboardError::Occupied { platform },
        ArboardError::ConversionFailure => ClipboardError::Unsupported {
            platform,
            payload_kind: "clipboard",
            reason: "platform clipboard content could not be converted",
        },
        ArboardError::Unknown { description } => ClipboardError::Backend {
            platform,
            message: description,
        },
        error => ClipboardError::Backend {
            platform,
            message: error.to_string(),
        },
    }
}

fn platform_unavailable_reason(platform: ClipboardPlatform) -> &'static str {
    match platform {
        ClipboardPlatform::Linux => {
            "x11/wayland clipboard reader is unavailable in the current environment"
        }
        ClipboardPlatform::MacOs => "pasteboard clipboard reader is unavailable",
        ClipboardPlatform::Windows => "win32 clipboard reader is unavailable",
        ClipboardPlatform::Unknown => "this target does not have a supported clipboard backend",
    }
}

fn platform_image_format(platform: ClipboardPlatform) -> &'static str {
    match platform {
        ClipboardPlatform::Linux => "arboard:image/png-or-decoded-rgba",
        ClipboardPlatform::MacOs => "arboard:NSImage-decoded-rgba",
        ClipboardPlatform::Windows => "arboard:CF_DIB-CF_BITMAP-PNG-decoded-rgba",
        ClipboardPlatform::Unknown => "arboard:decoded-rgba",
    }
}

fn platform_file_list_format(platform: ClipboardPlatform) -> &'static str {
    match platform {
        ClipboardPlatform::Linux => "arboard:text-uri-list",
        ClipboardPlatform::MacOs => "arboard:NSPasteboard-file-url",
        ClipboardPlatform::Windows => "arboard:CF_HDROP",
        ClipboardPlatform::Unknown => "arboard:file-list",
    }
}

fn platform_html_format(platform: ClipboardPlatform) -> &'static str {
    match platform {
        ClipboardPlatform::Linux => "arboard:text/html",
        ClipboardPlatform::MacOs => "arboard:NSPasteboardTypeHTML",
        ClipboardPlatform::Windows => "arboard:HTML Format",
        ClipboardPlatform::Unknown => "arboard:text/html",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug)]
    struct StubClipboardReader {
        reads: Vec<Option<&'static str>>,
        images: Vec<Option<ClipboardImage>>,
        file_lists: Vec<Option<ClipboardFileList>>,
        html: Vec<Option<ClipboardRichText>>,
        rtf: Vec<Option<ClipboardRichText>>,
        unknown: Vec<Option<ClipboardUnknown>>,
        index: usize,
        image_index: usize,
        file_list_index: usize,
        html_index: usize,
        rtf_index: usize,
        unknown_index: usize,
    }

    impl ClipboardReader for StubClipboardReader {
        fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
            let value = self.reads.get(self.index).copied().flatten();
            self.index += 1;
            Ok(value.map(std::string::ToString::to_string))
        }

        fn read_image(&mut self) -> Result<Option<ClipboardImage>, ClipboardError> {
            let value = self.images.get(self.image_index).cloned().flatten();
            self.image_index += 1;
            Ok(value)
        }

        fn read_file_list(&mut self) -> Result<Option<ClipboardFileList>, ClipboardError> {
            let value = self.file_lists.get(self.file_list_index).cloned().flatten();
            self.file_list_index += 1;
            Ok(value)
        }

        fn read_html(&mut self) -> Result<Option<ClipboardRichText>, ClipboardError> {
            let value = self.html.get(self.html_index).cloned().flatten();
            self.html_index += 1;
            Ok(value)
        }

        fn read_rtf(&mut self) -> Result<Option<ClipboardRichText>, ClipboardError> {
            let value = self.rtf.get(self.rtf_index).cloned().flatten();
            self.rtf_index += 1;
            Ok(value)
        }

        fn read_unknown(&mut self) -> Result<Option<ClipboardUnknown>, ClipboardError> {
            let value = self.unknown.get(self.unknown_index).cloned().flatten();
            self.unknown_index += 1;
            Ok(value)
        }
    }

    impl StubClipboardReader {
        fn new(reads: Vec<Option<&'static str>>) -> Self {
            Self {
                reads,
                images: Vec::new(),
                file_lists: Vec::new(),
                html: Vec::new(),
                rtf: Vec::new(),
                unknown: Vec::new(),
                index: 0,
                image_index: 0,
                file_list_index: 0,
                html_index: 0,
                rtf_index: 0,
                unknown_index: 0,
            }
        }
    }

    #[test]
    fn polling_watcher_emits_only_when_text_changes() {
        let reader =
            StubClipboardReader::new(vec![Some("first"), Some("first"), Some("second"), None]);
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::text("first"))
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::text("second"))
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
    }

    #[test]
    fn polling_watcher_treats_absent_text_as_state_change() {
        let reader = StubClipboardReader::new(vec![Some("first"), None, Some("first")]);
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::text("first"))
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::text("first"))
        );
    }

    #[test]
    fn polling_watcher_preserves_text_when_text_and_image_are_available() {
        let image = sample_clipboard_image();
        let mut reader = StubClipboardReader::new(vec![Some("fallback text")]);
        reader.images = vec![Some(image)];
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::text("fallback text"))
        );
    }

    #[test]
    fn polling_watcher_suppresses_duplicate_images_and_resets_on_none() {
        let image = sample_clipboard_image();
        let mut reader = StubClipboardReader::new(vec![None, None, None, None]);
        reader.images = vec![
            Some(image.clone()),
            Some(image.clone()),
            None,
            Some(image.clone()),
        ];
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::image(image.clone()))
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::image(image))
        );
    }

    #[test]
    fn polling_watcher_preserves_configured_interval() {
        let watcher = PollingClipboardWatcher::new(
            StubClipboardReader::new(vec![None]),
            ClipboardWatcherConfig {
                poll_interval: Duration::from_secs(2),
                max_image_bytes: 123,
            },
        );

        assert_eq!(watcher.poll_interval(), Duration::from_secs(2));
    }

    #[test]
    fn polling_watcher_emits_file_list_before_text_fallback() {
        let file_list = sample_file_list();
        let mut reader = StubClipboardReader::new(vec![Some("file:///tmp/a.txt")]);
        reader.file_lists = vec![Some(file_list.clone())];
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::file_list(file_list))
        );
    }

    #[test]
    fn polling_watcher_emits_html_with_plain_text_fallback() {
        let html = sample_html();
        let mut reader = StubClipboardReader::new(vec![Some("Hello")]);
        reader.html = vec![Some(html.clone())];
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::html(html))
        );
    }

    #[test]
    fn polling_watcher_emits_rtf_and_unknown_payloads() {
        let rtf = sample_rtf();
        let mut reader = StubClipboardReader::new(vec![None, None]);
        reader.rtf = vec![Some(rtf.clone()), None];
        reader.unknown = vec![Some(sample_unknown())];
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent::rtf(rtf))
        );
        assert!(matches!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                payload: ClipboardPayload::Unknown(_)
            })
        ));
    }

    #[test]
    fn platform_capabilities_describe_rich_payload_support() {
        let linux = platform_capabilities(ClipboardPlatform::Linux);
        assert!(linux.text);
        assert!(linux.image);
        assert!(linux.file_list);
        assert!(linux.html);
        assert!(!linux.rtf);
        assert!(linux.image_formats.contains(&"image/png"));
        assert!(linux.rich_text_formats.contains(&"text/html"));

        let unknown = platform_capabilities(ClipboardPlatform::Unknown);
        assert!(!unknown.text);
        assert!(!unknown.image);
        assert!(!unknown.file_list);
        assert!(!unknown.html);
        assert!(!unknown.rtf);
        assert!(unknown.image_formats.is_empty());
    }

    #[test]
    fn normalizes_rgba_image_to_png_payload() {
        let payload = normalize_image(
            ImageData {
                width: 1,
                height: 1,
                bytes: Cow::Borrowed(&[255, 0, 0, 255]),
            },
            ClipboardPlatform::Linux,
        )
        .expect("image should normalize");

        assert_eq!(payload.mime_type, "image/png");
        assert_eq!(payload.width, 1);
        assert_eq!(payload.height, 1);
        assert_eq!(payload.byte_size, payload.bytes.len());
        assert!(payload.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            payload.platform_format.as_deref(),
            Some("arboard:image/png-or-decoded-rgba")
        );
    }

    #[test]
    fn rejects_non_rgba_image_data() {
        let error = normalize_image(
            ImageData {
                width: 1,
                height: 1,
                bytes: Cow::Borrowed(&[255, 0, 0]),
            },
            ClipboardPlatform::Linux,
        )
        .expect_err("non-rgba data should fail");

        assert!(matches!(
            error,
            ClipboardError::Unsupported {
                payload_kind: "image",
                ..
            }
        ));
    }

    #[test]
    fn system_watcher_returns_typed_platform_error_when_unavailable() {
        if let Err(error) = system_watcher(ClipboardWatcherConfig::default()) {
            match error {
                ClipboardError::Unavailable { platform, reason } => {
                    assert_eq!(platform, current_platform());
                    assert!(!reason.is_empty());
                }
                ClipboardError::Occupied { platform } => {
                    assert_eq!(platform, current_platform());
                }
                ClipboardError::Backend { platform, message } => {
                    assert_eq!(platform, current_platform());
                    assert!(!message.is_empty());
                }
                ClipboardError::Unsupported { platform, .. } => {
                    assert_eq!(platform, current_platform());
                }
            }
        }
    }

    fn sample_clipboard_image() -> ClipboardImage {
        ClipboardImage {
            bytes: vec![1, 2, 3, 4],
            mime_type: "image/png".to_owned(),
            width: 1,
            height: 1,
            byte_size: 4,
            platform_format: Some("test:image".to_owned()),
        }
    }

    fn sample_file_list() -> ClipboardFileList {
        ClipboardFileList {
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            byte_size: 20,
            platform_format: Some("test:file-list".to_owned()),
        }
    }

    fn sample_html() -> ClipboardRichText {
        ClipboardRichText {
            bytes: b"<strong>Hello</strong>".to_vec(),
            mime_type: "text/html".to_owned(),
            byte_size: 22,
            plain_text: Some("Hello".to_owned()),
            platform_format: Some("test:html".to_owned()),
        }
    }

    fn sample_rtf() -> ClipboardRichText {
        ClipboardRichText {
            bytes: br"{\rtf1 Hello}".to_vec(),
            mime_type: "text/rtf".to_owned(),
            byte_size: 13,
            plain_text: Some("Hello".to_owned()),
            platform_format: Some("test:rtf".to_owned()),
        }
    }

    fn sample_unknown() -> ClipboardUnknown {
        ClipboardUnknown {
            bytes: Vec::new(),
            mime_type: None,
            byte_size: 42,
            platform_format: "application/x-test".to_owned(),
        }
    }
}
