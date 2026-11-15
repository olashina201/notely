use crate::state::CapturedImage;
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};
static CAPTURE_VERIFIED: AtomicBool = AtomicBool::new(false);

/// Optimize only the copy sent to AI. Keep the original capture/preview intact.
/// Smart retains captured detail; Fast bounds the upload cost for large images.
pub fn prepare_for_assist(mut capture: CapturedImage, smart: bool) -> Result<CapturedImage, String> {
    if smart || capture.data_base64.len() <= 240 * 1024 { return Ok(capture); }
    let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &capture.data_base64)
        .map_err(|_| "Invalid screenshot encoding")?;
    let image = image::load_from_memory(&data).map_err(|_| "Could not decode the screenshot for analysis")?;
    let image = image.thumbnail(1280, 1280).to_rgb8();
    let mut jpeg = Vec::new();
    for quality in [65, 55, 45] {
        jpeg.clear();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality).encode_image(&image)
            .map_err(|_| "Could not prepare the screenshot for analysis")?;
        if jpeg.len() <= 300 * 1024 { break; }
    }
    // Never replace an already smaller original with a larger re-encoding.
    if jpeg.len() < data.len() {
        capture.data_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &jpeg);
        capture.mime_type = "image/jpeg".into();
        capture.width = image.width();
        capture.height = image.height();
    }
    Ok(capture)
}

#[cfg(test)]
mod preparation_tests {
    use super::*;
    #[test]
    fn fast_reduces_large_uploads_and_smart_preserves_original() {
        let mut image = image::RgbImage::new(1512, 982);
        let mut value = 12345u32;
        for pixel in image.pixels_mut() {
            value ^= value << 13; value ^= value >> 17; value ^= value << 5;
            *pixel = image::Rgb([value as u8, (value >> 8) as u8, (value >> 16) as u8]);
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75).encode_image(&image).unwrap();
        let capture = CapturedImage { data_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &jpeg), mime_type: "image/jpeg".into(), width: 1512, height: 982, taken_at: 42 };
        let smart = prepare_for_assist(capture.clone(), true).unwrap();
        assert_eq!(smart.data_base64, capture.data_base64);
        let fast = prepare_for_assist(capture.clone(), false).unwrap();
        assert!(fast.data_base64.len() < capture.data_base64.len() / 2);
        assert_eq!(fast.width, 1280);
        assert_eq!(fast.taken_at, 42);
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &fast.data_base64).unwrap();
        assert!(image::load_from_memory(&decoded).is_ok());
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn notely_screen_permission() -> bool;
    fn notely_modern_capture_available() -> bool;
    fn notely_request_screen_permission() -> bool;
    fn notely_capture_image(display: u32, window: u32, x: f64, y: f64, w: f64, h: f64,
        bytes: *mut *mut u8, length: *mut usize, width: *mut u32, height: *mut u32) -> i32;
    fn notely_capture_free(memory: *mut std::ffi::c_void);
}

pub fn screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    { (unsafe { notely_screen_permission() }) || CAPTURE_VERIFIED.load(Ordering::SeqCst) }
    #[cfg(not(target_os = "macos"))]
    { true }
}

pub fn request_screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    { unsafe { notely_request_screen_permission() } }
    #[cfg(not(target_os = "macos"))]
    { true }
}

pub fn ensure_screen_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if unsafe { notely_modern_capture_available() } {
        // Preflight is advisory for modern capture. ScreenCaptureKit enforces
        // the OS permission itself; a false preflight must not prevent a real test.
        if !screen_permission() {
            crate::diagnostics::record("info", "capture.preflight_unconfirmed", None, serde_json::json!({"granted": false}));
        }
        return Ok(());
    }
    if screen_permission() { Ok(()) } else {
        crate::diagnostics::record("error", "capture.permission_denied", None, serde_json::json!({"granted": false}));
        Err("Screen Recording permission is not granted. Open Notely Settings → Screen capture, enable access in macOS, then restart Notely if macOS requests it.".into())
    }
}

#[cfg(target_os = "macos")]
fn modern_capture(display: u32, window: u32, rect: Option<(i32, i32, u32, u32)>) -> Result<Option<CapturedImage>, String> {
    let (x, y, w, h) = rect.unwrap_or_default();
    let mut bytes = std::ptr::null_mut();
    let (mut length, mut width, mut height) = (0usize, 0u32, 0u32);
    let result = unsafe { notely_capture_image(display, window, x as f64, y as f64, w as f64, h as f64,
        &mut bytes, &mut length, &mut width, &mut height) };
    if result == 6 { return Ok(None); }
    if result != 0 {
        if result == 1 { CAPTURE_VERIFIED.store(false, Ordering::SeqCst); }
        crate::diagnostics::record("error", "capture.native_failed", None, serde_json::json!({"code": result}));
        return Err(match result {
            1 => "macOS declined ScreenCaptureKit for this Notely build. If access is already enabled, the entry may belong to an earlier signed build. Open Screen Recording settings for this app and restart if macOS requests it.",
            2 => "Screen capture did not finish within 10 seconds. Check Screen Recording permission and try again.",
            4 => "The selected display or window is no longer available. Try another capture.",
            _ => "ScreenCaptureKit could not capture this screen. Check macOS Screen Recording settings and restart Notely if access was just enabled.",
        }.into());
    }
    struct Buffer(*mut u8);
    impl Drop for Buffer { fn drop(&mut self) { unsafe { notely_capture_free(self.0.cast()); } } }
    let buffer = Buffer(bytes);
    if buffer.0.is_null() || length == 0 || length > 50 * 1024 * 1024 || width == 0 || height == 0 { return Err("Invalid image received from screen capture".into()); }
    let data = unsafe { std::slice::from_raw_parts(buffer.0, length) };
    CAPTURE_VERIFIED.store(true, Ordering::SeqCst);
    crate::diagnostics::record("info", "capture.native_finished", None, serde_json::json!({"width": width, "height": height, "imageBytes": length}));
    Ok(Some(CapturedImage { data_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
        mime_type: "image/jpeg".into(), width, height, taken_at: chrono::Utc::now().timestamp_millis() }))
}

