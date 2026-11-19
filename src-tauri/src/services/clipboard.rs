use base64::Engine;
use image::ImageBuffer;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard unavailable: {0}")]
    Unavailable(String),

    #[error("clipboard error: {0}")]
    Access(String),

    #[error("image conversion error: {0}")]
    ImageConversion(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardFingerprint {
    Text(u64),
    Image(u64),
    Files(u64),
    Empty,
    Unreadable,
}

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_from(
    text: impl FnOnce() -> Result<String, arboard::Error>,
    image: impl FnOnce() -> Result<arboard::ImageData<'static>, arboard::Error>,
    files: impl FnOnce() -> Result<Vec<PathBuf>, arboard::Error>,
) -> ClipboardFingerprint {
    match text() {
        Ok(text) if !text.is_empty() => return ClipboardFingerprint::Text(hash_of(text)),
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => {}
        Err(_) => return ClipboardFingerprint::Unreadable,
    }
    match image() {
        Ok(image) => {
            return ClipboardFingerprint::Image(hash_of((image.width, image.height, &*image.bytes)))
        }
        Err(arboard::Error::ContentNotAvailable) => {}
        Err(_) => return ClipboardFingerprint::Unreadable,
    }
    match files() {
        Ok(files) if !files.is_empty() => ClipboardFingerprint::Files(hash_of(files)),
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => ClipboardFingerprint::Empty,
        Err(_) => ClipboardFingerprint::Unreadable,
    }
}

#[derive(Clone)]
pub struct ClipboardService {
    app: Option<AppHandle>,
}

impl ClipboardService {
    pub fn new(app: AppHandle) -> Self {
        if let Err(e) = arboard::Clipboard::new() {
            log::warn!("clipboard unavailable at startup: {e}");
        }
        Self { app: Some(app) }
    }

    pub fn get_text(&self) -> Result<String, ClipboardError> {
        let trimmed = self.get_text_raw()?.trim().to_string();
        if trimmed.is_empty() {
            return Err(ClipboardError::Unavailable("clipboard is empty".into()));
        }
        Ok(trimmed)
    }

    pub fn get_text_raw(&self) -> Result<String, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ClipboardError::Access(e.to_string()))?;
        clipboard
            .get_text()
            .map_err(|e| ClipboardError::Unavailable(e.to_string()))
    }

    pub fn set_text(&self, content: &str) -> Result<(), ClipboardError> {
        if let Some(app) = &self.app {
            return write_text(app, content);
        }

        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ClipboardError::Access(e.to_string()))?;
        clipboard
            .set_text(content)
            .map_err(|e| ClipboardError::Access(e.to_string()))
    }

    /// Blocks on clipboard reads: call from `spawn_blocking`.
    pub fn fingerprint(&self) -> ClipboardFingerprint {
        let Ok(clipboard) = arboard::Clipboard::new() else {
            return ClipboardFingerprint::Unreadable;
        };
        let clipboard = std::cell::RefCell::new(clipboard);
        fingerprint_from(
            || clipboard.borrow_mut().get_text(),
            || clipboard.borrow_mut().get_image(),
            || clipboard.borrow_mut().get().file_list(),
        )
    }

    pub fn has_image(&self) -> bool {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return false;
        };
        clipboard.get_image().is_ok()
    }

    pub fn has_file_list(&self) -> bool {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return false;
        };
        clipboard
            .get()
            .file_list()
            .is_ok_and(|files| !files.is_empty())
    }

    pub fn get_image_base64(&self) -> Result<(String, String), ClipboardError> {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ClipboardError::Access(e.to_string()))?;

        let image_data = clipboard
            .get_image()
            .map_err(|e| ClipboardError::Unavailable(e.to_string()))?;

        let rgba: ImageBuffer<image::Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(
            image_data.width as u32,
            image_data.height as u32,
            image_data.bytes.into_owned(),
        )
        .ok_or_else(|| {
            ClipboardError::ImageConversion("failed to create image buffer".into())
        })?;

        let mut png_bytes = Cursor::new(Vec::new());
        rgba.write_to(&mut png_bytes, image::ImageFormat::Png)
            .map_err(|e| ClipboardError::ImageConversion(e.to_string()))?;

        let base64_data =
            base64::engine::general_purpose::STANDARD.encode(png_bytes.into_inner());

        Ok((base64_data, "image/png".into()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKind {
    Nothing,
    Image,
    Text,
}

/// Files are not pasteable items yet, even when the clipboard also offers an image or text.
pub fn paste_kind(has_file_list: bool, has_image: bool) -> PasteKind {
    if has_file_list {
        PasteKind::Nothing
    } else if has_image {
        PasteKind::Image
    } else {
        PasteKind::Text
    }
}

pub enum PastedItem {
    Text(String),
    Image(String, String),
}

fn read_item_blocking(clipboard: &ClipboardService) -> Result<Option<PastedItem>, ClipboardError> {
    match paste_kind(clipboard.has_file_list(), clipboard.has_image()) {
        PasteKind::Nothing => Ok(None),
        PasteKind::Image => clipboard
            .get_image_base64()
            .map(|(data, media_type)| Some(PastedItem::Image(data, media_type))),
        PasteKind::Text => clipboard.get_text().map(|text| Some(PastedItem::Text(text))),
    }
}

/// `None` when the clipboard holds files. Reads off the async runtime.
pub async fn read_clipboard_item(
    clipboard: &ClipboardService,
) -> Result<Option<PastedItem>, ClipboardError> {
    let clipboard = clipboard.clone();
    tokio::task::spawn_blocking(move || read_item_blocking(&clipboard))
        .await
        .map_err(|e| ClipboardError::Access(e.to_string()))?
}

pub fn write_text(app: &AppHandle, content: &str) -> Result<(), ClipboardError> {
    #[cfg(target_os = "linux")]
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        match set_text_subprocess(content) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("clipboard subprocess failed: {e}");
            }
        }
    }

    app.clipboard()
        .write_text(content)
        .map_err(|e| ClipboardError::Access(e.to_string()))
}

