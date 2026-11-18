use std::ptr;
use std::time::{Duration, Instant};

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode, CGMouseButton,
};
use objc2_foundation::NSString;
use tauri::{AppHandle, Manager};

use super::{
    copied_on_select, equals_baseline, should_restore, AxSelection, Rect, Sample, TriggerDetector,
    COPY_WAIT_MS, POLL_INTERVAL_MS, SETTLE_DELAY_MS,
};
use crate::services::macos_ax::{
    AXUIElementCopyParameterizedAttributeValue, AXUIElementCreateApplication, AXUIElementCreateSystemWide,
    AXUIElementPerformAction, Cf,
};
use crate::services::notification::{NotificationLevel, NotificationService};
use crate::services::speech::claude_desktop::macos::{
    accessibility_trusted_or_prompt, on_main, restore_pasteboard, save_pasteboard,
};
use crate::services::speech::capture::{still_valid_now, Owner};
use crate::services::speech::widget::OVERLAY_WINDOW_LABEL;

const POLL_INTERVAL: Duration = Duration::from_millis(POLL_INTERVAL_MS);
const SETTLE_DELAY: Duration = Duration::from_millis(SETTLE_DELAY_MS);
const COPY_WAIT: Duration = Duration::from_millis(COPY_WAIT_MS);
const ARROW_KEYS: [CGKeyCode; 4] = [123, 124, 125, 126];
const KEY_A: CGKeyCode = 0;
const KEY_C: CGKeyCode = 8;
fn marker_range_text(element: &Cf) -> Option<String> {
    let marker = element.attribute("AXSelectedTextMarkerRange")?;
    let mut text = ptr::null();
    let status = unsafe {
        AXUIElementCopyParameterizedAttributeValue(
            element.0,
            &NSString::from_str("AXStringForTextMarkerRange"),
            marker.0,
            &mut text,
        )
    };
    if status != 0 {
        return None;
    }
    Cf::owned(text)?.string()
}

fn read_ax_selection() -> Option<AxSelection<Cf>> {
    let system = Cf::owned(unsafe { AXUIElementCreateSystemWide() })?;
    let element = system.attribute("AXFocusedUIElement")?;
    let range = element.range_attribute("AXSelectedTextRange");
    let text = element
        .string_attribute("AXSelectedText")
        .filter(|text| !text.is_empty())
        .or_else(|| marker_range_text(&element))
        .unwrap_or_default();
    Some(AxSelection { element, range, text })
}

enum CopyItem {
    Missing,
    Disabled,
    Enabled(Cf),
}

fn is_copy_item(item: &Cf) -> bool {
    item.string_attribute("AXMenuItemCmdChar").as_deref() == Some("C")
        && item.number_attribute("AXMenuItemCmdModifiers") == Some(0)
}

fn find_copy_item(node: &Cf) -> Option<Cf> {
    node.children()
        .into_iter()
        .find_map(|child| if is_copy_item(&child) { Some(child) } else { find_copy_item(&child) })
}

fn frontmost_copy_item() -> CopyItem {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return CopyItem::Missing;
    };
    let Some(application) = Cf::owned(unsafe { AXUIElementCreateApplication(app.processIdentifier()) }) else {
        return CopyItem::Missing;
    };
    let Some(item) = application
        .attribute("AXMenuBar")
        .and_then(|bar| find_copy_item(&bar))
    else {
        return CopyItem::Missing;
    };
    if item.bool_attribute("AXEnabled") == Some(false) {
        CopyItem::Disabled
    } else {
        CopyItem::Enabled(item)
    }
}

fn press_menu_item(item: &Cf) -> bool {
    unsafe { AXUIElementPerformAction(item.0, &NSString::from_str("AXPress")) == 0 }
}

fn post_copy_key() {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    for down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(source.as_deref(), KEY_C, down) else {
            log::warn!("selection capture: creating the Cmd+C event failed");
            return;
        };
        CGEvent::set_flags(Some(&event), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
}

fn change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

fn pasteboard_string() -> Option<String> {
    NSPasteboard::generalPasteboard()
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|text| text.to_string())
}

