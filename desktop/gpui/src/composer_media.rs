//! macOS representations that GPUI's clipboard and filename-only drops omit.
//! The app captures temporary bytes during the drop and opens ordinary
//! files while its grant is live. Copies, promises and URL-only browser
//! images get a dim chip while they are loading.
use std::path::PathBuf;
use futures::channel::oneshot;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Owns a completed handover even while it sits unread in a channel. If a
/// timeout wins concurrently, dropping the queued reply deletes its copies.
#[derive(Default)]
pub(crate) struct PreparedPictures { pub(crate) paths: Vec<PathBuf> }
impl Drop for PreparedPictures {
    fn drop(&mut self) { for path in &self.paths { crate::composer::unstage(path); } }
}
struct Pending {
    rx: oneshot::Receiver<Result<PreparedPictures, String>>,
    #[cfg(target_os = "macos")]
    _promise: Option<PromiseLease>,
}
static PENDING: LazyLock<Mutex<HashMap<PathBuf, Pending>>> = LazyLock::new(Default::default);

pub(crate) async fn ready(path: &std::path::Path) -> Result<PreparedPictures, String> {
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).remove(path);
    match pending {
        Some(mut pending) => (&mut pending.rx).await.unwrap_or_else(|_| Err("The picture source closed before it finished".into())),
        None => Ok(PreparedPictures { paths: vec![path.to_owned()] }),
    }
}

pub(crate) fn install(cx: &mut gpui_kit::App) {
    #[cfg(target_os = "macos")]
    cx.observe_new::<gpui_kit::base::Root>(|_, window, _| {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Some(window) = window {
            if let Ok(handle) = HasWindowHandle::window_handle(window) {
                if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                    // SAFETY: the new window's live NSView, on the main thread.
                    unsafe { tb_composer_install(handle.ns_view.as_ptr()); }
                }
            }
        }
    }).detach();
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}

pub(crate) fn read(drag: bool) -> Option<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        let mut captured = Captured::default();
        // SAFETY: read calls collect only before returning. Promises get
        // their own owned callback context, not this stack pointer.
        let taken = unsafe { tb_composer_read(drag, collect, &mut captured as *mut _ as *mut _) };
        taken.then_some(captured.paths)
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = drag; None }
}

#[cfg(target_os = "macos")]
type Sink = extern "C" fn(*mut std::ffi::c_void, i32, *const std::ffi::c_char, *const std::ffi::c_void, usize) -> bool;
#[cfg(target_os = "macos")]
extern "C" {
    fn tb_composer_install(view: *mut std::ffi::c_void);
    fn tb_composer_read(drag: bool, sink: Sink, context: *mut std::ffi::c_void) -> bool;
    fn tb_composer_promise(object: *const std::ffi::c_void, destination: *const std::ffi::c_char, sink: Sink, context: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct Captured {
    paths: Vec<PathBuf>,
    // AppKit requires all receivers in a drop to share one destination.
    promise_folder: Option<std::sync::Arc<tempfile::TempDir>>,
}

#[cfg(target_os = "macos")]
extern "C" fn collect(context: *mut std::ffi::c_void, kind: i32, name: *const std::ffi::c_char, bytes: *const std::ffi::c_void, len: usize) -> bool {
    use crate::composer::{keep_picture, paste_base, stage_private};
    use std::ffi::{CStr, CString};
    // SAFETY: read owns Captured for this synchronous call. AppKit supplies
    // valid UTF-8 strings and byte buffers for the duration of the callback.
    let captured = unsafe { &mut *context.cast::<Captured>() };
    let name = if name.is_null() { String::new() } else { unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned() };
    let path = match kind {
        0 => Some(keep_picture(PathBuf::from(name))),
        5 => load_file(std::path::Path::new(&name)),
        1 if len > 0 => {
            let ext = match name.to_ascii_lowercase().as_str() {
                "png" => "png", "jpeg" | "jpg" => "jpeg", "heic" => "heic", "heif" => "heif", "gif" => "gif", "webp" => "webp", _ => "png",
            };
            let data = unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), len) };
            stage_private(&paste_base(), &format!("pasted.{ext}"), data)
        }
        2 => load_url(name),
        3 => {
            if captured.promise_folder.is_none() {
                captured.promise_folder = crate::composer::private_folder(&paste_base()).map(std::sync::Arc::new);
            }
            let Some(source) = captured.promise_folder.clone() else { return false };
            if let Some(path) = stage_private(&paste_base(), "promised.png", b"") {
                let (id, pending) = register_promise(path.clone(), source.clone());
                PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), pending);
                let folder = CString::new(source.path().to_string_lossy().as_bytes()).unwrap();
                unsafe { tb_composer_promise(bytes, folder.as_ptr(), promised, id as *mut _); }
                Some(path)
            } else { None }
        }
        _ => None,
    };
    if let Some(path) = path { captured.paths.push(path); true } else { false }
}

