#![cfg(target_os = "macos")]

use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

use objc2::ffi::object_setClass;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, MethodImplementation, Sel};
use objc2::{class, msg_send, sel, AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSEvent, NSPanel, NSRunningApplication, NSTrackingArea,
    NSTrackingAreaOptions, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use tauri::{AppHandle, Emitter};

static PREVIOUS_APP_PID: AtomicI32 = AtomicI32::new(0);

extern "C-unwind" fn returns_yes(_: &AnyObject, _: Sel) -> Bool {
    Bool::YES
}

extern "C-unwind" fn returns_no(_: &AnyObject, _: Sel) -> Bool {
    Bool::NO
}

// Custom NSPanel subclasses: canBecomeMainWindow=NO, canBecomeKeyWindow=YES for
// the context menu and NO for the recording widget.
// OR'ing NonactivatingPanel into the style mask after object_setClass is not
// enough — Cocoa caches some behavior at init-time and the panel either fails
// to become key (keystrokes leak to the frontmost app) or pulls the whole app
// forward. Overriding the class methods directly nails down both behaviors.
fn register_panel_class(
    name: &'static CStr,
    can_become_key_window: impl MethodImplementation<Callee = AnyObject, Arguments = (), Return = Bool>,
) -> &'static AnyClass {
    let mut builder = ClassBuilder::new(name, class!(NSPanel))
        .unwrap_or_else(|| panic!("{name:?} class registration failed"));

    unsafe {
        builder.add_method(sel!(canBecomeKeyWindow), can_become_key_window);
        let main_fn: extern "C-unwind" fn(_, _) -> _ = returns_no;
        builder.add_method(sel!(canBecomeMainWindow), main_fn);
    }

    builder.register()
}

fn promptheus_panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let key_fn: extern "C-unwind" fn(_, _) -> _ = returns_yes;
        register_panel_class(c"PromptheusContextMenuPanel", key_fn)
    })
}

fn promptheus_widget_panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let key_fn: extern "C-unwind" fn(_, _) -> _ = returns_no;
        register_panel_class(c"PromptheusWidgetPanel", key_fn)
    })
}

fn with_ns_window<R>(
    ns_window: tauri::Result<*mut c_void>,
    f: impl FnOnce(&NSWindow) -> R,
) -> Result<R, String> {
    let ptr = ns_window.map_err(|e| e.to_string())?;
    if ptr.is_null() {
        return Err("ns_window pointer is null".into());
    }
    Ok(f(unsafe { &*(ptr as *const NSWindow) }))
}

pub async fn on_main(
    window: &tauri::WebviewWindow,
    f: impl FnOnce(&tauri::WebviewWindow) -> Result<(), String> + Send + 'static,
) -> crate::Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let target = window.clone();
    window.run_on_main_thread(move || {
        let _ = tx.send(f(&target));
    })?;
    rx.await.map_err(|e| e.to_string())??;
    Ok(())
}

fn reclass_as_nonactivating_panel(
    window: &tauri::WebviewWindow,
    class: &'static AnyClass,
) -> Result<&'static NSPanel, String> {
    with_ns_window(window.ns_window(), |ns_window| unsafe {
        object_setClass(ns_window as *const NSWindow as *mut AnyObject, class);

        let panel: &'static NSPanel = &*(ns_window as *const NSWindow as *const NSPanel);
        let mask = panel.styleMask();
        panel.setStyleMask(mask | NSWindowStyleMask::NonactivatingPanel);
        panel
    })
}

// Convert a webview window's underlying NSWindow into a non-activating NSPanel
// so that showing the panel does not raise the app's other windows. It stays
// visible when the app deactivates, so a click into another app (Select text)
// does not hide the context menu.
pub fn make_nonactivating_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    let panel = reclass_as_nonactivating_panel(window, promptheus_panel_class())?;
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(false);
    panel.setFloatingPanel(true);
    Ok(())
}

// Convert the recording widget into a non-activating NSPanel that never becomes
// key or main and stays visible when the app deactivates, so a click on it
// leaves the frontmost app active.
pub fn make_nonactivating_widget_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    let panel = reclass_as_nonactivating_panel(window, promptheus_widget_panel_class())?;
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(false);
    panel.setFloatingPanel(true);
    Ok(())
}