fn read_sample(started: Instant, owner_key: bool) -> Option<Sample> {
    let source = CGEventSourceStateID::CombinedSessionState;
    let flags = CGEventSource::flags_state(source);
    let pointer = CGEvent::location(Some(&*CGEvent::new(None)?));
    Some(Sample {
        at_ms: started.elapsed().as_millis() as u64,
        left_down: CGEventSource::button_state(source, CGMouseButton::Left),
        x: pointer.x,
        y: pointer.y,
        shift: flags.contains(CGEventFlags::MaskShift),
        arrow: ARROW_KEYS.iter().any(|key| CGEventSource::key_state(source, *key)),
        command: flags.contains(CGEventFlags::MaskCommand),
        a_key: CGEventSource::key_state(source, KEY_A),
        owner_key,
    })
}

fn owner_frame(app: &AppHandle, label: &str) -> Option<(Rect, bool)> {
    let window = app.get_window(label)?;
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let frame = Rect {
        x: position.x as f64 / scale,
        y: position.y as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    };
    Some((frame, window.is_focused().unwrap_or(false)))
}

fn owner_frame_and_key(app: &AppHandle, owner: &Owner) -> (Rect, bool) {
    let (frame, focused) = owner_frame(app, owner.label()).unwrap_or_default();
    (frame, focused && matches!(owner, Owner::Window(_)))
}

fn pill_overlay_frame(app: &AppHandle, owner: &Owner) -> Rect {
    let visible = app
        .get_window(OVERLAY_WINDOW_LABEL)
        .is_some_and(|window| window.is_visible().is_ok_and(|visible| visible));
    if !matches!(owner, Owner::Pill { .. }) || !visible {
        return Rect::default();
    }
    owner_frame(app, OVERLAY_WINDOW_LABEL).map(|(frame, _)| frame).unwrap_or_default()
}

async fn warn_permission_missing(app: &AppHandle) {
    log::info!("selection capture skipped: Accessibility permission missing");
    NotificationService::notify_now(
        app,
        "selection_permission",
        NotificationLevel::Warning,
        "Accessibility permission needed",
        Some("Allow Promptheus in System Settings → Privacy & Security → Accessibility, then try again"),
    )
    .await;
}