#[cfg(target_os = "macos")]
fn load_file(path: &std::path::Path) -> Option<PathBuf> {
    // Open while AppKit's grant is live. Only a temporary source needs its
    // snapshot completed before the drop returns; ordinary files may live
    // on a slow share, so retain the descriptor and copy behind a dim chip.
    let file = threadbear_core::open_policy::local_open(path).ok()?;
    if !file.metadata().ok()?.is_file() { return None; }
    if crate::composer::transient_picture(path) {
        return crate::composer::snapshot_picture(&file, path).ok();
    }
    snapshot_later(file, path.to_owned())
}

#[cfg(target_os = "macos")]
fn snapshot_later(file: std::fs::File, original: PathBuf) -> Option<PathBuf> {
    let path = crate::composer::stage_private(&crate::composer::paste_base(), "file.png", b"")?;
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), Pending { rx, _promise: None });
    std::thread::spawn(move || {
        let result = crate::composer::snapshot_picture(&file, &original)
            .map(|kept| PreparedPictures { paths: vec![kept] })
            .map_err(|error| error.to_string());
        // An unread reply owns its snapshot too, including after a timeout.
        let _ = tx.send(result);
    });
    Some(path)
}

#[cfg(target_os = "macos")]
struct Promise {
    path: PathBuf,
    prepared: PreparedPictures,
    tx: Option<oneshot::Sender<Result<PreparedPictures, String>>>,
    _source: std::sync::Arc<tempfile::TempDir>,
}
#[cfg(target_os = "macos")]
static PROMISES: LazyLock<Mutex<HashMap<usize, Promise>>> = LazyLock::new(Default::default);
#[cfg(target_os = "macos")]
static NEXT_PROMISE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// The waiting future owns receipt, including any partial snapshots. A
/// timeout or closed window drops it even if the source never calls back.
#[cfg(target_os = "macos")]
struct PromiseLease(usize);
#[cfg(target_os = "macos")]
impl Drop for PromiseLease {
    fn drop(&mut self) {
        let promise = PROMISES.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
        drop(promise); // release the lock before removing private files
    }
}

#[cfg(target_os = "macos")]
fn register_promise(path: PathBuf, source: std::sync::Arc<tempfile::TempDir>) -> (usize, Pending) {
    let id = NEXT_PROMISE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    PROMISES.lock().unwrap_or_else(|e| e.into_inner()).insert(id, Promise { path, prepared: PreparedPictures::default(), tx: Some(tx), _source: source });
    (id, Pending { rx, _promise: Some(PromiseLease(id)) })
}

#[cfg(target_os = "macos")]
extern "C" fn promised(context: *mut std::ffi::c_void, kind: i32, name: *const std::ffi::c_char, _: *const std::ffi::c_void, _: usize) -> bool {
    // This opaque value is an ID, never a dereferenced pointer. AppKit can
    // deliver after cancellation; a removed ID simply ignores that callback.
    let id = context as usize;
    if kind == 4 {
        let promise = PROMISES.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        let Some(mut promise) = promise else { return false };
        let prepared = std::mem::take(&mut promise.prepared);
        let result = if prepared.paths.is_empty() { Err("Couldn't read the promised picture".into()) } else { Ok(prepared) };
        if let Some(tx) = promise.tx.take() { let _ = tx.send(result); }
        return true;
    }
    if kind == 6 && !name.is_null() {
        // SAFETY: AppKit's file path is valid UTF-8 for this reader callback.
        let original = PathBuf::from(unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned());
        return retain_promised(id, original, crate::composer::keep_picture);
    }
    false
}

#[cfg(target_os = "macos")]
fn retain_promised(id: usize, original: PathBuf, keep: impl FnOnce(PathBuf) -> PathBuf) -> bool {
    let placeholder = PROMISES.lock().unwrap_or_else(|e| e.into_inner()).get(&id).map(|p| p.path.clone());
    let Some(placeholder) = placeholder else { return false };
    if !crate::composer::is_staged(&placeholder) { return false; }
    // No registry lock spans IO: a slow source mustn't hold up the main
    // thread's cancellation or the registration of a different drop.
    let kept = keep(original.clone());
    if kept == original { return false; }
    let mut copy = PreparedPictures { paths: vec![kept] };
    let mut promises = PROMISES.lock().unwrap_or_else(|e| e.into_inner());
    let accepted = if let Some(promise) = promises.get_mut(&id) {
        promise.prepared.paths.append(&mut copy.paths);
        true
    } else { false };
    drop(promises);
    // If cancellation won during the copy, its unclaimed snapshot goes now.
    accepted
}

