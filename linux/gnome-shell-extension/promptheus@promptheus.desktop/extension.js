import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const PLACE_WINDOW_TIMEOUT_MS = 1000;
const OBJECT_PATH = '/com/promptheus/Shell';
const PROVIDER_MENU_TITLE = 'Promptheus Provider Menu';
const APP_WM_CLASS = 'promptheus';
const IMAGE_PREVIEW_TITLE = 'Promptheus Image Preview';
const OVERLAY_TITLES = new Set([
    'Promptheus Context Menu',
    'Promptheus Palette',
    'Promptheus Palette Backdrop',
    PROVIDER_MENU_TITLE,
    'Promptheus Notifications',
    IMAGE_PREVIEW_TITLE,
]);
const TOAST_DURATIONS_MS = {success: 2000, error: 4000, info: 2000, warning: 3000};
const TOAST_MARGIN = 20;
const TOAST_GAP = 14;
const TOAST_OPACITY = 204;
const TOAST_SHOW_ANIMATION_MS = 150;
const TOAST_HIDE_ANIMATION_MS = 150;
const WIDGET_WIDTH = 264;
const WIDGET_HEIGHT = 72;
const WIDGET_ROW_HEIGHT = WIDGET_HEIGHT / 2;
const WIDGET_RADIUS = 16;
const WIDGET_ITEM_ROW_HEIGHT = 24;
const WIDGET_CHIP_RADIUS = 7;
const WIDGET_CHIP_REMOVE_LOCKED_OPACITY = 89;
const WIDGET_ITEM_ROW_GAP = 4;
const WIDGET_ITEMS_ROW_PADDING = 10;
const WIDGET_PADDING_X = 14;
const WIDGET_IMAGE_COLUMNS = 3;
const WIDGET_TEXT_CHIP_WIDTH = WIDGET_WIDTH - 2 * WIDGET_PADDING_X;
const WIDGET_CHIP_PADDING_LEFT = 6;
const WIDGET_CHIP_PADDING_RIGHT = 6;
const WIDGET_IMAGE_CHIP_PADDING_LEFT = 2;
const WIDGET_IMAGE_CHIP_PADDING_RIGHT = 4;
const WIDGET_CHIP_SPACING = 4;
const WIDGET_IMAGE_CHIP_SPACING = 5;
const WIDGET_THUMB_SIZE = 20;
const WIDGET_PREVIEW_MAX_HEIGHT = 160;
const WIDGET_POINTER_OFFSET_BELOW = 20;
const WIDGET_POINTER_OFFSET_ABOVE = 6;
const WIDGET_PREVIEW_MIN_BOX = 32;
const WIDGET_PREVIEW_DELAY_MS = 300;
const WIDGET_PREVIEW_PADDING = 8;
const WIDGET_PREVIEW_FADE_HEIGHT = 36;
const WIDGET_TOOLTIP_DELAY_MS = 700;
const WIDGET_TOOLTIP_CHAIN_MS = 500;
const WIDGET_TOOLTIP_HEIGHT = 24;
const WIDGET_TOOLTIP_PADDING_X = 8;
const WIDGET_DROPDOWN_ROW_HEIGHT = 28;
const WIDGET_DROPDOWN_PADDING = 4;
const WIDGET_DROPDOWN_SEPARATOR_HEIGHT = 9;
const WIDGET_DEFAULT_TARGET_LABEL = 'Dictation';
const WIDGET_BOTTOM_MARGIN = 24;
const WIDGET_STATES = ['recording', 'paused', 'processing', 'done'];
const WIDGET_BAR_MIN_HEIGHT = 4;
const WIDGET_BAR_MAX_HEIGHT = 28;
const WIDGET_BAR_MIN_SCALE = WIDGET_BAR_MIN_HEIGHT / WIDGET_BAR_MAX_HEIGHT;
const WIDGET_BAR_SCALE_EPSILON = 0.005;
const WIDGET_BAR_EASE_MS = 60;
const WIDGET_BAR_WIDTH = 2;
const WIDGET_BAR_SPACING = 3;
const WIDGET_TIME_WIDTH = 34;
const WIDGET_ROW_SPACING = 10;
const WIDGET_BUTTON_SIZE = 28;
const WIDGET_ACTION_SPACING = 4;
const WIDGET_ROW1_BUTTONS = 3;
const WIDGET_METER_WIDTH = WIDGET_WIDTH - 2 * WIDGET_PADDING_X - WIDGET_TIME_WIDTH - 2 * WIDGET_ROW_SPACING -
    WIDGET_ROW1_BUTTONS * WIDGET_BUTTON_SIZE - (WIDGET_ROW1_BUTTONS - 1) * WIDGET_ACTION_SPACING;
const WIDGET_BAR_COUNT = Math.floor(
    (WIDGET_METER_WIDTH + WIDGET_BAR_SPACING) / (WIDGET_BAR_WIDTH + WIDGET_BAR_SPACING));
const WIDGET_BAR_PATTERN = [0.5, 0.8, 0.65, 1, 0.7, 0.9, 0.55, 0.95, 0.75, 0.85, 0.6, 0.45, 0.8, 0.95, 0.6, 0.7, 1, 0.55];
const WIDGET_BAR_FACTORS = Array.from({length: WIDGET_BAR_COUNT},
    (_, i) => WIDGET_BAR_PATTERN[i % WIDGET_BAR_PATTERN.length]);
const WIDGET_WAVE_BAR_WIDTH = 2;
const WIDGET_WAVE_BAR_COUNT = Math.floor(
    (WIDGET_WIDTH - 2 * WIDGET_PADDING_X + WIDGET_BAR_SPACING) / (WIDGET_WAVE_BAR_WIDTH + WIDGET_BAR_SPACING));
const WIDGET_RETRY_WAVE_BAR_COUNT = Math.floor(
    (WIDGET_WIDTH - 2 * WIDGET_PADDING_X - WIDGET_BUTTON_SIZE - WIDGET_ROW_SPACING + WIDGET_BAR_SPACING) /
    (WIDGET_WAVE_BAR_WIDTH + WIDGET_BAR_SPACING));
const WIDGET_WAVE_HEIGHT = 24;
const WIDGET_WAVE_MIN_SCALE = WIDGET_BAR_MIN_HEIGHT / WIDGET_WAVE_HEIGHT;
const WIDGET_WAVE_AMPLITUDE = WIDGET_WAVE_HEIGHT / 3;
const WIDGET_WAVE_OPACITY = 150;
const WIDGET_DONE_ENTER_MS = 180;
const WIDGET_DONE_ENTER_SCALE = 0.85;
const WIDGET_SWAP_MS = 150;
const WIDGET_CROSSFADE_STATES = ['processing', 'retrying', 'done'];
const WIDGET_WAVE_PERIOD_US = 1200 * 1000;
const WIDGET_WAVE_FRAME_MS = 33;
const WIDGET_BUTTON_PLAIN = {rest: 'transparent', hover: '#3a3a3c'};
const WIDGET_BUTTON_PRIMARY = {rest: '#e5e5e7', hover: '#cfcfd3'};
const WIDGET_BUTTON_SECONDARY = {rest: '#3a3a3c', hover: '#48484a'};
const SELECTION_POLL_MS = 30;
const SELECTION_SETTLE_MS = 150;
const SELECTION_GESTURE_WINDOW_MS = 500;
const SELECTION_TEXT_MIMETYPES = ['text/plain;charset=utf-8', 'text/plain', 'UTF8_STRING', 'STRING', 'TEXT'];
const WIDGET_TARGET_PADDING_X = 8;
const WIDGET_DRAG_THRESHOLD_PX = 4;
const WIDGET_TARGET_ICONS = {'webview:claude': 'provider-claude', 'webview:chatgpt': 'provider-chatgpt'};
const WIDGET_TARGET_ICON_SIZE = 14;
const WIDGET_TARGET_ICON_GAP = 8;

function pointInRect(x, y, rect) {
    return x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height;
}

function resolveWidgetPosition(hasPosition, x, y, workAreas, pointerArea) {
    if (hasPosition && workAreas.some(area => pointInRect(x, y, area)))
        return {x, y, stored: true};
    return {
        x: Math.round(pointerArea.x + (pointerArea.width - WIDGET_WIDTH) / 2),
        y: pointerArea.y + pointerArea.height - WIDGET_BOTTOM_MARGIN - WIDGET_HEIGHT,
        stored: false,
    };
}

function workAreaIndexAt(x, y) {
    const areas = Main.layoutManager.monitors.map((_monitor, i) => Main.layoutManager.getWorkAreaForMonitor(i));
    return Math.max(0, areas.findIndex(area => pointInRect(x, y, area)));
}

function workAreaAt(x, y) {
    return Main.layoutManager.getWorkAreaForMonitor(workAreaIndexAt(x, y));
}

function placeWidgetPopover(popover, width, height) {
    const [pointerX, pointerY] = global.get_pointer();
    const area = workAreaAt(pointerX, pointerY);
    const x = Math.max(area.x, Math.min(pointerX, area.x + area.width - width));
    const below = pointerY + WIDGET_POINTER_OFFSET_BELOW + height <= area.y + area.height;
    popover.set_position(x, below ? pointerY + WIDGET_POINTER_OFFSET_BELOW : pointerY - WIDGET_POINTER_OFFSET_ABOVE - height);
}

function monitorScaleAt(x, y) {
    return global.display.get_monitor_scale(workAreaIndexAt(x, y));
}

function emptyWidgetModel() {
    return {
        targets: [],
        selectedId: '',
        selectedLabel: WIDGET_DEFAULT_TARGET_LABEL,
        locked: false,
        items: [],
        itemRows: 0,
        version: 0,
        stopChatDestination: '',
    };
}

function widgetItemsContentHeight(rows) {
    if (rows <= 0)
        return 0;
    return rows * WIDGET_ITEM_ROW_HEIGHT + (rows - 1) * WIDGET_ITEM_ROW_GAP;
}

function widgetItemsHeight(rows) {
    if (rows <= 0)
        return 0;
    return widgetItemsContentHeight(rows) + WIDGET_ITEMS_ROW_PADDING;
}

function isAppWindow(win, title) {
    return win.get_wm_class() === APP_WM_CLASS && win.get_title() === title;
}

function isMainWindowOf(win, wmClass) {
    return win.get_wm_class() === wmClass &&
        win.get_window_type() === Meta.WindowType.NORMAL &&
        !win.get_transient_for();
}

function elapsedMsSince(startUs) {
    return Math.floor((GLib.get_monotonic_time() - startUs) / 1000);
}

