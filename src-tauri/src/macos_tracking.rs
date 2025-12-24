//! Native macOS mouse tracking for transparent overlay windows.
//! Uses NSEvent mouseLocation and NSWindow frame polling since NSTrackingArea
//! doesn't work reliably with transparent Tauri windows.

#![allow(deprecated)] // cocoa/objc crates are deprecated but still work

#[cfg(target_os = "macos")]
use cocoa::base::id;
#[cfg(target_os = "macos")]
use cocoa::foundation::NSRect;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Sets up mouse tracking for the overlay window using native macOS polling.
/// Emits `overlay-mouse-enter` and `overlay-mouse-leave` events to the frontend.
pub fn setup_mouse_tracking(app: AppHandle) {
    let app_clone = app.clone();
    let is_inside = Arc::new(AtomicBool::new(false));

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(100));

            if let Some(window) = app_clone.get_webview_window("overlay") {
                // Check if window is visible
                if !window.is_visible().unwrap_or(false) {
                    continue;
                }

                #[cfg(target_os = "macos")]
                {
                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

                    if let Ok(handle) = window.window_handle() {
                        if let RawWindowHandle::AppKit(appkit_handle) = handle.as_raw() {
                            let ns_view = appkit_handle.ns_view.as_ptr() as id;

                            let mouse_in_window = unsafe {
                                // Get NSWindow from NSView
                                let ns_window: id = msg_send![ns_view, window];
                                if ns_window.is_null() {
                                    false
                                } else {
                                    // Get mouse location in screen coordinates
                                    let mouse_loc: cocoa::foundation::NSPoint =
                                        msg_send![class!(NSEvent), mouseLocation];

                                    // Get window frame in screen coordinates
                                    let frame: NSRect = msg_send![ns_window, frame];

                                    // Check if mouse is inside window frame
                                    mouse_loc.x >= frame.origin.x
                                        && mouse_loc.x <= frame.origin.x + frame.size.width
                                        && mouse_loc.y >= frame.origin.y
                                        && mouse_loc.y <= frame.origin.y + frame.size.height
                                }
                            };

                            let was_inside = is_inside.load(Ordering::Relaxed);

                            if mouse_in_window && !was_inside {
                                // Mouse entered
                                is_inside.store(true, Ordering::Relaxed);
                                let _ = app_clone.emit("overlay-mouse-enter", ());
                            } else if !mouse_in_window && was_inside {
                                // Mouse left
                                is_inside.store(false, Ordering::Relaxed);
                                let _ = app_clone.emit("overlay-mouse-leave", ());
                            }
                        }
                    }
                }

                #[cfg(not(target_os = "macos"))]
                {
                    // On non-macOS platforms, do nothing (events should work normally)
                }
            }
        }
    });
}
