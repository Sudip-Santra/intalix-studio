//! Main window placement.
//!
//! The configured default size (1440×900 logical) can be larger than the usable screen on
//! small or highly scaled displays, which would push the status bar under the taskbar. On
//! startup we fit the window inside the monitor's work area (screen minus taskbar), centre it,
//! and only then show it, so it never appears at the wrong size.

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

/// Share of the work area a fitted window uses, leaving a visible margin around it.
const FIT_RATIO: f64 = 0.94;

pub fn fit_and_show(window: &WebviewWindow) {
    if let Err(error) = fit(window) {
        tracing::warn!(%error, "could not fit the window to the screen");
    }
    if let Err(error) = window.show() {
        tracing::error!(%error, "could not show the main window");
    }
    let _ = window.set_focus();
}

fn fit(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let area = monitor.work_area();
    // set_size() sets the content (inner) size; the title bar and borders come on top, so
    // all fitting is done on the outer size and the frame is subtracted at the end.
    let outer = window.outer_size()?;
    let inner = window.inner_size()?;
    let frame = PhysicalSize::new(
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    if outer.width <= area.size.width && outer.height <= area.size.height {
        // Fits already; just make sure it is centred within the usable area.
        return centre(window, area.position, area.size, outer);
    }

    let target = PhysicalSize::new(
        scaled(area.size.width).min(outer.width),
        scaled(area.size.height).min(outer.height),
    );
    window.set_size(PhysicalSize::new(
        target.width.saturating_sub(frame.width),
        target.height.saturating_sub(frame.height),
    ))?;
    centre(window, area.position, area.size, target)
}

fn scaled(value: u32) -> u32 {
    // Truncation is intended: a pixel less is better than overflowing the screen.
    (f64::from(value) * FIT_RATIO) as u32
}

fn centre(
    window: &WebviewWindow,
    origin: PhysicalPosition<i32>,
    area: PhysicalSize<u32>,
    size: PhysicalSize<u32>,
) -> tauri::Result<()> {
    let offset = |avail: u32, used: u32| i32::try_from(avail.saturating_sub(used) / 2).unwrap_or(0);
    window.set_position(PhysicalPosition::new(
        origin.x + offset(area.width, size.width),
        origin.y + offset(area.height, size.height),
    ))
}