function formatElapsed(elapsedMs) {
    const seconds = Math.floor(elapsedMs / 1000);
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

function formatRetry(attempt, maxAttempts, remainingMs) {
    return `Retry ${attempt}/${maxAttempts} in ${Math.ceil(remainingMs / 1000)} s`;
}

function msUntilRetryTextChange(remainingMs) {
    return remainingMs - (Math.ceil(remainingMs / 1000) - 1) * 1000;
}

const PASTE_FOCUS_TIMEOUT_MS = 2000;
const PASTE_POLL_MS = 50;
const PASTE_WINDOW_TIMEOUT_MS = 20000;
const PASTE_WINDOW_POLL_MS = 100;
const PASTE_COLD_SETTLE_MS = 4000;
const RESTORE_TIMEOUT_MS = 2000;
const PASTE_READ_DELAY_MS = 1000;
const PASTE_RESTORE_DELAY_MS = 500;
const RESTORED_TEXT_MIMETYPE = 'text/plain;charset=utf-8';
const CAPTURE_MIN_SIZE_PX = 4;
const CAPTURE_FRAME_WAIT_MS = 100;

Gio._promisify(Shell.Screenshot.prototype, 'screenshot_area');

function fileListSource(uris) {
    const list = new TextEncoder().encode(uris.map(uri => `${uri}\r\n`).join(''));
    return Meta.SelectionSourceMemory.new('text/uri-list', new GLib.Bytes(list));
}

function sleep(ms) {
    return new Promise(resolve => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
        resolve();
        return GLib.SOURCE_REMOVE;
    }));
}

async function waitFor(condition, timeoutMs, pollMs = PASTE_POLL_MS) {
    for (let waited = 0; !condition(); waited += pollMs) {
        if (waited >= timeoutMs)
            return false;
        await sleep(pollMs);
    }
    return true;
}

// Restores one type (text, else an image); the other saved formats are lost, since
// Meta.SelectionSourceMemory holds one mimetype and GJS cannot override Meta.SelectionSource
// vfuncs that take a callback (read_async). Upgrade path: a Meta.SelectionSource subclass serving
// every saved type once GJS supports such vfuncs. Stops being enough when users lose rich text,
// a file list or both text and an image copied before a dictation.
async function saveClipboard(clipboard) {
    const selection = global.display.get_selection();
    const mimetypes = selection.get_mimetypes(clipboard);
    const mimetype = mimetypes.includes(RESTORED_TEXT_MIMETYPE)
        ? RESTORED_TEXT_MIMETYPE
        : mimetypes.find(type => type.startsWith('image/'));
    if (!mimetype)
        return null;
    try {
        const output = Gio.MemoryOutputStream.new_resizable();
        await new Promise((resolve, reject) => selection.transfer_async(clipboard, mimetype, -1, output, null,
            (_selection, result) => {
                try {
                    selection.transfer_finish(result);
                    resolve();
                } catch (e) {
                    reject(e);
                }
            }));
        output.close(null);
        return Meta.SelectionSourceMemory.new(mimetype, output.steal_as_bytes());
    } catch (e) {
        console.log('PasteIntoApp: the clipboard could not be saved');
        return null;
    }
}

function sendKeys(device, keyvals) {
    for (const [keyval, state] of keyvals)
        device.notify_keyval(GLib.get_monotonic_time(), keyval, state);
}

const INTERFACE_XML = `
<node>
  <interface name="com.promptheus.Shell">
    <method name="SetShortcuts">
      <arg type="a{sas}" direction="in" name="bindings"/>
      <arg type="as" direction="out" name="failed"/>
    </method>
    <method name="GetPointer">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
      <arg type="i" direction="out" name="work_x"/>
      <arg type="i" direction="out" name="work_y"/>
      <arg type="i" direction="out" name="work_width"/>
      <arg type="i" direction="out" name="work_height"/>
    </method>
    <method name="PlaceWindow">
      <arg type="s" direction="in" name="title"/>
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="b" direction="in" name="activate"/>
      <arg type="b" direction="out" name="placed"/>
    </method>
    <method name="PlaceWindowAnchored">
      <arg type="s" direction="in" name="title"/>
      <arg type="i" direction="in" name="right"/>
      <arg type="i" direction="in" name="bottom"/>
      <arg type="b" direction="in" name="activate"/>
      <arg type="b" direction="out" name="placed"/>
    </method>
    <method name="PlaceWindowOnParent">
      <arg type="s" direction="in" name="title"/>
      <arg type="i" direction="in" name="dx"/>
      <arg type="i" direction="in" name="dy"/>
      <arg type="b" direction="in" name="activate"/>
      <arg type="b" direction="out" name="placed"/>
    </method>
    <method name="ActivateWindow">
      <arg type="s" direction="in" name="title"/>
      <arg type="b" direction="out" name="activated"/>
    </method>
    <method name="GetFocusedWmClass">
      <arg type="s" direction="out" name="wm_class"/>
    </method>
    <method name="ShowToast">
      <arg type="s" direction="in" name="level"/>
      <arg type="s" direction="in" name="title"/>
      <arg type="s" direction="in" name="message"/>
      <arg type="b" direction="in" name="monochromatic"/>
      <arg type="s" direction="in" name="action"/>
    </method>
    <method name="ShowRecordingWidget">
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="b" direction="in" name="has_position"/>
      <arg type="b" direction="in" name="monochromatic"/>
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
      <arg type="i" direction="out" name="work_x"/>
      <arg type="i" direction="out" name="work_y"/>
      <arg type="i" direction="out" name="work_width"/>
      <arg type="i" direction="out" name="work_height"/>
      <arg type="d" direction="out" name="scale"/>
    </method>
    <method name="UpdateRecordingWidget">
      <arg type="s" direction="in" name="state"/>
      <arg type="d" direction="in" name="level"/>
      <arg type="u" direction="in" name="elapsed_ms"/>
    </method>
    <method name="UpdateRecordingWidgetRetry">
      <arg type="u" direction="in" name="attempt"/>
      <arg type="u" direction="in" name="max_attempts"/>
      <arg type="u" direction="in" name="delay_ms"/>
    </method>
    <method name="UpdateRecordingWidgetModel">
      <arg type="a(ss)" direction="in" name="targets"/>
      <arg type="s" direction="in" name="selected_id"/>
      <arg type="s" direction="in" name="selected_label"/>
      <arg type="b" direction="in" name="target_locked"/>
      <arg type="a(ussssuu)" direction="in" name="items"/>
      <arg type="u" direction="in" name="item_rows"/>
      <arg type="t" direction="in" name="context_version"/>
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="s" direction="in" name="stop_chat_destination"/>
    </method>
    <method name="HideRecordingWidget"/>
    <method name="PasteIntoApp">
      <arg type="s" direction="in" name="wm_class"/>
      <arg type="as" direction="in" name="uris"/>
      <arg type="s" direction="out" name="outcome"/>
    </method>
    <method name="RestoreAppWindow">
      <arg type="s" direction="in" name="wm_class"/>
      <arg type="s" direction="out" name="state"/>
    </method>
    <method name="WatchAppTitles">
      <arg type="s" direction="in" name="wm_class"/>
      <arg type="u" direction="in" name="duration_ms"/>
    </method>
    <method name="CaptureArea">
      <arg type="s" direction="in" name="path"/>
      <arg type="s" direction="in" name="owner"/>
      <arg type="s" direction="out" name="outcome"/>
    </method>
    <method name="CaptureSelection">
      <arg type="s" direction="in" name="owner"/>
      <arg type="s" direction="out" name="outcome"/>
      <arg type="s" direction="out" name="text"/>
    </method>
    <method name="CancelCapture">
      <arg type="s" direction="in" name="owner"/>
      <arg type="b" direction="out" name="cancelled"/>
    </method>
    <signal name="ToastActivated">
      <arg type="s" name="action"/>
    </signal>
    <signal name="ShortcutActivated">
      <arg type="s" name="action"/>
    </signal>
    <signal name="RecordingWidgetAction">
      <arg type="s" name="action"/>
    </signal>
    <signal name="RecordingWidgetTargetSelected">
      <arg type="s" name="id"/>
    </signal>
    <signal name="RecordingWidgetItemRemoved">
      <arg type="u" name="index"/>
      <arg type="t" name="context_version"/>
    </signal>
    <signal name="RecordingWidgetMoved">
      <arg type="i" name="x"/>
      <arg type="i" name="y"/>
      <arg type="i" name="work_x"/>
      <arg type="i" name="work_y"/>
      <arg type="i" name="work_width"/>
      <arg type="i" name="work_height"/>
      <arg type="d" name="scale"/>
    </signal>
    <signal name="Ready"/>
  </interface>
</node>`;

export default class PromptheusExtension extends Extension {
    enable() {
        this._grabbed = new Map();
        this._pending = new Map();
        this._selection = null;
        this._activations = new Map();
        this._overlayStops = new Set();
        this._titleWatch = null;
        this._anchors = new Map();
        this._watchId = 0;
        this._toasts = [];
        this._toastMonitor = 0;
        this._restackId = 0;
        this._widget = null;
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._acceleratorId = global.display.connect(
            'accelerator-activated',
            (display, action, device, timestamp) => this._onAcceleratorActivated(action));
        this._mapId = global.window_manager.connect_after('map', (shellwm, actor) => this._pivotProviderMenuAtTop(actor));
        this._destroyId = global.window_manager.connect_after('destroy', (shellwm, actor) => this._pivotProviderMenuAtTop(actor));
        this._overlayCreatedId = global.display.connect(
            'window-created', (display, win) => this._typeOverlayWhenReady(win));
        for (const actor of global.get_window_actors())
            this._typeOverlayWhenReady(actor.get_meta_window());
        this._dbus.emit_signal('Ready', null);
    }

    _typeOverlay(win) {
        const title = win.get_title();
        if (win.get_wm_class() !== APP_WM_CLASS || !OVERLAY_TITLES.has(title))
            return false;
        win.set_type(Meta.WindowType.UTILITY);
        if (title === IMAGE_PREVIEW_TITLE)
            Main.activateWindow(win);
        return true;
    }

    _typeOverlayWhenReady(win) {
        if (this._typeOverlay(win))
            return;
        const stop = this._watchWindow(win, this._overlayStops, () => {
            if (this._typeOverlay(win))
                stop();
        });
    }

    _stopTitleWatch() {
        const watch = this._titleWatch;
        if (!watch)
            return;
        this._titleWatch = null;
        global.display.disconnect(watch.createdId);
        if (watch.timeoutId)
            GLib.source_remove(watch.timeoutId);
        for (const stop of [...watch.stops])
            stop();
        console.log('WatchAppTitles: end');
    }

