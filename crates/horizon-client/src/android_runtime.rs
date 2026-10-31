//! Explicit Android renderer policy. Both modes render through Vulkan on the GPU.
use std::sync::atomic::{AtomicBool, Ordering};
static GPU_BATCHING: AtomicBool = AtomicBool::new(false);
pub fn configure_gpu(batching: bool) {
    GPU_BATCHING.store(batching, Ordering::Relaxed);
}
pub fn compatibility() -> bool {
    cfg!(target_os = "android") && !GPU_BATCHING.load(Ordering::Relaxed)
}

/// Android's native entry point installs these without making the shared lib
/// depend on an Activity or on JNI. Clipboard access is only on explicit edits.
pub struct ClipboardHooks {
    pub get: fn() -> Option<String>,
    pub set: fn(&str),
}
static CLIPBOARD: std::sync::OnceLock<ClipboardHooks> = std::sync::OnceLock::new();
pub fn install_clipboard(hooks: ClipboardHooks) {
    let _ = CLIPBOARD.set(hooks);
}
pub fn clipboard_get() -> Option<String> {
    CLIPBOARD.get().and_then(|h| (h.get)())
}
pub fn clipboard_set(text: &str) {
    if let Some(h) = CLIPBOARD.get() {
        (h.set)(text);
    }
}