#[cfg(target_os = "linux")]
fn set_text_subprocess(content: &str) -> Result<(), ClipboardError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let cmd = "wl-copy";

    let mut child = Command::new(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ClipboardError::Access(format!("{cmd}: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(content.as_bytes())
            .map_err(|e| ClipboardError::Access(e.to_string()))?;
    }

    let status = child
        .wait()
        .map_err(|e| ClipboardError::Access(e.to_string()))?;

    if !status.success() {
        return Err(ClipboardError::Access(format!(
            "{cmd} exited with {}",
            status
        )));
    }

    Ok(())
}

#[cfg(test)]
impl ClipboardService {
    pub fn without_app() -> Self {
        Self { app: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arboard::{Error as E, ImageData};
    use std::borrow::Cow;

    fn img(bytes: &[u8]) -> Result<ImageData<'static>, E> {
        Ok(ImageData { width: 1, height: 1, bytes: Cow::Owned(bytes.to_vec()) })
    }

    fn absent<T>() -> Result<T, E> {
        Err(E::ContentNotAvailable)
    }

    #[test]
    fn fingerprint_text_image_and_files_give_distinct_variants() {
        let text = fingerprint_from(|| Ok("a".into()), absent, absent);
        let image = fingerprint_from(absent, || img(&[1]), absent);
        let files = fingerprint_from(absent, absent, || Ok(vec![PathBuf::from("/a")]));
        assert!(matches!(text, ClipboardFingerprint::Text(_)));
        assert!(matches!(image, ClipboardFingerprint::Image(_)));
        assert!(matches!(files, ClipboardFingerprint::Files(_)));
    }

    #[test]
    fn fingerprint_equal_text_is_equal_and_different_text_differs() {
        let a = || fingerprint_from(|| Ok("same".into()), absent, absent);
        assert_eq!(a(), a());
        assert_ne!(a(), fingerprint_from(|| Ok("other".into()), absent, absent));
    }

    #[test]
    fn fingerprint_text_present_does_not_read_image() {
        let result = fingerprint_from(
            || Ok("a".into()),
            || panic!("image read"),
            || panic!("files read"),
        );
        assert!(matches!(result, ClipboardFingerprint::Text(_)));
    }

    #[test]
    fn fingerprint_everything_unavailable_is_empty() {
        assert_eq!(
            fingerprint_from(absent, absent, absent),
            ClipboardFingerprint::Empty
        );
    }

    #[test]
    fn fingerprint_read_error_is_unreadable() {
        let result = fingerprint_from(absent, || Err(E::ClipboardOccupied), absent);
        assert_eq!(result, ClipboardFingerprint::Unreadable);
    }

    #[test]
    fn a_file_list_blocks_paste_even_with_an_image_or_text() {
        assert_eq!(paste_kind(true, true), PasteKind::Nothing);
        assert_eq!(paste_kind(true, false), PasteKind::Nothing);
    }

    #[test]
    fn paste_prefers_an_image_over_text() {
        assert_eq!(paste_kind(false, true), PasteKind::Image);
        assert_eq!(paste_kind(false, false), PasteKind::Text);
    }

    #[test]
    fn error_display() {
        let unavailable = ClipboardError::Unavailable("empty".into());
        assert_eq!(unavailable.to_string(), "clipboard unavailable: empty");

        let access = ClipboardError::Access("denied".into());
        assert_eq!(access.to_string(), "clipboard error: denied");

        let conversion = ClipboardError::ImageConversion("bad format".into());
        assert_eq!(
            conversion.to_string(),
            "image conversion error: bad format"
        );
    }
}