    async RestoreAppWindowAsync(params, invocation) {
        const [wmClass] = params;
        const finish = state => {
            console.log(`RestoreAppWindow: ${state}`);
            invocation.return_value(new GLib.Variant('(s)', [state]));
        };
        try {
            const win = global.get_window_actors()
                .map(actor => actor.get_meta_window())
                .find(candidate => isMainWindowOf(candidate, wmClass));
            if (!win)
                return finish('absent');
            if (!win.minimized && global.display.focus_window === win)
                return finish('shown');
            const cause = win.minimized ? 'minimized' : 'unfocused';
            const startUs = GLib.get_monotonic_time();
            Main.activateWindow(win);
            const restored = await waitFor(
                () => !win.minimized && global.display.focus_window === win, RESTORE_TIMEOUT_MS);
            console.log(`RestoreAppWindow: ${cause}, restore wait ${elapsedMsSince(startUs)} ms`);
            finish(restored ? 'restored' : 'restore_timeout');
        } catch (e) {
            logError(e, 'Promptheus: RestoreAppWindow failed');
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(e));
        }
    }

    WatchAppTitles(wmClass, durationMs) {
        this._stopTitleWatch();
        const startUs = GLib.get_monotonic_time();
        const stops = new Set();
        const track = (win, created) => {
            let matched = isMainWindowOf(win, wmClass);
            if (!matched && !created)
                return;
            if (matched && created)
                console.log(`WatchAppTitles: window ${win.get_id()} created +${elapsedMsSince(startUs)} ms`);
            this._watchWindow(win, stops, (_, pspec) => {
                if (!matched) {
                    matched = isMainWindowOf(win, wmClass);
                    if (matched)
                        console.log(`WatchAppTitles: window ${win.get_id()} created +${elapsedMsSince(startUs)} ms`);
                } else if (pspec?.name === 'title') {
                    console.log(`WatchAppTitles: title change window ${win.get_id()} +${elapsedMsSince(startUs)} ms`);
                }
            });
        };
        const watch = {stops, createdId: 0, timeoutId: 0};
        this._titleWatch = watch;
        watch.createdId = global.display.connect('window-created', (display, win) => track(win, true));
        watch.timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, durationMs, () => {
            watch.timeoutId = 0;
            this._stopTitleWatch();
            return GLib.SOURCE_REMOVE;
        });
        const existing = global.get_window_actors().map(actor => actor.get_meta_window());
        for (const win of existing)
            track(win, false);
        console.log(`WatchAppTitles: start (${stops.size} windows)`);
    }

    _watchWindow(win, stops, onChange) {
        const ids = [
            win.connect('notify::title', onChange),
            win.connect('notify::wm-class', onChange),
            win.connect('unmanaged', () => stop()),
        ];
        const stop = () => {
            for (const id of ids.splice(0))
                win.disconnect(id);
            stops.delete(stop);
        };
        stops.add(stop);
        return stop;
    }

    _pivotProviderMenuAtTop(actor) {
        if (actor.meta_window?.get_title() === PROVIDER_MENU_TITLE)
            actor.set_pivot_point(0.5, 0);
    }

    disable() {
        if (this._acceleratorId) {
            global.display.disconnect(this._acceleratorId);
            this._acceleratorId = 0;
        }
        if (this._mapId) {
            global.window_manager.disconnect(this._mapId);
            this._mapId = 0;
        }
        if (this._destroyId) {
            global.window_manager.disconnect(this._destroyId);
            this._destroyId = 0;
        }
        if (this._overlayCreatedId) {
            global.display.disconnect(this._overlayCreatedId);
            this._overlayCreatedId = 0;
        }
        for (const stop of [...this._overlayStops])
            stop();
        this._stopTitleWatch();
        this._unwatchSender();
        this._ungrabAll();
        for (const pending of [...this._pending.values()])
            pending.finish(false);
        this._selection?.finish('cancelled');
        for (const activation of [...this._activations.values()])
            activation.finish(false);
        for (const anchor of [...this._anchors.values()])
            this._releaseAnchor(anchor);
        this._clearToasts();
        this._removeRecordingWidget();
        this._pending = null;
        this._selection = null;
        this._activations = null;
        this._overlayStops = null;
        this._anchors = null;
        this._grabbed = null;
        this._dbus.unexport();
        this._dbus = null;
    }

    SetShortcutsAsync(params, invocation) {
        try {
            const [bindings] = params;
            this._ungrabAll();
            const failed = [];
            for (const [action, accelerators] of Object.entries(bindings)) {
                for (const accelerator of accelerators) {
                    const id = global.display.grab_accelerator(accelerator, Meta.KeyBindingFlags.NONE);
                    if (id === Meta.KeyBindingAction.NONE) {
                        failed.push(accelerator);
                        continue;
                    }
                    Main.wm.allowKeybinding(
                        Meta.external_binding_name_for_action(id),
                        Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW);
                    this._grabbed.set(id, action);
                }
            }
            this._watchSender(invocation.get_sender());
            invocation.return_value(new GLib.Variant('(as)', [failed]));
        } catch (e) {
            logError(e, 'Promptheus: SetShortcuts failed');
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(e));
        }
    }

    GetPointer() {
        const [x, y] = global.get_pointer();
        const monitor = global.display.get_current_monitor();
        const area = Main.layoutManager.getWorkAreaForMonitor(monitor);
        return [x, y, area.x, area.y, area.width, area.height];
    }

    PlaceWindowAsync(params, invocation) {
        const [title, x, y, activate] = params;
        this._placeOrWait('PlaceWindow', title, invocation,
            win => this._placeNow(win, x, y, activate));
    }

    PlaceWindowAnchoredAsync(params, invocation) {
        const [title, right, bottom, activate] = params;
        this._placeOrWait('PlaceWindowAnchored', title, invocation,
            win => this._anchorWindow(win, title, right, bottom, activate));
    }

    PlaceWindowOnParentAsync(params, invocation) {
        const [title, dx, dy, activate] = params;
        this._placeOrWait('PlaceWindowOnParent', title, invocation,
            win => this._placeOnParent(win, title, dx, dy, activate));
    }

    ActivateWindowAsync(params, invocation) {
        const [title] = params;
        try {
            this._activations.get(title)?.finish(false);
            const win = global.get_window_actors()
                .map(actor => actor.get_meta_window())
                .find(candidate => isAppWindow(candidate, title));
            if (win) {
                Main.activateWindow(win);
                invocation.return_value(new GLib.Variant('(b)', [true]));
                return;
            }
            this._waitForActivation(title, invocation);
        } catch (e) {
            logError(e, 'Promptheus: ActivateWindow failed');
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(e));
        }
    }

    async PasteIntoAppAsync(params, invocation) {
        const [wmClass, uris] = params;
        const clipboard = Meta.SelectionType.SELECTION_CLIPBOARD;
        const state = {cold: false, restored: false};
        const startUs = GLib.get_monotonic_time();
        const step = text => console.log(`PasteIntoApp: ${text} +${elapsedMsSince(startUs)} ms`);
        const finish = outcome => {
            console.log(`PasteIntoApp: outcome ${outcome} (${state.cold ? 'cold' : 'warm'}, restored ${state.restored})`);
            invocation.return_value(new GLib.Variant('(s)', [outcome]));
        };
        const findWindow = () => global.get_window_actors()
            .map(actor => actor.get_meta_window())
            .find(candidate => isMainWindowOf(candidate, wmClass));
        try {
            let win = findWindow();
            const focused = () => global.display.focus_window === win;
            if (!win) {
                state.cold = true;
                await waitFor(() => (win = findWindow()), PASTE_WINDOW_TIMEOUT_MS, PASTE_WINDOW_POLL_MS);
            }
            if (!win)
                return finish('no_window');
            step(`window found (${state.cold ? 'cold' : 'warm'})`);
            Main.activateWindow(win);
            if (!await waitFor(focused, PASTE_FOCUS_TIMEOUT_MS))
                return finish('focus_lost');
            step('focused');
            if (state.cold) {
                await sleep(PASTE_COLD_SETTLE_MS);
                step('settle done');
            }
            const keyboard = Clutter.get_default_backend().get_default_seat()
                .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
            let saved = null;
            if (uris.length > 0) {
                saved = await saveClipboard(clipboard);
                global.display.get_selection().set_owner(clipboard, fileListSource(uris));
                if (!focused())
                    return finish('focus_lost');
                const {PRESSED, RELEASED} = Clutter.KeyState;
                sendKeys(keyboard, [
                    [Clutter.KEY_Control_L, PRESSED], [Clutter.KEY_v, PRESSED],
                    [Clutter.KEY_v, RELEASED], [Clutter.KEY_Control_L, RELEASED],
                ]);
                step(`files pasted, Ctrl+V sent (${uris.length} URIs)`);
                // Fixed delay instead of a read confirmation: Meta.SelectionSourceMemory reports no
                // read. Claude Desktop reads text/uri-list at Ctrl+V and its message waits for the
                // uploads (Enter 1.08 s after pasting 4 x 25.5 MB images sent all of them). Upgrade
                // path: count reads in a Meta.SelectionSource subclass once GJS supports overriding
                // read_async. Stops being correct when Claude Desktop has not taken the paste within
                // the delay (busy machine): Enter then sends the text without the files.
                await sleep(PASTE_READ_DELAY_MS);
            }
            if (!focused())
                return finish('focus_lost');
            sendKeys(keyboard, [[Clutter.KEY_Return, Clutter.KeyState.PRESSED], [Clutter.KEY_Return, Clutter.KeyState.RELEASED]]);
            step('Return sent');
            if (saved) {
                await sleep(PASTE_RESTORE_DELAY_MS);
                global.display.get_selection().set_owner(clipboard, saved);
                state.restored = true;
                step('clipboard restored');
            }
            finish('sent');
        } catch (e) {
            logError(e, 'Promptheus: PasteIntoApp failed');
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(e));
        }
    }

    async CaptureAreaAsync(params, invocation) {
        const [path, owner] = params;
        const widget = this._widget;
        if (widget) {
            this._closeDropdown(widget);
            this._closePreview(widget);
            this._closeTooltip(widget);
            widget.actor.hide();
        }
        const ownerWindow = owner === '' ? null : this._findWindow(owner) ?? null;
        const ownerFocused = !!ownerWindow && global.display.focus_window === ownerWindow;
        ownerWindow?.get_compositor_private()?.hide();
        console.log(`CaptureArea: owner ${owner === '' ? 'pill' : ownerWindow ? 'window' : 'window not found'}`);
        let outcome = 'cancelled';
        let failure = null;
        try {
            const area = await this._selectCaptureArea();
            if (area) {
                await sleep(CAPTURE_FRAME_WAIT_MS);
                await this._writeAreaPng(area, path);
                outcome = 'captured';
            }
            console.log(`CaptureArea: outcome ${outcome}` + (area ? ` (${area.width}x${area.height})` : ''));
        } catch (e) {
            failure = e;
        }
        ownerWindow?.get_compositor_private()?.show();
        if (ownerFocused)
            Main.activateWindow(ownerWindow);
        this._widget?.actor.show();
        if (failure) {
            logError(failure, 'Promptheus: CaptureArea failed');
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(failure));
        } else {
            invocation.return_value(new GLib.Variant('(s)', [outcome]));
        }
    }

    CaptureSelectionAsync(params, invocation) {
        const [owner] = params;
        const widget = this._widget;
        const win = owner === '' ? null : this._findWindow(owner) ?? null;
        if (owner === '' ? !widget : !win) {
            console.log(`CaptureSelection: ${owner === '' ? 'pill not shown' : 'owner window not found'}`);
            invocation.return_value(new GLib.Variant('(ss)', ['cancelled', '']));
            return;
        }
        const previous = this._selection;
        previous?.finish('cancelled');
        if (previous?.owner === '' && owner !== '' && this._widget)
            this._fillRow2(this._widget);
        const primary = Meta.SelectionType.SELECTION_PRIMARY;
        const clipboard = Meta.SelectionType.SELECTION_CLIPBOARD;
        const display = global.display.get_selection();
        const held = Clutter.ModifierType.BUTTON1_MASK | Clutter.ModifierType.SHIFT_MASK;
        let done = false;
        let busy = false;
        let timerId = 0;
        const read = clipboardType => St.Clipboard.get_default().get_text(clipboardType, (_clipboard, text) => {
            if (done)
                return;
            if (text)
                selection.finish('captured', text);
            else
                busy = false;
        });
        const awaitRelease = clipboardType => {
            timerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SELECTION_POLL_MS, () => {
                const [, , modifiers] = global.get_pointer();
                if (modifiers & held)
                    return GLib.SOURCE_CONTINUE;
                timerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SELECTION_SETTLE_MS, () => {
                    timerId = 0;
                    read(clipboardType);
                    return GLib.SOURCE_REMOVE;
                });
                return GLib.SOURCE_REMOVE;
            });
        };
        let buttonDown = false;
        let gestureHeld = false;
        let gestureReleasedUs = null;
        const gestureId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SELECTION_POLL_MS, () => {
            const [x, y, modifiers] = global.get_pointer();
            const down = (modifiers & Clutter.ModifierType.BUTTON1_MASK) !== 0;
            if (down && !buttonDown) {
                gestureHeld = !pointInRect(x, y, owner === '' ? widget.actor : win.get_frame_rect());
            } else if (!down && gestureHeld) {
                gestureHeld = false;
                gestureReleasedUs = GLib.get_monotonic_time();
            }
            buttonDown = down;
            return GLib.SOURCE_CONTINUE;
        });
        const inGesture = () => gestureHeld ||
            (gestureReleasedUs !== null && elapsedMsSince(gestureReleasedUs) <= SELECTION_GESTURE_WINDOW_MS);
        // CLIPBOARD counts for apps that copy on select (herdr via OSC 52) and set no PRIMARY, only while button 1
        // is held for a press outside the pill or up to SELECTION_GESTURE_WINDOW_MS after its release. Wrong when a
        // CLIPBOARD write that is not a selection lands in that window (a background write or a clipboard-manager
        // takeover during a drag): it is taken as the selection. Upgrade path: the writers this app controls
        // (backend clipboard writes, the PasteIntoApp restore) tell the wait to skip their owner change; writes of
        // other clients stay indistinguishable, the owner-changed source names no client.
        const ownerId = display.connect('owner-changed', (_display, type, source) => {
            if ((type !== primary && type !== clipboard) || busy || !source ||
                !source.get_mimetypes().some(mimetype => SELECTION_TEXT_MIMETYPES.includes(mimetype)))
                return;
            if (win && global.display.focus_window === win) {
                console.log('CaptureSelection: owner window focused, ignored');
                return;
            }
            const fromClipboard = type === clipboard;
            if (fromClipboard && !inGesture()) {
                console.log('CaptureSelection: CLIPBOARD owner changed outside a mouse gesture, ignored');
                return;
            }
            busy = true;
            console.log(`CaptureSelection: ${fromClipboard ? 'CLIPBOARD' : 'PRIMARY'} owner changed`);
            awaitRelease(fromClipboard ? St.ClipboardType.CLIPBOARD : St.ClipboardType.PRIMARY);
        });
        const selection = {
            owner,
            win,
            finish: (outcome, text = '') => {
                if (done)
                    return;
                done = true;
                display.disconnect(ownerId);
                GLib.source_remove(gestureId);
                if (timerId)
                    GLib.source_remove(timerId);
                if (this._selection === selection)
                    this._selection = null;
                console.log(`CaptureSelection: outcome ${outcome} (${text.length} chars)`);
                invocation.return_value(new GLib.Variant('(ss)', [outcome, text]));
                if (outcome === 'captured' && owner === '' && this._widget === widget)
                    this._fillRow2(widget);
            },
        };
        this._selection = selection;
        if (owner === '')
            this._fillRow2(widget);
    }

    CancelCaptureAsync(params, invocation) {
        const [owner] = params;
        const cancelled = this._selection?.owner === owner;
        if (cancelled) {
            this._selection.finish('cancelled');
            if (owner === '' && this._widget)
                this._fillRow2(this._widget);
        }
        invocation.return_value(new GLib.Variant('(b)', [cancelled]));
    }

    _pillSelection() {
        return this._selection?.owner === '' ? this._selection : null;
    }

    _cancelSelectionWaiting(widget) {
        this._pillSelection()?.finish('cancelled');
        this._fillRow2(widget);
    }

    _selectCaptureArea() {
        return new Promise(resolve => {
            const [stageWidth, stageHeight] = global.stage.get_size();
            const overlay = new St.Widget({
                reactive: true,
                width: stageWidth,
                height: stageHeight,
                style: 'background-color: rgba(0, 0, 0, 0.4);',
            });
            const frame = new St.Widget({
                visible: false,
                style: 'background-color: rgba(255, 255, 255, 0.15); border: 1px solid white;',
            });
            overlay.add_child(frame);
            overlay.set_cursor_type(Clutter.CursorType.CROSSHAIR);
            Main.layoutManager.uiGroup.add_child(overlay);
            const grab = Main.pushModal(overlay);

            let start = null;
            const areaTo = ([x, y]) => ({
                x: Math.round(Math.min(start[0], x)),
                y: Math.round(Math.min(start[1], y)),
                width: Math.round(Math.abs(x - start[0])),
                height: Math.round(Math.abs(y - start[1])),
            });
            const finish = area => {
                Main.popModal(grab);
                overlay.destroy();
                resolve(area);
            };

            overlay.connect('key-press-event', (_actor, event) => {
                if (event.get_key_symbol() === Clutter.KEY_Escape)
                    finish(null);
                return Clutter.EVENT_STOP;
            });
            overlay.connect('button-press-event', (_actor, event) => {
                if (event.get_button() === Clutter.BUTTON_PRIMARY)
                    start = event.get_coords();
                else if (event.get_button() === Clutter.BUTTON_SECONDARY)
                    finish(null);
                return Clutter.EVENT_STOP;
            });
            overlay.connect('motion-event', (_actor, event) => {
                if (start) {
                    const {x, y, width, height} = areaTo(event.get_coords());
                    frame.set_position(x, y);
                    frame.set_size(width, height);
                    frame.show();
                }
                return Clutter.EVENT_STOP;
            });
            overlay.connect('button-release-event', (_actor, event) => {
                if (start && event.get_button() === Clutter.BUTTON_PRIMARY) {
                    const area = areaTo(event.get_coords());
                    finish(area.width < CAPTURE_MIN_SIZE_PX || area.height < CAPTURE_MIN_SIZE_PX ? null : area);
                }
                return Clutter.EVENT_STOP;
            });
        });
    }

    async _writeAreaPng({x, y, width, height}, path) {
        const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.NONE, null);
        try {
            await new Shell.Screenshot().screenshot_area(x, y, width, height, stream);
        } finally {
            stream.close(null);
        }
    }

    GetFocusedWmClass() {
        return global.display.focus_window?.get_wm_class() ?? '';
    }

    ShowToast(level, title, message, monochromatic, action) {
        const duration = TOAST_DURATIONS_MS[level];
        if (duration === undefined)
            throw new Error(`Unknown toast level: ${level}`);
        const actor = this._buildToast(level, title, message, monochromatic);
        if (this._toasts.length === 0) {
            this._toastMonitor = global.display.get_current_monitor();
            global.compositor.disable_unredirect();
        }
        const toast = {actor, timerId: 0, activated: false};
        if (action !== '')
            this._makeToastClickable(toast, action);
        this._toasts.push(toast);
        toast.timerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, duration, () => {
            toast.timerId = 0;
            this._hideToast(toast);
            return GLib.SOURCE_REMOVE;
        });
        if (!this._restackId) {
            this._restackId = global.compositor.get_laters().add(Meta.LaterType.BEFORE_REDRAW, () => {
                this._restackId = 0;
                this._restackToasts();
                return GLib.SOURCE_REMOVE;
            });
        }
        Main.layoutManager.uiGroup.add_child(actor);
        actor.ease({
            opacity: TOAST_OPACITY,
            scale_x: 1,
            scale_y: 1,
            duration: TOAST_SHOW_ANIMATION_MS,
            mode: Clutter.AnimationMode.EASE_OUT_EXPO,
        });
    }

    _makeToastClickable(toast, action) {
        toast.actor.reactive = true;
        toast.actor.connect('button-press-event', () => {
            if (toast.activated)
                return Clutter.EVENT_STOP;
            toast.activated = true;
            this._dbus.emit_signal('ToastActivated', new GLib.Variant('(s)', [action]));
            if (toast.timerId) {
                GLib.Source.remove(toast.timerId);
                toast.timerId = 0;
            }
            this._hideToast(toast);
            return Clutter.EVENT_STOP;
        });
    }

    _hideToast(toast) {
        toast.actor.set_pivot_point(0.5, 0.5);
        toast.actor.ease({
            opacity: 0,
            scale_x: 0.8,
            scale_y: 0.8,
            duration: TOAST_HIDE_ANIMATION_MS,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onStopped: () => this._removeToast(toast),
        });
    }

    _removeToast(toast) {
        if (!this._toasts.includes(toast))
            return;
        this._toasts = this._toasts.filter(t => t !== toast);
        if (this._toasts.length === 0)
            global.compositor.enable_unredirect();
        toast.actor.destroy();
        this._restackToasts();
    }

    _buildToast(level, title, message, monochromatic) {
        const actor = new St.BoxLayout({
            style: 'width: 300px; background-color: #ffffff; border: 1px solid rgba(200,200,200,0.9); ' +
                'border-radius: 8px; padding: 12px 16px; spacing: 10px; ' +
                'font-family: "Inter", "Noto Sans", sans-serif; box-shadow: 0 4px 12px rgba(0,0,0,0.15);',
            opacity: 0,
            scale_x: 0.01,
            scale_y: 0.05,
            reactive: false,
        });
        actor.set_pivot_point(0.5, 1.0);
        const file = this.dir.get_child('icons').get_child(`${level}-${monochromatic ? 'mono' : 'color'}.svg`);
        actor.add_child(new St.Icon({
            gicon: new Gio.FileIcon({file}),
            icon_size: 20,
            y_align: Clutter.ActorAlign.CENTER,
        }));
        const text = new St.BoxLayout({vertical: true, x_expand: true});
        text.add_child(new St.Label({
            text: title,
            style: 'color: #1a1a1a; font-weight: 600; font-size: 14px;',
        }));
        if (message !== '') {
            text.add_child(new St.Label({
                text: message,
                style: 'color: rgba(0,0,0,0.65); font-size: 13px;',
            }));
        }
        actor.add_child(text);
        return actor;
    }

    _restackToasts() {
        const area = Main.layoutManager.getWorkAreaForMonitor(this._toastMonitor);
        let bottom = area.y + area.height - TOAST_MARGIN;
        for (const {actor} of [...this._toasts].reverse()) {
            const [, width] = actor.get_preferred_width(-1);
            const [, height] = actor.get_preferred_height(width);
            const y = bottom - height;
            actor.set_position(area.x + area.width - TOAST_MARGIN - width, y);
            bottom = y - TOAST_GAP;
        }
    }

    _clearToasts() {
        const toasts = this._toasts;
        this._toasts = [];
        if (toasts.length > 0)
            global.compositor.enable_unredirect();
        for (const toast of toasts) {
            if (toast.timerId)
                GLib.Source.remove(toast.timerId);
            toast.actor.remove_all_transitions();
            toast.actor.destroy();
        }
        if (this._restackId) {
            global.compositor.get_laters().remove(this._restackId);
            this._restackId = 0;
        }
    }

    ShowRecordingWidget(x, y, hasPosition, monochromatic) {
        if (!this._widget) {
            this._widget = this._buildRecordingWidget();
            global.compositor.disable_unredirect();
        }
        const widget = this._widget;
        widget.monochromatic = monochromatic;
        this._pillSelection()?.finish('cancelled');
        widget.scales.fill(WIDGET_BAR_MIN_SCALE);
        widget.elapsed = formatElapsed(0);
        widget.model = emptyWidgetModel();
        this._closeDropdown(widget);
        this._fillItems(widget);
        this._setWidgetState(widget, 'recording');
        const monitors = Main.layoutManager.monitors;
        const workAreas = monitors.map((_monitor, i) => Main.layoutManager.getWorkAreaForMonitor(i));
        const pointerArea = Main.layoutManager.getWorkAreaForMonitor(global.display.get_current_monitor());
        const position = resolveWidgetPosition(hasPosition, x, y, workAreas, pointerArea);
        widget.actor.set_position(position.x, position.y);
        this._applyWidgetSize(widget);
        console.log(`Promptheus: recording widget shown at ${position.x},${position.y} ` +
            `stored=${position.stored}`);
        const area = workAreaAt(position.x, position.y);
        return [position.x, position.y, area.x, area.y, area.width, area.height,
            monitorScaleAt(position.x, position.y)];
    }

    UpdateRecordingWidget(state, level, elapsedMs) {
        if (!WIDGET_STATES.includes(state))
            throw new Error(`Unknown recording widget state: ${state}`);
        const widget = this._widget;
        if (!widget)
            throw new Error('Recording widget is not shown');
        widget.elapsed = formatElapsed(elapsedMs);
        if (state !== widget.state)
            this._setWidgetState(widget, state);
        else if (state === 'recording')
            this._easeWidgetBars(widget, level);
        if (widget.timeLabel && widget.timeLabel.text !== widget.elapsed)
            widget.timeLabel.text = widget.elapsed;
    }

    UpdateRecordingWidgetRetry(attempt, maxAttempts, delayMs) {
        const widget = this._widget;
        if (!widget)
            throw new Error('Recording widget is not shown');
        widget.retry = {attempt, maxAttempts, endUs: GLib.get_monotonic_time() + delayMs * 1000};
        this._setWidgetState(widget, 'retrying');
    }

    UpdateRecordingWidgetModel(targets, selectedId, selectedLabel, locked, items, itemRows, version, x, y, stopChatDestination) {
        const widget = this._widget;
        if (!widget)
            throw new Error('Recording widget is not shown');
        widget.model = {targets, selectedId, selectedLabel, locked, items, itemRows, version, stopChatDestination};
        widget.actor.set_position(x, y);
        this._fillRow2(widget);
        this._fillItems(widget);
        this._applyWidgetSize(widget);
        if (widget.dropdown)
            this._openDropdown(widget);
    }

    HideRecordingWidget() {
        this._removeRecordingWidget();
    }

    _removeRecordingWidget() {
        const widget = this._widget;
        if (!widget)
            return;
        this._widget = null;
        this._pillSelection()?.finish('cancelled');
        global.compositor.enable_unredirect();
        this._endWidgetDrag(widget);
        this._closeDropdown(widget);
        this._closePreview(widget);
        this._closeTooltip(widget);
        this._clearWidgetContent(widget);
        widget.actor.remove_all_transitions();
        Main.layoutManager.removeChrome(widget.actor);
        widget.actor.destroy();
        console.log('Promptheus: recording widget hidden');
    }

    _emitRecordingWidgetAction(action) {
        this._dbus.emit_signal('RecordingWidgetAction', new GLib.Variant('(s)', [action]));
    }

    _emitRecordingWidgetTargetSelected(id) {
        this._dbus.emit_signal('RecordingWidgetTargetSelected', new GLib.Variant('(s)', [id]));
    }

    _emitRecordingWidgetItemRemoved(index, version) {
        this._dbus.emit_signal('RecordingWidgetItemRemoved', new GLib.Variant('(ut)', [index, version]));
    }

    _widgetRow(height) {
        return new St.BoxLayout({style: `height: ${height}px; padding: 0 ${WIDGET_PADDING_X}px;`});
    }

    _buildRecordingWidget() {
        const actor = new St.BoxLayout({
            vertical: true,
            style: `border-radius: ${WIDGET_RADIUS}px; background-color: rgba(28,28,30,0.92); ` +
                'color: #e5e5e7; font-family: "Inter", "Noto Sans", sans-serif; font-size: 13px; ' +
                'box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12);',
            reactive: true,
            can_focus: false,
            track_hover: false,
        });
        const widget = {
            actor,
            state: null,
            model: emptyWidgetModel(),
            itemsRow: this._widgetRow(0),
            preview: null,
            previewId: 0,
            tooltip: null,
            tooltipId: 0,
            tooltipHiddenAt: -Infinity,
            row1: new St.Widget({
                layout_manager: new Clutter.BinLayout(),
                style: `height: ${WIDGET_ROW_HEIGHT}px; padding: 0 ${WIDGET_PADDING_X}px;`,
            }),
            row2: this._widgetRow(WIDGET_ROW_HEIGHT),
            dropdown: null,
            elapsed: formatElapsed(0),
            scales: WIDGET_BAR_FACTORS.map(() => WIDGET_BAR_MIN_SCALE),
            bars: [],
            timeLabel: null,
            waveBars: [],
            waveStart: 0,
            waveId: 0,
            retry: null,
            retryLabel: null,
            retryId: 0,
            lockedLabel: null,
            lockedTarget: null,
            dragId: 0,
        };
        widget.itemsRow.visible = false;
        widget.row2.style += ' spacing: 6px;';
        actor.add_child(widget.itemsRow);
        actor.add_child(widget.row1);
        actor.add_child(widget.row2);
        actor.connect('button-press-event', (_actor, event) => this._beginWidgetDrag(widget, event));
        Main.layoutManager.addTopChrome(actor);
        return widget;
    }

    _beginWidgetDrag(widget, event) {
        if (event.get_button() !== Clutter.BUTTON_PRIMARY)
            return Clutter.EVENT_PROPAGATE;
        if (this._isInWidgetButton(widget, global.stage.get_event_actor(event)))
            return Clutter.EVENT_PROPAGATE;
        this._endWidgetDrag(widget);
        const [pressX, pressY] = event.get_coords();
        const offsetX = pressX - widget.actor.x;
        const offsetY = pressY - widget.actor.y;
        let dragging = false;
        widget.dragId = global.stage.connect('captured-event', (_stage, e) => {
            const type = e.type();
            const primaryLifted = type === Clutter.EventType.MOTION &&
                !(e.get_state() & Clutter.ModifierType.BUTTON1_MASK);
            if (type === Clutter.EventType.BUTTON_RELEASE || primaryLifted) {
                this._endWidgetDrag(widget);
                if (!dragging)
                    return Clutter.EVENT_PROPAGATE;
                this._emitRecordingWidgetMoved(widget);
                return Clutter.EVENT_STOP;
            }
            if (type === Clutter.EventType.MOTION) {
                const [x, y] = e.get_coords();
                if (!dragging && Math.hypot(x - pressX, y - pressY) <= WIDGET_DRAG_THRESHOLD_PX)
                    return Clutter.EVENT_PROPAGATE;
                if (!dragging) {
                    this._closeDropdown(widget);
                    this._closePreview(widget);
                    this._closeTooltip(widget);
                }
                dragging = true;
                widget.actor.set_position(Math.round(x - offsetX), Math.round(y - offsetY));
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });
        return Clutter.EVENT_STOP;
    }

    _isInWidgetButton(widget, actor) {
        for (let current = actor; current && current !== widget.actor; current = current.get_parent()) {
            if (current instanceof St.Button || current.isWidgetChip)
                return true;
        }
        return false;
    }

    _emitRecordingWidgetMoved(widget) {
        const x = Math.round(widget.actor.x);
        const y = Math.round(widget.actor.y);
        console.log(`Promptheus: recording widget dragged to ${x},${y}`);
        const area = workAreaAt(x, y);
        this._dbus.emit_signal('RecordingWidgetMoved',
            new GLib.Variant('(iiiiiid)', [x, y, area.x, area.y, area.width, area.height, monitorScaleAt(x, y)]));
    }

    _endWidgetDrag(widget) {
        if (widget.dragId) {
            global.stage.disconnect(widget.dragId);
            widget.dragId = 0;
        }
    }

    _clearWidgetContent(widget, fadeOut = false) {
        widget.bars.forEach(bar => bar.remove_all_transitions());
        // An outgoing wave stops moving during its WIDGET_SWAP_MS fade-out (the webview one keeps moving); unnoticeable
        // while the fade is short against the 1.2 s wave period. If the fade grows to a visible fraction of the period,
        // give each content its own wave timer, removed when that content is destroyed.
        this._stopWidgetWave(widget);
        if (widget.retryId) {
            GLib.Source.remove(widget.retryId);
            widget.retryId = 0;
        }
        widget.bars = [];
        widget.timeLabel = null;
        widget.retryLabel = null;
        const children = widget.row1.get_children();
        const outgoing = fadeOut ? children.pop() : null;
        children.forEach(child => child.destroy());
        if (outgoing) {
            this._setUnreactive(outgoing);
            this._easeWidgetOpacity(outgoing, 0);
        }
    }

    _setUnreactive(actor) {
        actor.reactive = false;
        actor.get_children().forEach(child => this._setUnreactive(child));
    }

    _easeWidgetOpacity(actor, opacity) {
        actor.ease({opacity, duration: WIDGET_SWAP_MS, mode: Clutter.AnimationMode.EASE_OUT_CUBIC});
    }

    _applyWidgetSize(widget) {
        const height = WIDGET_HEIGHT + widgetItemsHeight(widget.model.itemRows);
        widget.actor.set_size(WIDGET_WIDTH, height);
    }

    _widgetContent() {
        return new St.BoxLayout({
            x_expand: true,
            y_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            style: `spacing: ${WIDGET_ROW_SPACING}px;`,
        });
    }

    _setWidgetState(widget, state) {
        const crossfade = state !== widget.state && WIDGET_CROSSFADE_STATES.includes(state);
        this._clearWidgetContent(widget, crossfade);
        widget.state = state;
        const content = this._widgetContent();
        if (state === 'recording' || state === 'paused') {
            content.x_align = Clutter.ActorAlign.FILL;
            this._fillControls(widget, content, state);
        }
        else if (state === 'processing')
            this._fillWave(widget, content);
        else if (state === 'retrying')
            this._fillRetry(widget, content);
        else
            this._fillDone(widget, content);
        widget.row1.add_child(content);
        if (state === 'done') {
            content.set_pivot_point(0.5, 0.5);
            content.set_scale(WIDGET_DONE_ENTER_SCALE, WIDGET_DONE_ENTER_SCALE);
            content.opacity = 0;
            content.ease({
                scale_x: 1,
                scale_y: 1,
                opacity: 255,
                duration: WIDGET_DONE_ENTER_MS,
                mode: Clutter.AnimationMode.EASE_OUT_CUBIC,
            });
        }
        else if (crossfade) {
            content.opacity = 0;
            this._easeWidgetOpacity(content, 255);
        }
        this._fillRow2(widget);
    }

    _fillControls(widget, content, state) {
        const bars = this._widgetBarRow();
        if (state === 'paused')
            bars.opacity = WIDGET_WAVE_OPACITY;
        widget.bars = widget.scales.map(scale => this._widgetBar(bars, scale));
        widget.timeLabel = new St.Label({
            text: widget.elapsed,
            y_align: Clutter.ActorAlign.CENTER,
            style: `min-width: ${WIDGET_TIME_WIDTH}px;`,
        });
        content.add_child(bars);
        content.add_child(widget.timeLabel);
        content.add_child(this._widgetActions(
            state === 'paused'
                ? this._widgetActionButton('Resume', 'play', 'resume')
                : this._widgetActionButton('Pause', 'pause', 'pause'),
            this._widgetActionButton('Cancel', 'cancel', 'cancel', 'cancel-hover'),
            this._widgetActionButton('Finish', 'arrow-up', 'stop', undefined, WIDGET_BUTTON_PRIMARY)));
    }

    _widgetActions(...buttons) {
        const actions = new St.BoxLayout({
            x_expand: true,
            x_align: Clutter.ActorAlign.END,
            style: `spacing: ${WIDGET_ACTION_SPACING}px;`,
        });
        buttons.forEach(button => actions.add_child(button));
        return actions;
    }

    _widgetBarRow(spacing = WIDGET_BAR_SPACING, height = WIDGET_BAR_MAX_HEIGHT) {
        return new St.BoxLayout({
            y_align: Clutter.ActorAlign.CENTER,
            style: `spacing: ${spacing}px; height: ${height}px;`,
        });
    }

    _widgetBar(row, scale, width = WIDGET_BAR_WIDTH, height = WIDGET_BAR_MAX_HEIGHT) {
        const bar = new St.Widget({
            y_align: Clutter.ActorAlign.CENTER,
            style: `width: ${width}px; border-radius: 2px; background-color: #e5e5e7;`,
            height,
            scale_y: scale,
        });
        bar.set_pivot_point(0.5, 0.5);
        row.add_child(bar);
        return bar;
    }

    _retryRemainingMs(widget) {
        return (widget.retry.endUs - GLib.get_monotonic_time()) / 1000;
    }

    _fillRetry(widget, content) {
        content.x_align = Clutter.ActorAlign.FILL;
        if (this._retryRemainingMs(widget) > 0) {
            widget.retryLabel = new St.Label({y_align: Clutter.ActorAlign.CENTER});
            content.add_child(widget.retryLabel);
            this._armWidgetRetry(widget);
        }
        else
            this._fillWave(widget, content, WIDGET_RETRY_WAVE_BAR_COUNT);
        content.add_child(this._widgetActions(this._widgetActionButton('Cancel', 'cancel', 'cancel', 'cancel-hover')));
    }

    _armWidgetRetry(widget) {
        const remainingMs = this._retryRemainingMs(widget);
        widget.retryLabel.text = formatRetry(widget.retry.attempt, widget.retry.maxAttempts, remainingMs);
        widget.retryId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, Math.ceil(msUntilRetryTextChange(remainingMs)), () => {
            widget.retryId = 0;
            if (this._retryRemainingMs(widget) > 0)
                this._armWidgetRetry(widget);
            else
                this._showWidgetRetryWave(widget);
            return GLib.SOURCE_REMOVE;
        });
    }

    _showWidgetRetryWave(widget) {
        this._clearWidgetContent(widget);
        const content = this._widgetContent();
        this._fillRetry(widget, content);
        widget.row1.add_child(content);
    }

    _fillWave(widget, content, barCount = WIDGET_WAVE_BAR_COUNT) {
        const row = this._widgetBarRow(WIDGET_BAR_SPACING, WIDGET_WAVE_HEIGHT);
        row.opacity = WIDGET_WAVE_OPACITY;
        widget.waveBars = Array.from({length: barCount},
            () => this._widgetBar(row, WIDGET_WAVE_MIN_SCALE, WIDGET_WAVE_BAR_WIDTH, WIDGET_WAVE_HEIGHT));
        content.add_child(row);
        widget.waveStart = GLib.get_monotonic_time();
        this._updateWidgetWave(widget);
        widget.waveId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, WIDGET_WAVE_FRAME_MS, () => {
            this._updateWidgetWave(widget);
            return GLib.SOURCE_CONTINUE;
        });
    }

    _updateWidgetWave(widget) {
        const elapsed = GLib.get_monotonic_time() - widget.waveStart;
        const phase = 2 * Math.PI * (elapsed % WIDGET_WAVE_PERIOD_US) / WIDGET_WAVE_PERIOD_US;
        widget.waveBars.forEach((bar, i) => {
            const wave = 0.5 + 0.5 * Math.sin(phase - i * 2 * Math.PI / widget.waveBars.length);
            bar.scale_y = (WIDGET_BAR_MIN_HEIGHT + WIDGET_WAVE_AMPLITUDE * wave) / WIDGET_WAVE_HEIGHT;
        });
    }

    _stopWidgetWave(widget) {
        if (widget.waveId) {
            GLib.Source.remove(widget.waveId);
            widget.waveId = 0;
        }
        widget.waveBars = [];
    }

    _fillDone(widget, content) {
        content.add_child(this._widgetIcon(widget.monochromatic ? 'check-mono' : 'check'));
        content.add_child(new St.Label({
            text: 'Copied',
            y_align: Clutter.ActorAlign.CENTER,
        }));
    }

    _widgetGicon(name) {
        return new Gio.FileIcon({file: this.dir.get_child('icons').get_child(`recording-${name}.svg`)});
    }

    _widgetIcon(name, size = 16) {
        return new St.Icon({
            gicon: this._widgetGicon(name),
            icon_size: size,
            y_align: Clutter.ActorAlign.CENTER,
        });
    }

    _widgetButton(accessibleName, child, onClick,
        baseStyle = `width: ${WIDGET_BUTTON_SIZE}px; height: ${WIDGET_BUTTON_SIZE}px; border-radius: ${WIDGET_BUTTON_SIZE / 2}px;`,
        isHeld = () => false, background = WIDGET_BUTTON_PLAIN) {
        const button = new St.Button({
            can_focus: false,
            reactive: true,
            track_hover: true,
            y_align: Clutter.ActorAlign.CENTER,
            style: baseStyle,
            accessible_name: accessibleName,
            child,
        });
        const paint = () => {
            button.style = `${baseStyle} background-color: ${button.hover || isHeld() ? background.hover : background.rest};`;
        };
        button.connect('notify::hover', paint);
        paint();
        button.connect('clicked', onClick);
        return button;
    }

    _selectTextButton(widget) {
        return this._widgetButton('Select text', this._widgetIcon('select-text'), () => {
            if (this._pillSelection())
                this._cancelSelectionWaiting(widget);
            else
                this._emitRecordingWidgetAction('select_text');
        }, undefined, () => !!this._pillSelection());
    }

    _widgetActionButton(accessibleName, iconName, action, hoverIconName = iconName, background = WIDGET_BUTTON_PLAIN) {
        const icon = this._widgetIcon(iconName);
        const button = this._widgetButton(accessibleName, icon, () => this._emitRecordingWidgetAction(action),
            undefined, undefined, background);
        if (hoverIconName !== iconName) {
            button.connect('notify::hover', () => {
                icon.gicon = this._widgetGicon(button.hover ? hoverIconName : iconName);
            });
        }
        return button;
    }

    _fillRow2(widget) {
        this._closeTooltip(widget);
        const {selectedLabel, locked, stopChatDestination} = widget.model;
        const interactive = !locked && widget.state !== 'done';
        if (!interactive) {
            this._pillSelection()?.finish('cancelled');
            this._closeDropdown(widget);
            if (widget.lockedLabel) {
                widget.lockedLabel.text = selectedLabel;
                widget.lockedTarget.accessible_name = selectedLabel;
                return;
            }
        }
        const fade = widget.row2.get_n_children() > 0;
        widget.row2.destroy_all_children();
        widget.row2.style = `height: ${WIDGET_ROW_HEIGHT}px; spacing: ${WIDGET_ROW_SPACING}px; ` +
            `padding: 0 ${WIDGET_PADDING_X}px 0 ${WIDGET_PADDING_X - WIDGET_TARGET_PADDING_X}px;`;
        const content = new St.BoxLayout({style: 'spacing: 4px;'});
        content.opacity = WIDGET_WAVE_OPACITY;
        const waiting = !!this._pillSelection();
        const labelText = waiting ? 'Select text…' : selectedLabel;
        const label = new St.Label({text: labelText, y_align: Clutter.ActorAlign.CENTER});
        content.add_child(label);
        const chevron = waiting ? null : this._widgetIcon('chevrons-up-down', 14);
        if (chevron)
            content.add_child(chevron);
        const target = this._widgetButton(labelText, content,
            () => waiting ? this._cancelSelectionWaiting(widget) : this._toggleDropdown(widget),
            `height: 28px; border-radius: 14px; padding: 0 ${WIDGET_TARGET_PADDING_X}px;`);
        const stopChatText = stopChatDestination ? `Stop and send to ${stopChatDestination}` : 'Stop and open chat';
        const actions = this._widgetActions(
            this._tooltipButton(widget, this._widgetActionButton('Screenshot', 'screenshot', 'screenshot'),
                'Add screen area to context'),
            this._tooltipButton(widget, this._selectTextButton(widget), 'Add selected text to context'),
            this._tooltipButton(widget, this._widgetActionButton('Paste', 'paste', 'paste'),
                'Add clipboard to context'),
            this._tooltipButton(widget,
                this._widgetActionButton(stopChatText, stopChatDestination ? 'provider-claude' : 'chat', 'stop_chat',
                    undefined, WIDGET_BUTTON_SECONDARY),
                stopChatText));
        widget.row2.add_child(target);
        widget.row2.add_child(actions);
        widget.lockedLabel = interactive ? null : label;
        widget.lockedTarget = interactive ? null : target;
        if (interactive)
            return;
        this._setUnreactive(widget.row2);
        for (const actor of [chevron, actions]) {
            if (fade)
                this._easeWidgetOpacity(actor, 0);
            else
                actor.opacity = 0;
        }
    }

    _fillItems(widget) {
        const {items, locked, itemRows, version} = widget.model;
        this._closePreview(widget);
        widget.itemsRow.destroy_all_children();
        widget.itemsRow.visible = items.length > 0;
        widget.itemsRow.style = `height: ${widgetItemsContentHeight(itemRows)}px; padding: ${WIDGET_ITEMS_ROW_PADDING / 2}px ${WIDGET_PADDING_X}px;`;
        const grid = new St.BoxLayout({vertical: true, style: `spacing: ${WIDGET_ITEM_ROW_GAP}px;`});
        const chipFor = ([index, kind, label, preview, imagePath, displayWidth, displayHeight]) =>
            this._widgetChip(widget, kind, label, preview, imagePath, [displayWidth, displayHeight],
                () => this._emitRecordingWidgetItemRemoved(index, version), locked);
        const images = items.filter(item => item[1] === 'image');
        const texts = items.filter(item => item[1] === 'text');
        for (let start = 0; start < images.length; start += WIDGET_IMAGE_COLUMNS) {
            const line = new St.BoxLayout({style: `spacing: ${WIDGET_ITEM_ROW_GAP}px;`});
            images.slice(start, start + WIDGET_IMAGE_COLUMNS).forEach(item => line.add_child(chipFor(item)));
            grid.add_child(line);
        }
        texts.forEach(item => {
            const line = new St.BoxLayout({style: `spacing: ${WIDGET_ITEM_ROW_GAP}px;`});
            line.add_child(chipFor(item));
            grid.add_child(line);
        });
        const scroll = new St.ScrollView({
            overlay_scrollbars: true,
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            x_expand: true,
            y_expand: true,
        });
        scroll.set_child(grid);
        widget.itemsRow.add_child(scroll);
    }

    _widgetChip(widget, kind, label, text, imagePath, displaySize, onRemove, locked) {
        const isImage = kind === 'image';
        const paddingLeft = isImage ? WIDGET_IMAGE_CHIP_PADDING_LEFT : WIDGET_CHIP_PADDING_LEFT;
        const paddingRight = isImage ? WIDGET_IMAGE_CHIP_PADDING_RIGHT : WIDGET_CHIP_PADDING_RIGHT;
        const spacing = isImage ? WIDGET_IMAGE_CHIP_SPACING : WIDGET_CHIP_SPACING;
        const width = isImage ? '' : `width: ${WIDGET_TEXT_CHIP_WIDTH - paddingLeft - paddingRight}px; `;
        const chip = new St.BoxLayout({
            reactive: true,
            track_hover: true,
            y_align: Clutter.ActorAlign.CENTER,
            style: `${width}height: ${WIDGET_ITEM_ROW_HEIGHT}px; padding: 0 ${paddingRight}px 0 ${paddingLeft}px; ` +
                `spacing: ${spacing}px; border-radius: ${WIDGET_CHIP_RADIUS}px; background-color: rgba(255,255,255,0.12); font-size: 11px;`,
        });
        chip.isWidgetChip = true;
        chip.add_child(isImage && imagePath ? this._widgetThumb(imagePath) : this._widgetIcon(`item-${kind}`, 12));
        const numberActor = new St.Label({
            text: label,
            y_align: Clutter.ActorAlign.CENTER,
            style: 'color: #929294; font-size: 11.5px;',
        });
        numberActor.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        chip.add_child(numberActor);
        if (!isImage) {
            const textActor = new St.Label({
                text: text.replace(/\s+/g, ' ').trim(),
                x_expand: true,
                y_align: Clutter.ActorAlign.CENTER,
                style: 'color: #e5e5e7;',
            });
            textActor.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            chip.add_child(textActor);
        }
        const removeButton = this._widgetButton(`Remove ${label}`, this._widgetIcon('item-remove', 12), onRemove,
            'width: 16px; height: 16px; border-radius: 8px;');
        if (locked) {
            removeButton.reactive = false;
            removeButton.track_hover = false;
            removeButton.opacity = WIDGET_CHIP_REMOVE_LOCKED_OPACITY;
        }
        chip.add_child(removeButton);
        chip.connect('notify::hover', () => {
            if (chip.hover)
                this._schedulePreview(widget, chip, text, imagePath, displaySize);
            else
                this._closePreview(widget);
        });
        return chip;
    }

    _widgetThumb(imagePath) {
        return new St.Widget({
            y_align: Clutter.ActorAlign.CENTER,
            width: WIDGET_THUMB_SIZE,
            height: WIDGET_THUMB_SIZE,
            style: `border-radius: 5px; box-shadow: 0 0 0 1px rgba(255,255,255,0.18); background-image: url("file://${imagePath}"); ` +
                'background-size: cover; background-position: center; background-repeat: no-repeat;',
        });
    }

    _schedulePreview(widget, chip, text, imagePath, displaySize) {
        this._closePreview(widget);
        if (widget.dropdown || (!text && !imagePath))
            return;
        widget.previewId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, WIDGET_PREVIEW_DELAY_MS, () => {
            widget.previewId = 0;
            this._showPreview(widget, text, imagePath, displaySize);
            return GLib.SOURCE_REMOVE;
        });
    }

    _showPreview(widget, text, imagePath, displaySize) {
        if (widget.dropdown)
            return;
        const style = 'background-color: #252629; border: 1px solid rgba(255,255,255,0.12); border-radius: 6px; ' +
            'color: #e5e5e7; font-family: "Inter", "Noto Sans", sans-serif; font-size: 12px;';
        let popover;
        if (imagePath) {
            const [displayWidth, displayHeight] = displaySize;
            const sized = displayWidth > 0 && displayHeight > 0;
            const boxWidth = sized ? Math.max(displayWidth, WIDGET_PREVIEW_MIN_BOX) : WIDGET_WIDTH;
            const boxHeight = sized ? Math.max(displayHeight, WIDGET_PREVIEW_MIN_BOX) : WIDGET_PREVIEW_MAX_HEIGHT;
            const backgroundSize = sized ? `${displayWidth}px ${displayHeight}px` : 'contain';
            popover = new St.Widget({
                reactive: false,
                style: `${style} background-image: url("file://${imagePath}"); background-size: ${backgroundSize}; ` +
                    'background-position: center; background-repeat: no-repeat;',
            });
            popover.set_size(boxWidth, boxHeight);
        } else {
            const label = new St.Label({
                text,
                x_expand: true,
                y_expand: true,
                style: `padding: ${WIDGET_PREVIEW_PADDING}px;`,
            });
            label.clutter_text.line_wrap = true;
            label.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
            label.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
            popover = new St.Widget({
                reactive: false,
                layout_manager: new Clutter.BinLayout(),
                style,
                clip_to_allocation: true,
            });
            popover.add_child(label);
            Main.layoutManager.uiGroup.add_child(popover);
            const [, natural] = popover.get_preferred_height(WIDGET_WIDTH);
            if (natural > WIDGET_PREVIEW_MAX_HEIGHT)
                popover.add_child(this._previewFade());
            popover.set_size(WIDGET_WIDTH, Math.min(natural, WIDGET_PREVIEW_MAX_HEIGHT));
        }
        placeWidgetPopover(popover, popover.width, popover.height);
        if (!popover.get_parent())
            Main.layoutManager.uiGroup.add_child(popover);
        widget.preview = popover;
    }

    _previewFade() {
        return new St.Widget({
            height: WIDGET_PREVIEW_FADE_HEIGHT,
            x_expand: true,
            y_expand: true,
            y_align: Clutter.ActorAlign.END,
            style: 'background-gradient-direction: vertical; background-gradient-start: rgba(37,38,41,0); ' +
                'background-gradient-end: #252629; border-radius: 0 0 5px 5px;',
        });
    }

    _closePreview(widget) {
        if (widget.previewId) {
            GLib.Source.remove(widget.previewId);
            widget.previewId = 0;
        }
        if (widget.preview) {
            widget.preview.destroy();
            widget.preview = null;
        }
    }

    _tooltipButton(widget, button, text) {
        button.connect('notify::hover', () => {
            if (button.hover)
                this._scheduleTooltip(widget, text);
            else
                this._closeTooltip(widget);
        });
        button.connect('clicked', () => this._closeTooltip(widget));
        return button;
    }

    _scheduleTooltip(widget, text) {
        this._closeTooltip(widget);
        if (widget.dropdown)
            return;
        const chained = GLib.get_monotonic_time() / 1000 - widget.tooltipHiddenAt <= WIDGET_TOOLTIP_CHAIN_MS;
        if (chained) {
            this._showTooltip(widget, text);
            return;
        }
        widget.tooltipId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, WIDGET_TOOLTIP_DELAY_MS, () => {
            widget.tooltipId = 0;
            this._showTooltip(widget, text);
            return GLib.SOURCE_REMOVE;
        });
    }

    _showTooltip(widget, text) {
        const tooltip = new St.Bin({
            reactive: false,
            style: 'background-color: #252629; border: 1px solid rgba(255,255,255,0.12); border-radius: 6px; ' +
                'color: #e5e5e7; font-family: "Inter", "Noto Sans", sans-serif; font-size: 12px; ' +
                `padding: 0 ${WIDGET_TOOLTIP_PADDING_X}px;`,
            child: new St.Label({text, y_align: Clutter.ActorAlign.CENTER}),
        });
        tooltip.set_height(WIDGET_TOOLTIP_HEIGHT);
        Main.layoutManager.uiGroup.add_child(tooltip);
        const [, width] = tooltip.get_preferred_width(-1);
        placeWidgetPopover(tooltip, width, WIDGET_TOOLTIP_HEIGHT);
        widget.tooltip = tooltip;
    }

    _closeTooltip(widget) {
        if (widget.tooltipId) {
            GLib.Source.remove(widget.tooltipId);
            widget.tooltipId = 0;
        }
        if (widget.tooltip) {
            widget.tooltip.destroy();
            widget.tooltip = null;
            widget.tooltipHiddenAt = GLib.get_monotonic_time() / 1000;
        }
    }

    _dropdownAbove(widget) {
        const {actor} = widget;
        const centreY = actor.y + actor.height / 2;
        const area = workAreaAt(actor.x + actor.width / 2, centreY);
        return centreY > area.y + area.height / 2;
    }

    _toggleDropdown(widget) {
        if (widget.dropdown)
            this._closeDropdown(widget);
        else
            this._openDropdown(widget);
    }

    _openDropdown(widget) {
        this._closeDropdown(widget);
        this._closePreview(widget);
        this._closeTooltip(widget);
        const {targets, selectedId} = widget.model;
        if (targets.length === 0)
            return;
        const lastWebview = targets.findLastIndex(([id]) => id.startsWith('webview:'));
        const separatorIndex = lastWebview >= 0 && lastWebview < targets.length - 1 ? lastWebview : -1;
        const height = targets.length * WIDGET_DROPDOWN_ROW_HEIGHT + 2 * WIDGET_DROPDOWN_PADDING +
            (separatorIndex >= 0 ? WIDGET_DROPDOWN_SEPARATOR_HEIGHT : 0);
        const dropdown = new St.BoxLayout({
            vertical: true,
            reactive: true,
            can_focus: false,
            style: `padding: ${WIDGET_DROPDOWN_PADDING}px 1px; border-radius: ${WIDGET_RADIUS}px; ` +
                'background-color: rgba(28,28,30,0.92); color: #e5e5e7; ' +
                'font-family: "Inter", "Noto Sans", sans-serif; font-size: 13px; ' +
                'box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12);',
        });
        dropdown.set_size(WIDGET_WIDTH, height);
        const rowRadius = WIDGET_RADIUS - WIDGET_DROPDOWN_PADDING;
        targets.forEach(([id, label], index) => {
            const weight = id === selectedId ? 'font-weight: 700;' : '';
            const top = index === 0 ? rowRadius : 0;
            const bottom = index === targets.length - 1 ? rowRadius : 0;
            dropdown.add_child(this._widgetButton(label, this._dropdownRowContent(id, label),
                () => {
                    this._emitRecordingWidgetTargetSelected(id);
                    this._closeDropdown(widget);
                },
                `height: ${WIDGET_DROPDOWN_ROW_HEIGHT}px; padding: 0 ${WIDGET_PADDING_X}px; ` +
                `border-radius: ${top}px ${top}px ${bottom}px ${bottom}px; ${weight}`));
            if (index === separatorIndex) {
                dropdown.add_child(new St.Widget({
                    reactive: false,
                    style: `height: 1px; margin: ${(WIDGET_DROPDOWN_SEPARATOR_HEIGHT - 1) / 2}px ${WIDGET_PADDING_X}px; ` +
                        'background-color: rgba(255,255,255,0.12);',
                }));
            }
        });
        const {actor} = widget;
        dropdown.set_position(actor.x, this._dropdownAbove(widget) ? actor.y - height : actor.y + actor.height);
        Main.layoutManager.addTopChrome(dropdown);
        widget.dropdown = dropdown;
    }

    _dropdownRowContent(id, label) {
        const text = new St.Label({
            text: label,
            x_expand: true,
            x_align: Clutter.ActorAlign.START,
            y_align: Clutter.ActorAlign.CENTER,
        });
        const icon = WIDGET_TARGET_ICONS[id];
        if (!icon)
            return text;
        const content = new St.BoxLayout({
            x_expand: true,
            x_align: Clutter.ActorAlign.START,
            style: `spacing: ${WIDGET_TARGET_ICON_GAP}px;`,
        });
        content.add_child(this._widgetIcon(icon, WIDGET_TARGET_ICON_SIZE));
        content.add_child(text);
        return content;
    }

    _closeDropdown(widget) {
        if (!widget.dropdown)
            return;
        Main.layoutManager.removeChrome(widget.dropdown);
        widget.dropdown.destroy();
        widget.dropdown = null;
    }

    _easeWidgetBars(widget, level) {
        const clamped = Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0;
        const range = 1 - WIDGET_BAR_MIN_SCALE;
        widget.bars.forEach((bar, i) => {
            const scale = WIDGET_BAR_MIN_SCALE + range * clamped * WIDGET_BAR_FACTORS[i];
            if (Math.abs(scale - widget.scales[i]) < WIDGET_BAR_SCALE_EPSILON)
                return;
            widget.scales[i] = scale;
            bar.ease({
                scale_y: scale,
                duration: WIDGET_BAR_EASE_MS,
                mode: Clutter.AnimationMode.LINEAR,
            });
        });
    }

    _onAcceleratorActivated(id) {
        const action = this._grabbed.get(id);
        if (action !== undefined)
            this._dbus.emit_signal('ShortcutActivated', new GLib.Variant('(s)', [action]));
    }

    _ungrabAll() {
        for (const id of this._grabbed.keys())
            global.display.ungrab_accelerator(id);
        this._grabbed.clear();
    }

    _watchSender(sender) {
        this._unwatchSender();
        this._watchId = Gio.bus_watch_name(
            Gio.BusType.SESSION, sender, Gio.BusNameWatcherFlags.NONE, null,
            () => {
                this._unwatchSender();
                this._ungrabAll();
                this._removeRecordingWidget();
            });
    }

    _unwatchSender() {
        if (this._watchId) {
            Gio.bus_unwatch_name(this._watchId);
            this._watchId = 0;
        }
    }

    _findWindow(title) {
        return global.get_window_actors()
            .map(actor => actor.get_meta_window())
            .find(win => win.get_title() === title);
    }

    _placeOrWait(method, title, invocation, place) {
        try {
            this._pending.get(title)?.finish(false);
            const win = this._findWindow(title);
            if (win) {
                const placed = place(win) !== false;
                invocation.return_value(new GLib.Variant('(b)', [placed]));
                return;
            }
            this._waitForWindow(title, place, invocation);
        } catch (e) {
            logError(e, `Promptheus: ${method} failed`);
            invocation.return_dbus_error('com.promptheus.Shell.Error', String(e));
        }
    }

    _placeNow(win, x, y, activate) {
        win.move_frame(true, x, y);
        if (activate)
            Main.activateWindow(win);
    }

    _placeOnParent(win, title, dx, dy, activate) {
        const parent = win.get_transient_for();
        if (!parent) {
            console.warn(`Promptheus: PlaceWindowOnParent "${title}" has no transient parent`);
            return false;
        }
        const p = parent.get_frame_rect();
        win.move_frame(true, p.x + dx, p.y + dy);
        if (activate)
            Main.activateWindow(win);
        const f = win.get_frame_rect();
        console.log(`Promptheus: place on parent "${title}" parent=${p.x},${p.y} frame=${f.x},${f.y} ${f.width}x${f.height}`);
        return true;
    }

    _anchorWindow(win, title, right, bottom, activate) {
        const previous = this._anchors.get(title);
        if (previous)
            this._releaseAnchor(previous);
        const anchor = {title, win, right, bottom, sizeId: 0, unmanagedId: 0, idleId: 0};
        anchor.sizeId = win.connect('size-changed',
            () => this._applyAnchor(anchor, 're-anchor on size-changed'));
        anchor.unmanagedId = win.connect('unmanaged', () => this._releaseAnchor(anchor));
        this._anchors.set(title, anchor);
        win.make_above();
        console.log(`Promptheus: make_above "${title}"`);
        this._applyAnchor(anchor, 'anchor');
        anchor.idleId = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            anchor.idleId = 0;
            this._applyAnchor(anchor, 're-anchor after placement');
            return GLib.SOURCE_REMOVE;
        });
        if (activate)
            Main.activateWindow(win);
    }

    _applyAnchor(anchor, reason) {
        const {title, win, right, bottom} = anchor;
        const before = win.get_frame_rect();
        const x = right - before.width;
        const y = bottom - before.height;
        const moved = before.x !== x || before.y !== y;
        if (moved)
            win.move_frame(true, x, y);
        const after = win.get_frame_rect();
        console.log(`Promptheus: ${reason} "${title}" right=${right} bottom=${bottom} ` +
            `moved=${moved} frame=${after.x},${after.y} ${after.width}x${after.height}`);
    }

    _releaseAnchor(anchor) {
        if (anchor.sizeId)
            anchor.win.disconnect(anchor.sizeId);
        if (anchor.unmanagedId)
            anchor.win.disconnect(anchor.unmanagedId);
        if (anchor.idleId)
            GLib.Source.remove(anchor.idleId);
        anchor.sizeId = 0;
        anchor.unmanagedId = 0;
        anchor.idleId = 0;
        if (this._anchors?.get(anchor.title) === anchor)
            this._anchors.delete(anchor.title);
        console.log(`Promptheus: anchor released "${anchor.title}"`);
    }

    _waitForActivation(title, invocation) {
        const display = global.display;
        const stops = new Set();
        let createdId = 0;
        let timeoutId = 0;
        let done = false;

        const finish = activated => {
            if (done)
                return;
            done = true;
            for (const stop of [...stops])
                stop();
            if (createdId) {
                display.disconnect(createdId);
                createdId = 0;
            }
            if (timeoutId) {
                GLib.Source.remove(timeoutId);
                timeoutId = 0;
            }
            if (this._activations?.get(title) === pending)
                this._activations.delete(title);
            invocation.return_value(new GLib.Variant('(b)', [activated]));
        };
        const pending = {finish};

        const activateIfMatch = win => {
            if (!isAppWindow(win, title))
                return false;
            Main.activateWindow(win);
            finish(true);
            return true;
        };
        const watch = win => this._watchWindow(win, stops, () => activateIfMatch(win));

        for (const actor of global.get_window_actors()) {
            const win = actor.get_meta_window();
            if (win.get_wm_class() === APP_WM_CLASS)
                watch(win);
        }
        createdId = display.connect('window-created', (_display, win) => {
            if (!activateIfMatch(win))
                watch(win);
        });
        timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, PLACE_WINDOW_TIMEOUT_MS, () => {
            timeoutId = 0;
            finish(false);
            return GLib.SOURCE_REMOVE;
        });
        this._activations.set(title, pending);
    }

    _waitForWindow(title, place, invocation) {
        const display = global.display;
        const titleWatches = [];
        let createdId = 0;
        let timeoutId = 0;
        let found = null;
        let unmanagedId = 0;
        let actor = null;
        let frameId = 0;
        let laterId = 0;
        let done = false;

        const stopWatchingCreation = () => {
            if (createdId) {
                display.disconnect(createdId);
                createdId = 0;
            }
            for (const [win, id] of titleWatches)
                win.disconnect(id);
            titleWatches.length = 0;
        };

        const finish = placed => {
            if (done)
                return;
            done = true;
            stopWatchingCreation();
            if (timeoutId) {
                GLib.Source.remove(timeoutId);
                timeoutId = 0;
            }
            if (laterId)
                global.compositor.get_laters().remove(laterId);
            if (unmanagedId)
                found.disconnect(unmanagedId);
            if (actor) {
                if (frameId)
                    actor.disconnect(frameId);
                actor.opacity = 255;
            }
            if (this._pending?.get(title) === pending)
                this._pending.delete(title);
            invocation.return_value(new GLib.Variant('(b)', [placed]));
        };
        const pending = {finish};

        const onFound = win => {
            stopWatchingCreation();
            actor = win.get_compositor_private();
            if (!actor) {
                finish(place(win) !== false);
                return;
            }
            found = win;
            unmanagedId = win.connect('unmanaged', () => finish(false));
            actor.opacity = 0;
            frameId = actor.connect('first-frame', () => {
                actor.disconnect(frameId);
                frameId = 0;
                // Restacking while Mutter paints a frame aborts the shell (assertion !priv->frame_in_progress).
                laterId = global.compositor.get_laters().add(Meta.LaterType.BEFORE_REDRAW, () => {
                    laterId = 0;
                    finish(place(win) !== false);
                    return GLib.SOURCE_REMOVE;
                });
            });
        };

        createdId = display.connect('window-created', (_display, win) => {
            if (win.get_title() === title) {
                onFound(win);
                return;
            }
            const id = win.connect('notify::title', () => {
                if (win.get_title() === title)
                    onFound(win);
            });
            titleWatches.push([win, id]);
        });
        timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, PLACE_WINDOW_TIMEOUT_MS, () => {
            timeoutId = 0;
            finish(false);
            return GLib.SOURCE_REMOVE;
        });
        this._pending.set(title, pending);
    }
}