#[cfg(target_os = "macos")]
fn load_url(url: String) -> Option<PathBuf> {
    use crate::composer::{paste_base, stage_private};
    let path = stage_private(&paste_base(), "browser.png", b"")?;
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), Pending { rx, _promise: None });
    let destination = path.clone();
    #[cfg(feature = "automation")]
    let fixture = fixture_address(&url);
    std::thread::spawn(move || {
        let result = (|| {
            let resolver: crate::composer_url::PublicResolver = Default::default();
            #[cfg(feature = "automation")]
            let resolver = resolver.allow_fixture(fixture);
            let agent = crate::composer_url::agent(resolver);
            let mut response = agent.get(&url).call().map_err(|e| e.to_string())?;
            let data = response.body_mut().with_config().limit(25 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
            // Sniff the bytes before handing them on. A webpage's URL must
            // fail as a picture, rather than importing HTML under a PNG name.
            image::guess_format(&data).map_err(|_| "That link didn't return a supported picture".to_string())?;
            crate::composer::write_staged(&destination, &data).map_err(|e| e.to_string())
        })();
        let _ = tx.send(result.map(|()| PreparedPictures { paths: vec![destination] }));
    });
    Some(path)
}

#[cfg(all(feature = "automation", target_os = "macos"))]
pub(crate) fn fixture_begin(kind: &str, path: &std::path::Path) -> Result<(), String> {
    use std::ffi::CString;
    extern "C" { fn tb_composer_fixture_begin(kind: *const std::ffi::c_char, path: *const std::ffi::c_char, ended: extern "C" fn()); }
    fixture_finished();
    let permitted = (kind == "url-fixture").then(|| path.to_string_lossy().into_owned());
    let (kind, path) = (CString::new(kind).map_err(|e| e.to_string())?, CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?);
    unsafe { tb_composer_fixture_begin(kind.as_ptr(), path.as_ptr(), fixture_finished); }
    *FIXTURE_URL.lock().unwrap() = permitted;
    Ok(())
}
#[cfg(all(feature = "automation", target_os = "macos"))]
static FIXTURE_URL: Mutex<Option<String>> = Mutex::new(None);
#[cfg(all(feature = "automation", target_os = "macos"))]
extern "C" fn fixture_finished() {
    *FIXTURE_URL.lock().unwrap() = None;
}
#[cfg(all(feature = "automation", target_os = "macos"))]
fn fixture_address(url: &str) -> Option<std::net::SocketAddr> {
    let fixture = FIXTURE_URL.lock().unwrap();
    if fixture.as_deref() != Some(url) { return None; }
    let uri: ureq::http::Uri = url.parse().ok()?;
    let address: std::net::SocketAddr = uri.authority()?.as_str().parse().ok()?;
    address.ip().is_loopback().then_some(address)
}
#[cfg(all(feature = "automation", target_os = "macos"))]
pub(crate) fn fixture_end() {
    extern "C" { fn tb_composer_fixture_end(); }
    unsafe { tb_composer_fixture_end(); }
}
#[cfg(all(feature = "automation", target_os = "macos"))]
pub(crate) fn fixture_drag(window: &gpui_kit::Window, phase: i32, vanish: Option<&std::path::Path>) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    extern "C" { fn tb_composer_fixture_drag(view: *mut std::ffi::c_void, phase: i32, vanish: *const std::ffi::c_char); }
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return Err("no AppKit window".into()) };
    let vanish = vanish.map(|p| std::ffi::CString::new(p.to_string_lossy().as_bytes()).unwrap());
    unsafe { tb_composer_fixture_drag(handle.ns_view.as_ptr(), phase, vanish.as_ref().map_or(std::ptr::null(), |p| p.as_ptr())); }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn an_open_file_copy_preserves_its_error() {
        let file = tempfile::tempfile().unwrap();
        let path = super::snapshot_later(file, std::path::PathBuf::new()).unwrap();
        let error = futures::executor::block_on(super::ready(&path)).err().unwrap();
        assert!(error.contains("filename"), "{error}");
        crate::composer::unstage(&path);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_open_file_can_be_copied_after_its_name_disappears() {
        let folder = tempfile::tempdir().unwrap();
        let original = folder.path().join("original.png");
        std::fs::write(&original, b"the original image bytes").unwrap();
        let file = threadbear_core::open_policy::local_open(&original).unwrap();
        std::fs::remove_file(&original).unwrap();
        let path = super::snapshot_later(file, original).unwrap();
        let pictures = futures::executor::block_on(super::ready(&path)).unwrap();
        assert_eq!(std::fs::read(&pictures.paths[0]).unwrap(), b"the original image bytes");
        drop(pictures);
        crate::composer::unstage(&path);
    }

    #[cfg(all(feature = "automation", target_os = "macos"))]
    #[test]
    fn ending_a_fixture_clears_its_url_exception() {
        let url = "http://127.0.0.1:12345/picture.png";
        *super::FIXTURE_URL.lock().unwrap() = Some(url.into());
        assert!(super::fixture_address(url).is_some());
        super::fixture_finished();
        assert!(super::fixture_address(url).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cancellation_during_a_copy_does_not_block_or_keep_the_copy() {
        let base = tempfile::tempdir().unwrap();
        let original = base.path().join("original.png");
        std::fs::write(&original, b"the user's original picture").unwrap();
        let source = std::sync::Arc::new(crate::composer::private_folder(base.path()).unwrap());
        let path = crate::composer::stage_private(base.path(), "promise.png", b"").unwrap();
        let (id, pending) = super::register_promise(path.clone(), source);
        let mut copied = None;
        assert!(!super::retain_promised(id, original.clone(), |file| {
            assert!(super::PROMISES.try_lock().is_ok(), "a copy held the promise registry lock");
            drop(pending); // cancellation between checking the ID and finishing IO
            let kept = crate::composer::keep_picture(file);
            copied = Some(kept.clone());
            kept
        }));
        assert!(!copied.unwrap().exists());
        assert!(original.exists());
        crate::composer::unstage(&path);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_dragged_url_never_fetches_a_loopback_service() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/private.png", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while std::time::Instant::now() < until {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request);
                    let picture = include_bytes!("../../../features/support/sample.png");
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", picture.len()).unwrap();
                    stream.write_all(picture).unwrap();
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            false
        });
        let path = super::load_url(url).unwrap();
        let result = futures::executor::block_on(super::ready(&path));
        let fetched = server.join().unwrap();
        drop(result);
        crate::composer::unstage(&path);
        assert!(!fetched, "dragging an image made a GET to a private service");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cancelling_a_stalled_promise_removes_partial_pictures() {
        use futures::FutureExt;
        use std::ffi::CString;
        let base = tempfile::tempdir().unwrap();
        let source = std::sync::Arc::new(crate::composer::private_folder(base.path()).unwrap());
        let folder = source.path().to_owned();
        let original = folder.join("first.png");
        std::fs::write(&original, b"part of a promised bundle").unwrap();
        let path = crate::composer::stage_private(base.path(), "promise.png", b"").unwrap();
        let (id, pending) = super::register_promise(path.clone(), source);
        super::PENDING.lock().unwrap().insert(path.clone(), pending);
        let name = CString::new(original.to_str().unwrap()).unwrap();
        super::promised(id as *mut _, 6, name.as_ptr(), std::ptr::null(), 0);
        let snapshots = super::PROMISES.lock().unwrap()[&id].prepared.paths.clone();
        assert_eq!(snapshots.len(), 1);
        let mut ready = Box::pin(super::ready(&path));
        assert!(ready.as_mut().now_or_never().is_none());
        drop(ready); // the composer's timer wins, or its window closes
        let removed = !folder.exists() && snapshots.iter().all(|p| !p.exists());
        // Complete the fake source afterwards, also checking a late callback.
        super::promised(id as *mut _, 6, name.as_ptr(), std::ptr::null(), 0);
        super::promised(id as *mut _, 4, std::ptr::null(), std::ptr::null(), 0);
        crate::composer::unstage(&path);
        assert!(removed, "a cancelled receiver kept its source folder or partial copies");
    }

    #[test]
    fn an_unread_completion_removes_its_snapshots() {
        let base = tempfile::tempdir().unwrap();
        let path = crate::composer::stage_private(base.path(), "promise.png", b"private picture").unwrap();
        let (tx, rx) = futures::channel::oneshot::channel();
        tx.send(super::PreparedPictures { paths: vec![path.clone()] }).ok().unwrap();
        assert!(path.exists());
        drop(rx); // completion won send(), but the UI's timeout won select()
        assert!(!path.exists());
    }
}
