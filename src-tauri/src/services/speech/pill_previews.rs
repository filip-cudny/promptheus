use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use image::imageops::FilterType;
use image::ImageFormat;

pub const DIR_NAME: &str = "pill-previews";
const MAX_WIDTH: u32 = 528;
const MAX_HEIGHT: u32 = 320;
const THUMBNAIL_SIZE: u32 = 40;

pub fn preview_file_name(data: &str) -> String {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}.png", hasher.finish())
}

pub fn write_preview(dir: &Path, data: &str, media_type: &str) -> io::Result<PathBuf> {
    let path = dir.join(preview_file_name(data));
    if path.exists() {
        return Ok(path);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(io::Error::other)?;
    if media_type == "image/png" {
        let source = image::load_from_memory_with_format(&bytes, ImageFormat::Png).map_err(io::Error::other)?;
        let fitted = if source.width() > MAX_WIDTH || source.height() > MAX_HEIGHT {
            source.thumbnail(MAX_WIDTH, MAX_HEIGHT)
        } else {
            source
        };
        fitted.save_with_format(&path, ImageFormat::Png).map_err(io::Error::other)?;
    } else {
        std::fs::write(&path, bytes)?;
    }
    Ok(path)
}

fn encode_thumbnail(data: &str) -> io::Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(io::Error::other)?;
    let source = image::load_from_memory_with_format(&bytes, ImageFormat::Png).map_err(io::Error::other)?;
    let thumbnail = source.resize_to_fill(THUMBNAIL_SIZE, THUMBNAIL_SIZE, FilterType::Triangle);
    let mut png = Vec::new();
    thumbnail
        .write_to(&mut io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(io::Error::other)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

pub fn thumbnail_data_uri(data: &str, media_type: &str) -> Option<String> {
    if media_type != "image/png" {
        return None;
    }
    match encode_thumbnail(data) {
        Ok(uri) => Some(uri),
        Err(e) => {
            log::debug!("pill thumbnail failed: {e}");
            None
        }
    }
}

fn read_pixel_size(data: &str) -> io::Result<(u32, u32)> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(io::Error::other)?;
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()
        .map_err(io::Error::other)
}

pub fn pixel_size(data: &str) -> Option<(u32, u32)> {
    match read_pixel_size(data) {
        Ok(size) => Some(size),
        Err(e) => {
            log::debug!("pill image size unreadable: {e}");
            None
        }
    }
}

struct KeyedCache<V> {
    entries: Mutex<HashMap<String, V>>,
}

impl<V> Default for KeyedCache<V> {
    fn default() -> Self {
        Self { entries: Mutex::new(HashMap::new()) }
    }
}

impl<V: Clone> KeyedCache<V> {
    fn locked(&self) -> std::sync::MutexGuard<'_, HashMap<String, V>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn refresh(&self, images: &[(String, String)], compute: impl Fn(&str, &str) -> V) {
        let mut current = HashMap::new();
        for (data, media_type) in images {
            let key = preview_file_name(data);
            if current.contains_key(&key) {
                continue;
            }
            let cached = self.locked().get(&key).cloned();
            current.insert(key, cached.unwrap_or_else(|| compute(data, media_type)));
        }
        *self.locked() = current;
    }

    fn get(&self, data: &str) -> Option<V> {
        self.locked().get(&preview_file_name(data)).cloned()
    }
}

#[derive(Default)]
pub struct ThumbnailCache(KeyedCache<Option<String>>);

impl ThumbnailCache {
    pub fn refresh(&self, images: &[(String, String)]) {
        self.0.refresh(images, thumbnail_data_uri);
    }

    pub fn get(&self, data: &str) -> Option<String> {
        self.0.get(data).flatten()
    }
}

#[derive(Default)]
pub struct PixelSizeCache(KeyedCache<Option<(u32, u32)>>);

impl PixelSizeCache {
    pub fn refresh(&self, images: &[(String, String)]) {
        self.0.refresh(images, |data, _| pixel_size(data));
    }

    pub fn get(&self, data: &str) -> Option<(u32, u32)> {
        self.0.get(data).flatten()
    }
}