/// Capture full screen: primary monitor, or the monitor at index.
pub fn capture_full(monitor_index: Option<usize>) -> Result<CapturedImage, String> {
    ensure_screen_permission()?;
    let monitors = xcap::Monitor::all().map_err(|e| format!("monitor list failed: {e}"))?;
    if monitors.is_empty() {
        return Err("no monitors found".into());
    }
    let mon = match monitor_index {
        Some(i) => monitors.get(i.min(monitors.len() - 1)),
        None => monitors.iter().find(|m| m.is_primary().unwrap_or(false)).or(monitors.first()),
    }
    .ok_or("monitor not found")?;
    #[cfg(target_os = "macos")]
    if let Some(image) = modern_capture(mon.id().map_err(|e| e.to_string())?, 0, None)? { return Ok(image); }
    let img = mon.capture_image().map_err(|e| format!("capture failed: {e}"))?;
    Ok(encode(&img))
}

/// Capture one window: prefer the focused non-minimized window, else the active one.
pub fn capture_active_window() -> Result<CapturedImage, String> {
    ensure_screen_permission()?;
    let windows = xcap::Window::all().map_err(|e| format!("window list failed: {e}"))?;
    let pick = windows
        .iter()
        .find(|w| w.pid().ok() != Some(std::process::id()) && w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
        .or_else(|| {
            windows
                .iter()
                .find(|w| w.pid().ok() != Some(std::process::id()) && !w.is_minimized().unwrap_or(true) && w.title().map(|t| !t.is_empty()).unwrap_or(false))
        })
        .ok_or("no capturable window")?;
    #[cfg(target_os = "macos")]
    if let Some(image) = modern_capture(0, pick.id().map_err(|e| e.to_string())?, None)? { return Ok(image); }
    let img = pick.capture_image().map_err(|e| format!("window capture failed: {e}"))?;
    Ok(encode(&img))
}

/// Capture a region given in **global screen coordinates** (device pixels).
/// xcap's capture_region wants monitor-relative coordinates, so resolve the
/// monitor under the region center, subtract its origin, and clamp to its bounds.
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> Result<CapturedImage, String> {
    ensure_screen_permission()?;
    if w == 0 || h == 0 {
        return Err("empty region".into());
    }
    let cx = x.saturating_add((w / 2) as i32);
    let cy = y.saturating_add((h / 2) as i32);
    let mon = xcap::Monitor::from_point(cx, cy).map_err(|e| format!("monitor lookup failed: {e}"))?;
    let mx = mon.x().map_err(|e| format!("monitor origin failed: {e}"))?;
    let my = mon.y().map_err(|e| format!("monitor origin failed: {e}"))?;
    let mw = mon.width().map_err(|e| format!("monitor size failed: {e}"))?;
    let mh = mon.height().map_err(|e| format!("monitor size failed: {e}"))?;
    let mut rx = x - mx;
    let mut ry = y - my;
    let mut rw = w;
    let mut rh = h;
    if rx < 0 {
        rw = rw.saturating_sub((-rx) as u32);
        rx = 0;
    }
    if ry < 0 {
        rh = rh.saturating_sub((-ry) as u32);
        ry = 0;
    }
    if (rx as u32).saturating_add(rw) > mw {
        rw = mw.saturating_sub(rx as u32);
    }
    if (ry as u32).saturating_add(rh) > mh {
        rh = mh.saturating_sub(ry as u32);
    }
    if rw == 0 || rh == 0 {
        return Err("region is outside monitor bounds".into());
    }
    #[cfg(target_os = "macos")]
    if let Some(image) = modern_capture(mon.id().map_err(|e| e.to_string())?, 0, Some((rx, ry, rw, rh)))? { return Ok(image); }
    let img = mon
        .capture_region(rx as u32, ry as u32, rw, rh)
        .map_err(|e| format!("region capture failed: {e}"))?;
    Ok(encode(&img))
}

pub fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("monitor list failed: {e}"))?;
    Ok(monitors
        .iter()
        .map(|m| MonitorInfo {
            name: m.friendly_name().unwrap_or_else(|_| "Display".into()),
            x: m.x().unwrap_or(0),
            y: m.y().unwrap_or(0),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            scale_factor: m.scale_factor().unwrap_or(1.0) as f64,
            is_primary: m.is_primary().unwrap_or(false),
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// Downscale to max 1920px wide, JPEG encode, base64.
fn encode(img: &RgbaImage) -> CapturedImage {
    let (w, h) = (img.width(), img.height());
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let scaled = if w > 1920 {
        let nh = (h as f32 * 1920.0 / w as f32) as u32;
        image::imageops::thumbnail(&rgb, 1920, nh.max(1))
    } else {
        rgb
    };
    let (sw, sh) = (scaled.width(), scaled.height());
    let mut jpeg = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75);
    enc.encode_image(&scaled).unwrap_or(());
    CapturedImage {
        data_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &jpeg),
        mime_type: "image/jpeg".into(),
        width: sw,
        height: sh,
        taken_at: chrono::Utc::now().timestamp_millis(),
    }
}
