use crate::state::CapturedImage;
use image::RgbaImage;

/// Capture full screen: primary monitor, or the monitor at index.
pub fn capture_full(monitor_index: Option<usize>) -> Result<CapturedImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("monitor list failed: {e}"))?;
    if monitors.is_empty() {
        return Err("no monitors found".into());
    }
    let mon = match monitor_index {
        Some(i) => monitors.get(i.min(monitors.len() - 1)),
        None => monitors.iter().find(|m| m.is_primary().unwrap_or(false)).or(monitors.first()),
    }
    .ok_or("monitor not found")?;
    let img = mon.capture_image().map_err(|e| format!("capture failed: {e}"))?;
    Ok(encode(&img))
}

/// Capture one window: prefer the focused non-minimized window, else the active one.
pub fn capture_active_window() -> Result<CapturedImage, String> {
    let windows = xcap::Window::all().map_err(|e| format!("window list failed: {e}"))?;
    let pick = windows
        .iter()
        .find(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
        .or_else(|| {
            windows
                .iter()
                .find(|w| !w.is_minimized().unwrap_or(true) && w.title().map(|t| !t.is_empty()).unwrap_or(false))
        })
        .ok_or("no capturable window")?;
    let img = pick.capture_image().map_err(|e| format!("window capture failed: {e}"))?;
    Ok(encode(&img))
}

/// Capture a region given in **global screen coordinates** (device pixels).
/// xcap's capture_region wants monitor-relative coordinates, so resolve the
/// monitor under the region center, subtract its origin, and clamp to its bounds.
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> Result<CapturedImage, String> {
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