// Order the panel front and grab system-wide focus without raising siblings.
//
// `makeKeyAndOrderFront:` alone is not enough: a NonactivatingPanel becomes the
// app's key window but does not make the app frontmost, so keystrokes still go
// to whatever app was frontmost. We follow with NSRunningApplication's
// cooperative-activation API (no `.activateAllWindows`), which makes the
// process frontmost and routes the keystrokes to our panel without raising
// our other windows.
pub fn show_panel_without_activating(window: &tauri::WebviewWindow) -> Result<(), String> {
    with_ns_window(window.ns_window(), |ns_window| unsafe {
        remember_frontmost_app();
        ns_window.makeKeyAndOrderFront(None);

        let running: *mut AnyObject =
            msg_send![class!(NSRunningApplication), currentApplication];
        if !running.is_null() {
            let _: bool = msg_send![running, activateWithOptions: 0usize];
        }
    })
}

// Repositioning shows the panel again while it is key, with Promptheus
// frontmost; that show keeps the pid remembered by an earlier one.
fn remember_frontmost_app() {
    let pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
        .filter(|&pid| pid != std::process::id() as i32);
    if let Some(pid) = pid {
        PREVIOUS_APP_PID.store(pid, Ordering::Relaxed);
    }
}

// Showing the panel activated our app, so hiding it alone leaves our app active
// with no key window. Focus goes back to the app that was frontmost before the
// show only while the panel still had it: when another app or one of our
// windows took focus, that one keeps it. The caller passes `restore_focus:
// false` when it opens one of our windows right after the hide.
pub fn hide_panel(window: &tauri::WebviewWindow, restore_focus: bool) -> Result<(), String> {
    with_ns_window(window.ns_window(), |ns_window| {
        let panel_has_focus =
            ns_window.isKeyWindow() && NSRunningApplication::currentApplication().isActive();
        ns_window.orderOut(None);
        let pid = PREVIOUS_APP_PID.swap(0, Ordering::Relaxed);
        if restore_focus && panel_has_focus && pid != 0 {
            activate_app(pid);
        }
    })
}

fn activate_app(pid: i32) {
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        log::debug!("context menu hidden: previous app pid={pid} is gone");
        return;
    };
    let activated = app.activateWithOptions(NSApplicationActivationOptions::empty());
    log::debug!("context menu hidden: previous app pid={pid} re-activated={activated}");
}

// Force the green traffic-light button into native fullscreen-arrows mode.
// Tauri's `TitleBarStyle::Overlay` + `hidden_title(true)` leaves the window
// without `FullScreenPrimary` collection behavior, so AppKit falls back to
// "+" (zoom) by default. Setting it explicitly restores the native default:
// arrows on hover, "+" on Option+hover.
pub fn enable_fullscreen_primary(window: &tauri::Window) -> Result<(), String> {
    with_ns_window(window.ns_window(), |ns_window| {
        let mut behavior = ns_window.collectionBehavior();
        behavior.remove(NSWindowCollectionBehavior::FullScreenNone);
        behavior.remove(NSWindowCollectionBehavior::FullScreenAuxiliary);
        behavior.insert(NSWindowCollectionBehavior::FullScreenPrimary);
        ns_window.setCollectionBehavior(behavior);
    })
}

// Re-focus the (already visible) panel after a geometry change.
pub fn make_key_without_activating(window: &tauri::WebviewWindow) -> Result<(), String> {
    with_ns_window(window.ns_window(), |ns_window| {
        ns_window.makeKeyAndOrderFront(None);
    })
}

// Move and resize the panel in one `setFrame:display:` call on the main thread.
// Input is Tauri's top-left physical px; Cocoa wants points with a bottom-left
// origin measured from the primary screen's height.
pub fn set_frame_physical(
    window: &tauri::WebviewWindow,
    origin: (f64, f64),
    size: (f64, f64),
    scale: f64,
    primary_height_points: f64,
) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let moved = with_ns_window(target.ns_window(), |ns_window| {
                set_frame_in_points(ns_window, origin, size, scale, primary_height_points);
            });
            if let Err(e) = moved {
                log::warn!("setFrame skipped: {e}");
            }
        })
        .map_err(|e| e.to_string())
}

fn set_frame_in_points(
    ns_window: &NSWindow,
    origin: (f64, f64),
    size: (f64, f64),
    scale: f64,
    primary_height_points: f64,
) {
    let height = size.1 / scale;
    let mut frame = ns_window.frame();
    frame.origin.x = origin.0 / scale;
    frame.origin.y = primary_height_points - origin.1 / scale - height;
    frame.size.width = size.0 / scale;
    frame.size.height = height;
    ns_window.setFrame_display(frame, true);
}

fn run_overlay_closure(
    window: &tauri::WebviewWindow,
    what: &'static str,
    f: impl FnOnce(&NSWindow) + Send + 'static,
) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(e) = with_ns_window(target.ns_window(), f) {
                log::warn!("{what} skipped: {e}");
            }
        })
        .map_err(|e| e.to_string())
}

