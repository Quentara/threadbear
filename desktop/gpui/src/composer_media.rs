//! macOS representations that GPUI's clipboard and filename-only drops omit.
//! All bytes are staged in the app, synchronously during a drop. Promised
//! files and URL-only browser images get a dim chip while they are loading.
use std::path::PathBuf;
use futures::channel::oneshot;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Owns a completed handover even while it sits unread in a channel. If a
/// timeout wins concurrently, dropping the queued reply deletes its copies.
pub(crate) struct PreparedPictures { pub(crate) paths: Vec<PathBuf> }
impl Drop for PreparedPictures {
    fn drop(&mut self) { for path in &self.paths { crate::composer::unstage(path); } }
}
type Pending = oneshot::Receiver<Result<PreparedPictures, String>>;
static PENDING: LazyLock<Mutex<HashMap<PathBuf, Pending>>> = LazyLock::new(Default::default);

pub(crate) async fn ready(path: &std::path::Path) -> Result<PreparedPictures, String> {
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).remove(path);
    match pending {
        Some(pending) => pending.await.unwrap_or_else(|_| Err("The picture source closed before it finished".into())),
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
type Sink = extern "C" fn(*mut std::ffi::c_void, i32, *const std::ffi::c_char, *const std::ffi::c_void, usize);
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
extern "C" fn collect(context: *mut std::ffi::c_void, kind: i32, name: *const std::ffi::c_char, bytes: *const std::ffi::c_void, len: usize) {
    use crate::composer::{keep_picture, paste_base, stage_private};
    use std::ffi::{CStr, CString};
    // SAFETY: read owns Captured for this synchronous call. AppKit supplies
    // valid UTF-8 strings and byte buffers for the duration of the callback.
    let captured = unsafe { &mut *context.cast::<Captured>() };
    let name = if name.is_null() { String::new() } else { unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned() };
    let path = match kind {
        0 => Some(keep_picture(PathBuf::from(name))),
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
            let Some(source) = captured.promise_folder.clone() else { return };
            if let Some(path) = stage_private(&paste_base(), "promised.png", b"") {
                let (tx, rx) = oneshot::channel();
                PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), rx);
                let folder = CString::new(source.path().to_string_lossy().as_bytes()).unwrap();
                let context = Box::into_raw(Box::new(Promise { path: path.clone(), paths: Vec::new(), tx, _source: source }));
                unsafe { tb_composer_promise(bytes, folder.as_ptr(), promised, context.cast()); }
                Some(path)
            } else { None }
        }
        _ => None,
    };
    if let Some(path) = path { captured.paths.push(path); }
}

#[cfg(target_os = "macos")]
struct Promise {
    path: PathBuf,
    paths: Vec<PathBuf>,
    tx: oneshot::Sender<Result<PreparedPictures, String>>,
    _source: std::sync::Arc<tempfile::TempDir>,
}
#[cfg(target_os = "macos")]
extern "C" fn promised(context: *mut std::ffi::c_void, kind: i32, name: *const std::ffi::c_char, _: *const std::ffi::c_void, _: usize) {
    // SAFETY: the native serial reader queue calls once per file, then one
    // terminal callback (4). The owned Box and destination outlive all files.
    if kind == 4 {
        let promise = unsafe { Box::from_raw(context.cast::<Promise>()) };
        let paths = promise.paths;
        let result = if paths.is_empty() { Err("Couldn't read the promised picture".into()) } else { Ok(PreparedPictures { paths }) };
        let _ = promise.tx.send(result);
        return;
    }
    let promise = unsafe { &mut *context.cast::<Promise>() };
    if kind == 6 && !name.is_null() && crate::composer::is_staged(&promise.path) {
        let original = PathBuf::from(unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned());
        let kept = crate::composer::keep_picture(original.clone());
        if kept != original { promise.paths.push(kept); }
    }
}

#[cfg(target_os = "macos")]
fn load_url(url: String) -> Option<PathBuf> {
    use crate::composer::{paste_base, stage_private};
    let path = stage_private(&paste_base(), "browser.png", b"")?;
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), rx);
    let destination = path.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(30))).build().new_agent();
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
    extern "C" { fn tb_composer_fixture_begin(kind: *const std::ffi::c_char, path: *const std::ffi::c_char); }
    let (kind, path) = (CString::new(kind).map_err(|e| e.to_string())?, CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?);
    unsafe { tb_composer_fixture_begin(kind.as_ptr(), path.as_ptr()); }
    Ok(())
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
