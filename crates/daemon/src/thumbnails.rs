//! The thumbnail service: `dev.frascella.Silica.Thumbnails1` on `/dev/frascella/Silica/Thumbnails1`.
//!
//! Apps and the shell ask for the thumbnail of a file at a pixel size and get the path of a
//! cached PNG. The cache follows the freedesktop thumbnail specification, so other Linux apps
//! share it: `$XDG_CACHE_HOME/thumbnails/{normal,large,x-large,xx-large}/<md5 of the URI>.png`,
//! with the `Thumb::URI` and `Thumb::MTime` text chunks, and `fail/silica-daemon/` for files that
//! could not be rendered.
//!
//! `Icon` is the second entry point: the icon of a file or folder composed at the exact pixel
//! size (see [`crate::icons`], docs/thumbnails.md), preferring a real preview for files.
//!
//! Generators (v1): raster images (PNG, JPEG, GIF first frame, WebP, BMP, TIFF; EXIF
//! orientation) and the first page of a PDF, when `pdftoppm` (poppler) is installed. Anything
//! else, and folders, get an empty path: the client keeps the file-type icon. Rendering runs on
//! a small bounded worker pool, never on the D-Bus thread. The composed folder icon is a later
//! step.

use std::fs::{self, File};
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use crate::icons::Icons;
use zbus::interface;
use zbus::object_server::SignalEmitter;

pub const THUMBNAILS_PATH: &str = "/dev/frascella/Silica/Thumbnails1";

/// The buckets of the specification: folder name and edge in pixels.
pub const BUCKETS: [(&str, u32); 4] = [
    ("normal", 128),
    ("large", 256),
    ("x-large", 512),
    ("xx-large", 1024),
];

/// The name of the folder of failures, under `thumbnails/fail/`.
const FAIL_APP: &str = "silica-daemon";

/// Source files bigger than this are not decoded.
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;

/// How long `pdftoppm` may take.
const PDF_TIMEOUT: Duration = Duration::from_secs(20);

/// `Icon` flag: never answer with a preview of the content, always the composed icon.
pub const ICON_NO_PREVIEW: u32 = 1;

const WORKERS: usize = 3;
const QUEUE: usize = 128;

/// The smallest bucket whose edge is at least `size_px` (the largest one for bigger sizes).
pub fn bucket_for(size_px: u32) -> (&'static str, u32) {
    BUCKETS
        .iter()
        .copied()
        .find(|&(_, edge)| edge >= size_px)
        .unwrap_or(("xx-large", 1024))
}

/// The cache file name of a URI: the MD5 of the URI, in lowercase hex, plus `.png`.
pub fn thumb_name(uri: &str) -> String {
    format!("{:x}.png", md5::compute(uri.as_bytes()))
}

/// `$XDG_CACHE_HOME/thumbnails`, or `$HOME/.cache/thumbnails`.
pub fn cache_root(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let base = env("XDG_CACHE_HOME")
        .filter(|v| v.starts_with('/'))
        .unwrap_or_else(|| format!("{}/.cache", env("HOME").unwrap_or_default()));
    Path::new(&base).join("thumbnails")
}

/// Where the thumbnail of `uri` in `bucket` lives.
pub fn thumb_path(root: &Path, bucket: &str, uri: &str) -> PathBuf {
    root.join(bucket).join(thumb_name(uri))
}

/// Where the failure record of `uri` lives.
pub fn fail_path(root: &Path, uri: &str) -> PathBuf {
    root.join("fail").join(FAIL_APP).join(thumb_name(uri))
}

fn mtime_secs(meta: &fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// The `Thumb::MTime` of a cached PNG (or failure record), when it has one.
fn stored_mtime(path: &Path) -> Option<u64> {
    let decoder = png::Decoder::new(File::open(path).ok()?);
    let reader = decoder.read_info().ok()?;
    reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .find(|chunk| chunk.keyword == "Thumb::MTime")
        .and_then(|chunk| chunk.text.trim().parse().ok())
}

/// A cached file is fresh when its `Thumb::MTime` is the source's modification time.
pub fn is_fresh(cached: &Path, source_mtime: u64) -> bool {
    stored_mtime(cached) == Some(source_mtime)
}

fn percent_decode(text: &str) -> String {
    crate::share::local_path(text)
}

/// The local path of a `file://` URI, None for other schemes.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(percent_decode(uri)))
}