// Sets the frame at alpha 0 and click-through in the same main-thread closure,
// then orders the window in when it is not, so its content and frame change
// without a frame on screen.
pub fn prepare_overlay_frame(
    window: &tauri::WebviewWindow,
    origin: (f64, f64),
    size: (f64, f64),
    scale: f64,
    primary_height_points: f64,
) -> Result<(), String> {
    run_overlay_closure(window, "prepare overlay frame", move |ns_window| {
        ns_window.setAlphaValue(0.0);
        ns_window.setIgnoresMouseEvents(true);
        set_frame_in_points(ns_window, origin, size, scale, primary_height_points);
        if !ns_window.isVisible() {
            ns_window.orderFrontRegardless();
        }
    })
}

pub fn reveal_overlay_panel(window: &tauri::WebviewWindow, ignore_mouse: bool) -> Result<(), String> {
    run_overlay_closure(window, "reveal overlay panel", move |ns_window| {
        ns_window.setIgnoresMouseEvents(ignore_mouse);
        ns_window.setAlphaValue(1.0);
    })
}

pub fn conceal_overlay_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    run_overlay_closure(window, "conceal overlay panel", |ns_window| {
        ns_window.setAlphaValue(0.0);
        ns_window.setIgnoresMouseEvents(true);
    })
}

pub fn top_left_y(location_y: f64, bounds_height: f64, flipped: bool) -> f64 {
    if flipped {
        location_y
    } else {
        bounds_height - location_y
    }
}

static POINTER_APP: OnceLock<AppHandle> = OnceLock::new();
static POINTER_OWNERS: Mutex<Vec<(usize, String, &'static str)>> = Mutex::new(Vec::new());

fn emit_pointer(owner: &AnyObject, payload: serde_json::Value) {
    let Some(app) = POINTER_APP.get() else {
        return;
    };
    let key = owner as *const AnyObject as usize;
    let targets = POINTER_OWNERS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, label, event)) = targets.iter().find(|(owner, _, _)| *owner == key) {
        let _ = app.emit_to(label.as_str(), event, payload);
    }
}

extern "C-unwind" fn pointer_moved(this: &AnyObject, _: Sel, event: &NSEvent) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(view) = event.window(mtm).and_then(|window| window.contentView()) else {
        return;
    };
    let point = view.convertPoint_fromView(event.locationInWindow(), None);
    let y = top_left_y(point.y, view.bounds().size.height, view.isFlipped());
    log::trace!("window pointer x={} y={y}", point.x);
    emit_pointer(this, serde_json::json!({ "x": point.x, "y": y }));
}

extern "C-unwind" fn pointer_exited(this: &AnyObject, _: Sel, _: &NSEvent) {
    emit_pointer(this, serde_json::Value::Null);
}

fn pointer_owner_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new(c"PromptheusPointerOwner", class!(NSObject))
            .expect("PromptheusPointerOwner class registration failed");
        let moved: extern "C-unwind" fn(_, _, _) = pointer_moved;
        let exited: extern "C-unwind" fn(_, _, _) = pointer_exited;
        unsafe {
            builder.add_method(sel!(mouseMoved:), moved);
            builder.add_method(sel!(mouseEntered:), moved);
            builder.add_method(sel!(mouseExited:), exited);
        }
        builder.register()
    })
}

// WebKit tracks the mouse only in the key window, and the widget panel never
// becomes key, so the page gets no hover events. A tracking area that is active
// always reports the pointer to the page as top-left points instead, emitted as
// `event` to the window's own label.
pub fn install_pointer_tracking(
    window: &tauri::WebviewWindow,
    app: AppHandle,
    event: &'static str,
) -> Result<(), String> {
    let _ = POINTER_APP.set(app);
    let view = with_ns_window(window.ns_window(), |ns_window| ns_window.contentView())?
        .ok_or("content view is missing")?;

    unsafe {
        let owner: Retained<AnyObject> = msg_send![pointer_owner_class(), new];
        let options = NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::InVisibleRect;
        let area = NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            view.bounds(),
            options,
            Some(&owner),
            None,
        );
        view.addTrackingArea(&area);
        POINTER_OWNERS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((&*owner as *const AnyObject as usize, window.label().to_string(), event));
        std::mem::forget(owner);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::top_left_y;

    #[test]
    fn a_non_flipped_view_measures_y_from_the_bottom() {
        assert_eq!(top_left_y(30.0, 112.0, false), 82.0);
    }

    #[test]
    fn a_flipped_view_already_measures_y_from_the_top() {
        assert_eq!(top_left_y(30.0, 112.0, true), 30.0);
    }
}