async fn wait_for_pasteboard_change(app: &AppHandle, before: isize) -> Option<isize> {
    let started = Instant::now();
    loop {
        let count = on_main(app, change_count).await.ok()?;
        if count != before {
            return Some(count);
        }
        if started.elapsed() >= COPY_WAIT {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn copy_fallback(app: &AppHandle, owner: &Owner, run: u64) -> Option<String> {
    let started = Instant::now();
    let item = match tauri::async_runtime::spawn_blocking(frontmost_copy_item).await.ok()? {
        CopyItem::Disabled => {
            log::debug!("selection capture: Copy menu item disabled, no selection");
            return None;
        }
        CopyItem::Missing => None,
        CopyItem::Enabled(item) => Some(item),
    };
    if !still_valid_now(app, owner, run).await {
        return None;
    }
    let (saved, before) = on_main(app, || (save_pasteboard(), change_count())).await.ok()?;
    let pressed = item.is_some();
    tauri::async_runtime::spawn_blocking(move || {
        if !item.is_some_and(|item| press_menu_item(&item)) {
            post_copy_key();
        }
    })
    .await
    .ok()?;
    let Some(after_copy) = wait_for_pasteboard_change(app, before).await else {
        log::debug!("selection capture: copy fallback changed nothing, menu item={pressed}");
        return None;
    };
    let text = on_main(app, move || {
        let text = pasteboard_string();
        let restore = should_restore(change_count(), after_copy);
        if restore {
            restore_pasteboard(saved);
        }
        log::debug!("selection capture: pasteboard restored={restore}");
        text
    })
    .await
    .ok()?
    .filter(|text| !text.is_empty());
    log::debug!(
        "selection capture: copy fallback menu item={pressed}, {} ms, length={:?}",
        started.elapsed().as_millis(),
        text.as_ref().map(String::len)
    );
    text
}

// A pasteboard change between the left press that started a drag or multi-click outside the owner window and
// the `changeCount` read in `read_after_trigger` right after the settle delay, with that read at most
// GESTURE_WINDOW_MS after the release, counts as the frontmost app's copy-on-select. The read comes before
// the AX read and `still_valid_now`, whose waits would otherwise push it past the window.
// Wrong when a write that is not the selection lands in that window (a skill result or webview-send
// fallback written by this app, a background write by another app) and the frontmost app gives no AX
// selection: that content is taken as the selection. Upgrade path: record the `changeCount` each of this
// app's own pasteboard writes produces and reject it here; writes by other apps stay indistinguishable,
// NSPasteboard names no writer.
async fn copy_on_select_text(app: &AppHandle) -> Option<String> {
    let text = on_main(app, pasteboard_string).await.ok()?.filter(|text| !text.is_empty())?;
    log::debug!(
        "selection capture: pasteboard changed since the press, copy-on-select, length={}",
        text.len()
    );
    Some(text)
}

async fn read_after_trigger(
    app: &AppHandle,
    owner: &Owner,
    run: u64,
    baseline: &mut Option<AxSelection<Cf>>,
    pointer_gesture: Option<(isize, Instant)>,
) -> Option<String> {
    tokio::time::sleep(SETTLE_DELAY).await;
    let copied = match pointer_gesture {
        Some((change_count_at_press, released)) => on_main(app, move || {
            let ms_since_release = released.elapsed().as_millis() as u64;
            copied_on_select(change_count(), change_count_at_press, ms_since_release)
        })
        .await
        .ok()?,
        None => false,
    };
    let read = tauri::async_runtime::spawn_blocking(read_ax_selection).await.ok()?;
    if let Some(read) = read.filter(|read| !read.text.is_empty()) {
        if equals_baseline(baseline.as_ref(), &read) {
            log::debug!("selection capture: AX selection equals the baseline, waiting");
            return None;
        }
        log::debug!("selection capture: AX path, length={}", read.text.len());
        return Some(read.text);
    }
    if !still_valid_now(app, owner, run).await {
        return None;
    }
    if copied {
        if let Some(text) = copy_on_select_text(app).await {
            return Some(text);
        }
    }
    copy_fallback(app, owner, run).await
}

pub async fn capture(app: &AppHandle, owner: &Owner, run: u64) -> Option<String> {
    if !accessibility_trusted_or_prompt() {
        warn_permission_missing(app).await;
        return None;
    }
    let mut baseline = tauri::async_runtime::spawn_blocking(read_ax_selection).await.ok()?;
    let mut change_count_at_press = None;
    let started = Instant::now();
    let mut detector = TriggerDetector::default();
    loop {
        if !still_valid_now(app, owner, run).await {
            log::debug!("selection capture stopped after {} ms", started.elapsed().as_millis());
            return None;
        }
        let (frame, owner_key) = owner_frame_and_key(app, owner);
        if let Some(sample) = read_sample(started, owner_key) {
            if detector.is_press(&sample) {
                change_count_at_press = Some(on_main(app, change_count).await.ok()?);
            }
            detector.exclude = frame;
            detector.overlay = pill_overlay_frame(app, owner);
            if let Some(trigger) = detector.feed(&sample) {
                log::debug!("selection capture: trigger {}", trigger.name());
                let pointer_gesture = change_count_at_press
                    .filter(|_| trigger.is_pointer())
                    .map(|count| (count, Instant::now()));
                let read = read_after_trigger(app, owner, run, &mut baseline, pointer_gesture).await;
                if let Some(text) = read {
                    return Some(text);
                }
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