pub fn sync_previews(dir: &Path, images: &[(String, String)]) -> io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let paths = images
        .iter()
        .map(|(data, media_type)| write_preview(dir, data, media_type))
        .collect::<io::Result<Vec<_>>>()?;
    let keep: HashSet<&PathBuf> = paths.iter().collect();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && !keep.contains(&path) {
            std::fs::remove_file(path)?;
        }
    }
    Ok(paths)
}

pub fn clear_previews(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_base64(width: u32, height: u32, shade: u8) -> String {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([shade, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut io::Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn the_file_name_follows_the_data() {
        assert_eq!(preview_file_name("a"), preview_file_name("a"));
        assert_ne!(preview_file_name("a"), preview_file_name("b"));
        assert_eq!(preview_file_name("a").len(), 20);
    }

    #[test]
    fn a_large_png_is_fitted_into_the_thumbnail_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_preview(dir.path(), &png_base64(2000, 1000, 1), "image/png").unwrap();
        assert_eq!(image::image_dimensions(path).unwrap(), (528, 264));
    }

    #[test]
    fn a_small_png_is_not_upscaled() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_preview(dir.path(), &png_base64(40, 20, 1), "image/png").unwrap();
        assert_eq!(image::image_dimensions(path).unwrap(), (40, 20));
    }

    #[test]
    fn sync_keeps_the_current_set_and_deletes_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let png = |shade| (png_base64(10, 10, shade), "image/png".to_string());
        let (a, b, c) = (png(1), png(2), png(3));

        let first = sync_previews(dir.path(), &[a.clone(), b.clone()]).unwrap();
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|path| path.exists()));

        let second = sync_previews(dir.path(), &[b, c]).unwrap();
        assert!(!first[0].exists());
        assert_eq!(second[0], first[1]);
        assert!(second.iter().all(|path| path.exists()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_png_thumbnail_is_a_40_px_square_data_uri() {
        let uri = thumbnail_data_uri(&png_base64(100, 40, 1), "image/png").unwrap();
        let encoded = uri.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let thumbnail = image::load_from_memory_with_format(&bytes, ImageFormat::Png).unwrap();
        assert_eq!((thumbnail.width(), thumbnail.height()), (40, 40));
    }

    #[test]
    fn a_non_png_image_has_no_thumbnail() {
        assert_eq!(thumbnail_data_uri(&png_base64(10, 10, 1), "image/jpeg"), None);
    }

    #[test]
    fn a_png_that_does_not_decode_has_no_thumbnail() {
        assert_eq!(thumbnail_data_uri("not an image", "image/png"), None);
    }

    #[test]
    fn the_cache_keeps_only_the_current_images() {
        let cache = ThumbnailCache::default();
        let png = |shade| (png_base64(10, 10, shade), "image/png".to_string());
        let (a, b) = (png(1), png(2));

        cache.refresh(&[a.clone(), b.clone()]);
        assert!(cache.get(&a.0).is_some());
        assert!(cache.get(&b.0).is_some());

        cache.refresh(&[b.clone()]);
        assert_eq!(cache.get(&a.0), None);
        assert!(cache.get(&b.0).is_some());
    }

    #[test]
    fn the_pixel_size_comes_from_the_png_header() {
        assert_eq!(pixel_size(&png_base64(120, 40, 1)), Some((120, 40)));
        assert_eq!(pixel_size(&png_base64(7, 300, 1)), Some((7, 300)));
    }

    #[test]
    fn invalid_base64_has_no_pixel_size() {
        assert_eq!(pixel_size("not an image"), None);
    }

    #[test]
    fn the_pixel_size_cache_keeps_only_the_current_images() {
        let cache = PixelSizeCache::default();
        let png = |width| (png_base64(width, 10, 1), "image/png".to_string());
        let (a, b) = (png(10), png(20));

        cache.refresh(&[a.clone(), b.clone()]);
        assert_eq!(cache.get(&a.0), Some((10, 10)));
        assert_eq!(cache.get(&b.0), Some((20, 10)));

        cache.refresh(&[b.clone()]);
        assert_eq!(cache.get(&a.0), None);
        assert_eq!(cache.get(&b.0), Some((20, 10)));
    }

    #[test]
    fn clearing_a_missing_dir_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        clear_previews(&dir.path().join("missing"));
    }
}