fn write_png(
    path: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    uri: &str,
    mtime: u64,
) -> Option<()> {
    let dir = path.parent()?;
    ensure_dir(dir)?;
    let tmp = dir.join(format!(".{}.{}.tmp", std::process::id(), thumb_name(uri)));
    let result = (|| {
        let file = BufWriter::new(File::create(&tmp).ok()?);
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("Thumb::URI".to_owned(), uri.to_owned())
            .ok()?;
        encoder
            .add_text_chunk("Thumb::MTime".to_owned(), mtime.to_string())
            .ok()?;
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
        fs::rename(&tmp, path).ok()
    })();
    if result.is_none() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn ensure_dir(dir: &Path) -> Option<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .ok()
}

fn record_failure(root: &Path, uri: &str, mtime: u64) {
    let _ = write_png(&fail_path(root, uri), 1, 1, &[0, 0, 0, 0], uri, mtime);
}

fn is_pdf(path: &Path) -> bool {
    let mut magic = [0u8; 5];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && &magic == b"%PDF-"
}

/// Decodes a raster image, applying its EXIF orientation.
fn decode_raster(path: &Path) -> Option<image::DynamicImage> {
    use image::ImageDecoder;
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        img.apply_orientation(orientation);
    }
    Some(img)
}

/// The first page of a PDF, through `pdftoppm`; None if it is missing or fails.
fn render_pdf(path: &Path, edge: u32) -> Option<image::DynamicImage> {
    let dir = std::env::temp_dir().join(format!(
        "silica-thumb-{}-{:x}",
        std::process::id(),
        md5::compute(path.as_os_str().as_encoded_bytes())
    ));
    ensure_dir(&dir)?;
    let prefix = dir.join("page");
    let result = (|| {
        let mut child = Command::new("pdftoppm")
            .args(["-png", "-f", "1", "-l", "1", "-singlefile", "-scale-to"])
            .arg(edge.to_string())
            .arg(path)
            .arg(&prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = std::time::Instant::now() + PDF_TIMEOUT;
        loop {
            match child.try_wait().ok()? {
                Some(status) if status.success() => break,
                Some(_) => return None,
                None if std::time::Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        image::open(dir.join("page.png")).ok()
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// The cached thumbnail of `uri` in the bucket for `size_px`, generating it if it is missing or
/// stale. None when the file has no thumbnail (unsupported, unreadable, or a recorded failure).
pub fn thumbnail(root: &Path, uri: &str, size_px: u32) -> Option<PathBuf> {
    let source = uri_to_path(uri)?;
    let meta = fs::metadata(&source).ok()?;
    if !meta.is_file() || meta.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mtime = mtime_secs(&meta)?;
    let (bucket, edge) = bucket_for(size_px);
    let cached = thumb_path(root, bucket, uri);
    if is_fresh(&cached, mtime) {
        return Some(cached);
    }
    let failed = fail_path(root, uri);
    if is_fresh(&failed, mtime) {
        return None;
    }
    let img = if is_pdf(&source) {
        render_pdf(&source, edge)
    } else {
        decode_raster(&source)
    };
    let Some(img) = img else {
        record_failure(root, uri, mtime);
        return None;
    };
    // Fit in edge x edge, never enlarge.
    let img = if img.width() > edge || img.height() > edge {
        img.thumbnail(edge, edge)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    write_png(
        &cached,
        rgba.width(),
        rgba.height(),
        rgba.as_raw(),
        uri,
        mtime,
    )?;
    let _ = fs::remove_file(&failed);
    Some(cached)
}

/// Forgets the thumbnails of `uri` in every bucket, and its failure record.
pub fn invalidate(root: &Path, uri: &str) {
    for (bucket, _) in BUCKETS {
        let _ = fs::remove_file(thumb_path(root, bucket, uri));
    }
    let _ = fs::remove_file(fail_path(root, uri));
}

/// What a job asks for.
enum Request {
    Thumbnail {
        uri: String,
        size_px: u32,
    },
    Icon {
        uri: String,
        size_px: u32,
        scale: f64,
        flags: u32,
    },
}

type Answer = Option<(PathBuf, bool)>;

struct Job {
    request: Request,
    reply: async_channel::Sender<Answer>,
}

/// The icon of `uri`: a preview when the file has one (and it is wanted), else the composed
/// icon. The flag says which.
pub(crate) fn icon_job(
    root: &Path,
    icons: Option<&Icons>,
    uri: &str,
    size_px: u32,
    scale: f64,
    flags: u32,
) -> Answer {
    let path = uri_to_path(uri)?;
    let meta = fs::metadata(&path).ok()?;
    if meta.is_file()
        && flags & ICON_NO_PREVIEW == 0
        && let Some(preview) = thumbnail(root, uri, size_px)
    {
        return Some((preview, true));
    }
    let pt = if scale.is_finite() && scale >= 0.25 {
        size_px as f32 / scale as f32
    } else {
        size_px as f32
    };
    icons?.icon(&path, size_px, pt).map(|p| (p, false))
}

/// A fixed set of threads that render thumbnails.
struct Pool {
    jobs: SyncSender<Job>,
}

impl Pool {
    fn new(root: PathBuf, icons: Option<Arc<Icons>>) -> Self {
        let (jobs, queue) = mpsc::sync_channel::<Job>(QUEUE);
        let queue = Arc::new(Mutex::new(queue));
        for n in 0..WORKERS {
            let queue = Arc::clone(&queue);
            let root = root.clone();
            let icons = icons.clone();
            let _ = std::thread::Builder::new()
                .name(format!("thumbnails-{n}"))
                .spawn(move || {
                    loop {
                        let job = match queue.lock() {
                            Ok(rx) => rx.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = job else { return };
                        let out = match &job.request {
                            Request::Thumbnail { uri, size_px } => {
                                thumbnail(&root, uri, *size_px).map(|p| (p, true))
                            }
                            Request::Icon {
                                uri,
                                size_px,
                                scale,
                                flags,
                            } => icon_job(&root, icons.as_deref(), uri, *size_px, *scale, *flags),
                        };
                        let _ = job.reply.send_blocking(out);
                    }
                });
        }
        Self { jobs }
    }

    /// Queues a job; None when the queue is full or the pool is gone.
    fn submit(&self, request: Request) -> Option<async_channel::Receiver<Answer>> {
        let (reply, answer) = async_channel::bounded(1);
        match self.jobs.try_send(Job { request, reply }) {
            Ok(()) => Some(answer),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => None,
        }
    }
}

pub struct Thumbnails {
    root: PathBuf,
    pool: Pool,
}

impl Thumbnails {
    pub fn new(root: PathBuf) -> Self {
        Self::with_icons(root, None)
    }

    pub fn with_icons(root: PathBuf, icons: Option<Arc<Icons>>) -> Self {
        let pool = Pool::new(root.clone(), icons);
        Self { root, pool }
    }

    /// The service with the cache of the current user and the system assets.
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok();
        Self::with_icons(cache_root(&env), Icons::load(&env).map(Arc::new))
    }
}

#[interface(name = "dev.frascella.Silica.Thumbnails1")]
impl Thumbnails {
    /// The path of the cached PNG for `uri` at `size_px` (the bucket is the smallest edge that
    /// fits), or "" when the file has no thumbnail. Rendering happens off the bus thread; the
    /// `Ready` signal follows a successful render.
    async fn thumbnail(
        &self,
        uri: String,
        size_px: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
        let request = Request::Thumbnail {
            uri: uri.clone(),
            size_px,
        };
        let Some(answer) = self.pool.submit(request) else {
            return String::new();
        };
        let Ok(Some((path, _))) = answer.recv().await else {
            return String::new();
        };
        let path = path.to_string_lossy().into_owned();
        let _ = Self::ready(&emitter, &uri, size_px, &path).await;
        path
    }

    /// The icon of a file or folder for `uri` at `size_px` pixels on a display of `scale`
    /// (the icon side in points is `size_px / scale`: the symbol weight and the extension label
    /// depend on it). Returns the path of a PNG of exactly `size_px` pixels, composed from the
    /// system layers (folders: with their colour and symbol; files: the handling app's icon or
    /// the symbol of their kind, and the extension), or, for a file that has one and unless
    /// `flags` has `NO_PREVIEW` (1), the preview of its content (a cached thumbnail of up to
    /// the bucket size) with `true`. "" when nothing can be made (the client keeps its own
    /// icon). Rendering happens off the bus thread; `Ready` follows a success.
    async fn icon(
        &self,
        uri: String,
        size_px: u32,
        scale: f64,
        flags: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> (String, bool) {
        let request = Request::Icon {
            uri: uri.clone(),
            size_px,
            scale,
            flags,
        };
        let Some(answer) = self.pool.submit(request) else {
            return (String::new(), false);
        };
        let Ok(Some((path, preview))) = answer.recv().await else {
            return (String::new(), false);
        };
        let path = path.to_string_lossy().into_owned();
        let _ = Self::ready(&emitter, &uri, size_px, &path).await;
        (path, preview)
    }

    /// Forgets the cached thumbnails of these files (after they changed).
    fn invalidate(&self, uris: Vec<String>) {
        for uri in &uris {
            invalidate(&self.root, uri);
        }
    }

    /// A thumbnail was rendered or confirmed.
    #[zbus(signal)]
    async fn ready(
        emitter: &SignalEmitter<'_>,
        uri: &str,
        size_px: u32,
        path: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("thumbs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cache_names_follow_the_specification() {
        // MD5 of the URI, as `md5sum` prints it.
        assert_eq!(
            thumb_name("file:///home/jens/photo/me.jpg"),
            "6a6e2356333b14814bde41eb5adb7653.png"
        );
        let env = |k: &str| match k {
            "XDG_CACHE_HOME" => Some("/c".to_owned()),
            _ => Some("/h".to_owned()),
        };
        assert_eq!(cache_root(&env), PathBuf::from("/c/thumbnails"));
        let home = |k: &str| (k == "HOME").then(|| "/h".to_owned());
        assert_eq!(cache_root(&home), PathBuf::from("/h/.cache/thumbnails"));
        assert_eq!(
            thumb_path(Path::new("/r"), "large", "file:///a"),
            Path::new("/r/large").join(thumb_name("file:///a"))
        );
        assert!(fail_path(Path::new("/r"), "file:///a").starts_with("/r/fail/silica-daemon"));
    }

    #[test]
    fn the_smallest_bucket_that_fits_is_chosen() {
        assert_eq!(bucket_for(0).0, "normal");
        assert_eq!(bucket_for(64), ("normal", 128));
        assert_eq!(bucket_for(128).0, "normal");
        assert_eq!(bucket_for(129), ("large", 256));
        assert_eq!(bucket_for(300).0, "x-large");
        assert_eq!(bucket_for(1024).0, "xx-large");
        assert_eq!(bucket_for(5000), ("xx-large", 1024));
    }

    #[test]
    fn uris_map_to_paths() {
        assert_eq!(
            uri_to_path("file:///home/a/My%20File.png"),
            Some(PathBuf::from("/home/a/My File.png"))
        );
        assert_eq!(uri_to_path("https://x/y.png"), None);
        assert_eq!(uri_to_path("file://host/y.png"), None);
    }

    #[test]
    fn a_png_thumbnail_is_generated_cached_and_invalidated_by_mtime() {
        let dir = tmp("gen");
        let src = dir.join("pic one.png");
        image::RgbaImage::from_pixel(400, 200, image::Rgba([200, 30, 30, 255]))
            .save(&src)
            .unwrap();
        let uri = format!("file://{}", src.display().to_string().replace(' ', "%20"));
        let root = dir.join("cache");

        let out = thumbnail(&root, &uri, 100).unwrap();
        assert_eq!(out, thumb_path(&root, "normal", &uri));
        let img = image::open(&out).unwrap();
        assert_eq!((img.width(), img.height()), (128, 64));
        let meta = fs::metadata(&src).unwrap();
        assert!(is_fresh(&out, mtime_secs(&meta).unwrap()));
        assert!(!is_fresh(&out, mtime_secs(&meta).unwrap() + 1));

        // A small image is not enlarged, and a bigger request uses a bigger bucket.
        let big = thumbnail(&root, &uri, 200).unwrap();
        assert_eq!(big, thumb_path(&root, "large", &uri));
        assert_eq!(image::open(&big).unwrap().width(), 256);

        // A stale entry is regenerated: pretend the cached one was made for another mtime.
        let old = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
        write_png(&out, 2, 2, old.as_raw(), &uri, 1).unwrap();
        let again = thumbnail(&root, &uri, 100).unwrap();
        assert_eq!(image::open(&again).unwrap().width(), 128);

        invalidate(&root, &uri);
        assert!(!out.exists() && !big.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_files_fail_once_and_are_remembered() {
        let dir = tmp("fail");
        let src = dir.join("notes.txt");
        fs::write(&src, "hello").unwrap();
        let uri = format!("file://{}", src.display());
        let root = dir.join("cache");
        assert_eq!(thumbnail(&root, &uri, 128), None);
        assert!(fail_path(&root, &uri).exists());
        assert_eq!(thumbnail(&root, &uri, 128), None);
        // Folders and missing files have no thumbnail and leave no record.
        let folder = format!("file://{}", dir.display());
        assert_eq!(thumbnail(&root, &folder, 128), None);
        assert!(!fail_path(&root, &folder).exists());
        assert_eq!(thumbnail(&root, "file:///nonexistent/x.png", 128), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
