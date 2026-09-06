//! macOS desktop shell. Same product as `ui.rs`, native AppKit.
//! See `docs/spec/desktop-shell.md` and `docs/CONTRIBUTING-macos.md`.

#![allow(non_snake_case)]

use crate::discord;
use crate::paths::{self, DiskStatus};
use crate::session::{Engine, Event};

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSBezelStyle, NSButton, NSColor, NSFont, NSLevelIndicator, NSLevelIndicatorStyle, NSMenu,
    NSMenuItem, NSModalResponseOK, NSOpenPanel, NSStatusBar, NSStatusItem, NSTextAlignment,
    NSTextField, NSVariableStatusItemLength, NSWindow, NSWindowDelegate, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    ns_string, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
    NSTimer, NSURL,
};

const WIN_W: f64 = 400.0;
const WIN_H: f64 = 520.0;

#[derive(Clone, Debug)]
enum Status {
    Notice,
    Ready,
    Starting,
    DiscordMissing,
    DiskRefuse,
    Recording,
    Saved(String),
    Error(String),
}

struct Widgets {
    #[allow(dead_code)] // retained so AppKit does not release the window
    window: Retained<NSWindow>,
    title: Retained<NSTextField>,
    subtitle: Retained<NSTextField>,
    discord_label: Retained<NSTextField>,
    discord_meter: Retained<NSLevelIndicator>,
    mic_label: Retained<NSTextField>,
    mic_meter: Retained<NSLevelIndicator>,
    record: Retained<NSButton>,
    rec_caption: Retained<NSTextField>,
    saving: Retained<NSTextField>,
    path: Retained<NSTextField>,
    change: Retained<NSButton>,
    open: Retained<NSButton>,
    status: Retained<NSTextField>,
    extra: Retained<NSTextField>,
    continue_btn: Retained<NSButton>,
    settings: Retained<NSButton>,
}

struct App {
    widgets: Widgets,
    delegate: Retained<Delegate>,
    engine: Option<Engine>,
    pid: Option<u32>,
    status: Status,
    recording: bool,
    last_path: Option<PathBuf>,
    tray: Option<Retained<NSStatusItem>>,
}

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<App>>>> = const { RefCell::new(None) };
}

pub fn run() -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("DiscRec must start on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
    Ok(())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DiscRecDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, notification: &NSNotification) {
            let mtm = self.mtm();
            let ns_app = notification
                .object()
                .unwrap()
                .downcast::<NSApplication>()
                .unwrap();
            build_menu(&ns_app, mtm);
            let widgets = build_window(mtm, self);
            let notice = !paths::notice_was_shown();
            let app = Rc::new(RefCell::new(App {
                widgets,
                delegate: self.retain(),
                engine: None,
                pid: None,
                status: if notice {
                    Status::Notice
                } else {
                    Status::DiscordMissing
                },
                recording: false,
                last_path: None,
                tray: None,
            }));
            APP.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&app)));
            apply_layout(&app.borrow());
            unsafe {
                let _ = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    0.066,
                    self.as_ref(),
                    sel!(tick:),
                    None,
                    true,
                );
            }
            ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_quit(&self, _app: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            with_app(|app| {
                if app.recording {
                    if let Some(eng) = app.engine.as_ref() {
                        eng.end_recording();
                    }
                }
                app.engine = None;
                app.tray = None;
            });
            let ns_app = NSApplication::sharedApplication(self.mtm());
            ns_app.terminate(None);
        }
    }

    impl Delegate {
        #[unsafe(method(record:))]
        fn record(&self, _sender: Option<&AnyObject>) {
            with_app(on_primary);
        }

        #[unsafe(method(changeFolder:))]
        fn change_folder(&self, _sender: Option<&AnyObject>) {
            with_app(change_folder);
        }

        #[unsafe(method(openFolder:))]
        fn open_folder(&self, _sender: Option<&AnyObject>) {
            with_app(|_| open_folder());
        }

        #[unsafe(method(continueNotice:))]
        fn continue_notice(&self, _sender: Option<&AnyObject>) {
            with_app(on_primary);
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            open_privacy_settings();
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _timer: Option<&AnyObject>) {
            with_app(tick);
        }

        #[unsafe(method(trayStop:))]
        fn tray_stop(&self, _sender: Option<&AnyObject>) {
            with_app(on_primary);
        }

        #[unsafe(method(trayFolder:))]
        fn tray_folder(&self, _sender: Option<&AnyObject>) {
            with_app(|_| open_folder());
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|slot| {
        if let Some(app) = slot.borrow().as_ref() {
            f(&mut app.borrow_mut());
        }
    });
}

fn build_menu(app: &NSApplication, mtm: MainThreadMarker) {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
    let app_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!(""),
            None,
            ns_string!(""),
        )
    };
    let app_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("DiscRec"));
    unsafe {
        app_menu.addItemWithTitle_action_keyEquivalent(
            ns_string!("Quit DiscRec"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        );
    }
    app_item.setSubmenu(Some(&app_menu));
    menu.addItem(&app_item);
    app.setMainMenu(Some(&menu));
}

fn label(text: &str, size: f64, muted: bool, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = unsafe { NSTextField::labelWithString(&NSString::from_str(text), mtm) };
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    let color = if muted {
        NSColor::secondaryLabelColor()
    } else {
        NSColor::labelColor()
    };
    field.setTextColor(Some(&color));
    field.setDrawsBackground(false);
    field.setBezeled(false);
    field.setEditable(false);
    field.setAlignment(NSTextAlignment::Left);
    field
}

fn button(
    title: &str,
    action: objc2::runtime::Sel,
    target: &Delegate,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target.as_ref()),
            Some(action),
            mtm,
        )
    };
    #[allow(deprecated)]
    btn.setBezelStyle(NSBezelStyle::Rounded);
    btn
}

fn meter(mtm: MainThreadMarker) -> Retained<NSLevelIndicator> {
    let meter =
        unsafe { NSLevelIndicator::initWithFrame(NSLevelIndicator::alloc(mtm), NSRect::ZERO) };
    meter.setMinValue(0.0);
    meter.setMaxValue(1.0);
    meter.setLevelIndicatorStyle(NSLevelIndicatorStyle::ContinuousCapacity);
    meter.setWarningValue(0.85);
    meter.setCriticalValue(0.95);
    meter
}

fn build_window(mtm: MainThreadMarker, delegate: &Delegate) -> Widgets {
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIN_W, WIN_H)),
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(ns_string!("DiscRec"));
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
        18.0 / 255.0,
        18.0 / 255.0,
        20.0 / 255.0,
        1.0,
    )));
    unsafe { window.setDelegate(Some(ProtocolObject::from_ref(delegate))) };

    let title = label("DiscRec", 22.0, false, mtm);
    let subtitle = label("Discord + your microphone, one file.", 13.0, true, mtm);
    let discord_label = label("Discord", 16.0, false, mtm);
    let discord_meter = meter(mtm);
    let mic_label = label("Microphone", 16.0, false, mtm);
    let mic_meter = meter(mtm);
    let record = button("Record", sel!(record:), delegate, mtm);
    let rec_caption = label("Record", 13.0, true, mtm);
    rec_caption.setAlignment(NSTextAlignment::Center);
    let saving = label("Saving to", 13.0, true, mtm);
    let path = label("", 13.0, false, mtm);
    let change = button("Change folder", sel!(changeFolder:), delegate, mtm);
    let open = button("Open folder", sel!(openFolder:), delegate, mtm);
    let status = label("", 13.0, true, mtm);
    let extra = label("", 13.0, true, mtm);
    let continue_btn = button("Continue", sel!(continueNotice:), delegate, mtm);
    let settings = button("Open Settings", sel!(openSettings:), delegate, mtm);
    settings.setHidden(true);
    status.setMaximumNumberOfLines(3);

    let view = window.contentView().expect("content view");
    view.addSubview(&title);
    view.addSubview(&subtitle);
    view.addSubview(&discord_label);
    view.addSubview(&discord_meter);
    view.addSubview(&mic_label);
    view.addSubview(&mic_meter);
    view.addSubview(&record);
    view.addSubview(&rec_caption);
    view.addSubview(&saving);
    view.addSubview(&path);
    view.addSubview(&change);
    view.addSubview(&open);
    view.addSubview(&status);
    view.addSubview(&extra);
    view.addSubview(&continue_btn);
    view.addSubview(&settings);

    title.setFrame(NSRect::new(NSPoint::new(28.0, 470.0), NSSize::new(344.0, 28.0)));
    subtitle.setFrame(NSRect::new(NSPoint::new(28.0, 440.0), NSSize::new(344.0, 28.0)));
    discord_label.setFrame(NSRect::new(NSPoint::new(28.0, 400.0), NSSize::new(344.0, 20.0)));
    discord_meter.setFrame(NSRect::new(NSPoint::new(28.0, 378.0), NSSize::new(344.0, 16.0)));
    mic_label.setFrame(NSRect::new(NSPoint::new(28.0, 348.0), NSSize::new(344.0, 20.0)));
    mic_meter.setFrame(NSRect::new(NSPoint::new(28.0, 326.0), NSSize::new(344.0, 16.0)));
    record.setFrame(NSRect::new(NSPoint::new(150.0, 250.0), NSSize::new(100.0, 44.0)));
    rec_caption.setFrame(NSRect::new(NSPoint::new(28.0, 220.0), NSSize::new(344.0, 20.0)));
    saving.setFrame(NSRect::new(NSPoint::new(28.0, 170.0), NSSize::new(344.0, 18.0)));
    path.setFrame(NSRect::new(NSPoint::new(28.0, 148.0), NSSize::new(344.0, 20.0)));
    change.setFrame(NSRect::new(NSPoint::new(28.0, 110.0), NSSize::new(140.0, 28.0)));
    open.setFrame(NSRect::new(NSPoint::new(232.0, 110.0), NSSize::new(140.0, 28.0)));
    status.setFrame(NSRect::new(NSPoint::new(28.0, 64.0), NSSize::new(344.0, 40.0)));
    extra.setFrame(NSRect::new(NSPoint::new(28.0, 44.0), NSSize::new(344.0, 20.0)));
    continue_btn.setFrame(NSRect::new(NSPoint::new(130.0, 200.0), NSSize::new(140.0, 36.0)));
    settings.setFrame(NSRect::new(NSPoint::new(130.0, 16.0), NSSize::new(140.0, 28.0)));

    window.center();
    window.makeKeyAndOrderFront(None);

    Widgets {
        window,
        title,
        subtitle,
        discord_label,
        discord_meter,
        mic_label,
        mic_meter,
        record,
        rec_caption,
        saving,
        path,
        change,
        open,
        status,
        extra,
        continue_btn,
        settings,
    }
}

fn apply_layout(app: &App) {
    let notice = matches!(app.status, Status::Notice);
    let w = &app.widgets;
    w.discord_label.setHidden(notice);
    w.discord_meter.setHidden(notice);
    w.mic_label.setHidden(notice);
    w.mic_meter.setHidden(notice);
    w.record.setHidden(notice);
    w.rec_caption.setHidden(notice);
    w.saving.setHidden(notice);
    w.path.setHidden(notice);
    w.change.setHidden(notice);
    w.open.setHidden(notice);
    w.continue_btn.setHidden(!notice);
    if notice {
        w.subtitle.setStringValue(ns_string!(
            "Everyone in the call is being recorded — tell them before you start."
        ));
        w.status.setStringValue(ns_string!(
            "Records Discord and your microphone into one file."
        ));
        w.extra.setStringValue(ns_string!(""));
        w.settings.setHidden(true);
    } else {
        w.subtitle
            .setStringValue(ns_string!("Discord + your microphone, one file."));
        w.path.setStringValue(&NSString::from_str(
            &paths::recordings_dir().display().to_string(),
        ));
        paint_status(app);
    }
}

fn paint_status(app: &App) {
    let (line, extra, settings) = match &app.status {
        Status::Notice => ("", None, false),
        Status::Ready => (
            "Ready. Tell people in the call before you press record.",
            disk_warn(),
            false,
        ),
        Status::Starting => ("Starting capture…", None, false),
        Status::DiscordMissing => ("Start Discord first.", None, false),
        Status::DiskRefuse => ("Not enough disk space (need 500 MB free).", None, false),
        Status::Recording => (
            "Keep talking — quiet is fine, the file keeps going.",
            None,
            false,
        ),
        Status::Saved(name) => ("Saved.", Some(name.as_str()), false),
        Status::Error(m) => {
            let perm = m.to_ascii_lowercase().contains("permission");
            (m.as_str(), None, perm)
        }
    };
    app.widgets.status.setStringValue(&NSString::from_str(line));
    app.widgets
        .extra
        .setStringValue(&NSString::from_str(extra.unwrap_or("")));
    app.widgets.settings.setHidden(!settings);
    if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            let caption = format!("Recording  {}", fmt_elapsed(eng.meters.elapsed_ms()));
            app.widgets
                .rec_caption
                .setStringValue(&NSString::from_str(&caption));
            app.widgets.record.setTitle(ns_string!("Stop"));
        }
    } else {
        app.widgets.rec_caption.setStringValue(ns_string!("Record"));
        app.widgets.record.setTitle(ns_string!("Record"));
    }
}

fn disk_warn() -> Option<&'static str> {
    match paths::disk_status(&paths::recordings_dir()) {
        DiskStatus::Warn => Some("Low disk — under 2 GB free."),
        _ => None,
    }
}

fn fmt_elapsed(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn on_primary(app: &mut App) {
    if matches!(app.status, Status::Notice) {
        paths::mark_notice_shown();
        app.status = Status::DiscordMissing;
        apply_layout(app);
        return;
    }
    toggle_record(app);
}

fn toggle_record(app: &mut App) {
    if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            eng.end_recording();
        }
        return;
    }
    match paths::disk_status(&paths::recordings_dir()) {
        DiskStatus::Refuse => {
            app.status = Status::DiskRefuse;
            paint_status(app);
            return;
        }
        DiskStatus::Warn | DiskStatus::Ok => {}
    }
    let Some(pid) = app.pid.or_else(|| discord::find().map(|f| f.pid)) else {
        app.status = Status::DiscordMissing;
        paint_status(app);
        return;
    };
    ensure_engine(app, pid);
    let path = match paths::new_recording_path() {
        Ok(p) => p,
        Err(e) => {
            app.status = Status::Error(format!("Cannot create folder: {e}"));
            paint_status(app);
            return;
        }
    };
    if let Some(eng) = app.engine.as_ref() {
        eng.begin_recording(path);
    }
}

fn ensure_engine(app: &mut App, pid: u32) {
    if app.pid == Some(pid) && app.engine.is_some() {
        return;
    }
    app.engine = None;
    app.pid = Some(pid);
    app.engine = Some(Engine::start(pid));
    if !matches!(app.status, Status::Recording | Status::Notice) {
        app.status = Status::Starting;
    }
}

fn tick(app: &mut App) {
    if matches!(app.status, Status::Notice) {
        return;
    }
    match discord::find().map(|f| f.pid) {
        Some(pid) => {
            if !app.recording {
                ensure_engine(app, pid);
            }
        }
        None => {
            if !app.recording {
                app.engine = None;
                app.pid = None;
                app.status = Status::DiscordMissing;
                app.widgets.discord_meter.setDoubleValue(0.0);
                app.widgets.mic_meter.setDoubleValue(0.0);
            }
        }
    }

    let mut events = Vec::new();
    if let Some(eng) = app.engine.as_ref() {
        while let Some(ev) = eng.poll_event() {
            events.push(ev);
        }
        app.widgets
            .discord_meter
            .setDoubleValue(eng.meters.discord() as f64);
        app.widgets.mic_meter.setDoubleValue(eng.meters.mic() as f64);
    }
    for ev in events {
        handle_event(app, ev);
    }
    if app.recording {
        update_tray(app);
    }
    app.widgets.path.setStringValue(&NSString::from_str(
        &paths::recordings_dir().display().to_string(),
    ));
    paint_status(app);
}

fn handle_event(app: &mut App, ev: Event) {
    match ev {
        Event::Previewing => {
            if !app.recording {
                app.status = Status::Ready;
            }
        }
        Event::Recording => {
            app.recording = true;
            app.status = Status::Recording;
            add_tray(app);
        }
        Event::Saved { path } => {
            app.recording = false;
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "recording.ogg".into());
            app.last_path = Some(path);
            app.status = Status::Saved(name);
            app.tray = None;
        }
        Event::Failed(msg) => {
            app.recording = false;
            app.status = Status::Error(msg);
            app.tray = None;
        }
    }
}

fn add_tray(app: &mut App) {
    if app.tray.is_some() {
        return;
    }
    let mtm = MainThreadMarker::new().unwrap();
    let item = unsafe { NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength) };
    if let Some(button) = item.button(mtm) {
        button.setTitle(ns_string!("● DiscRec"));
    }
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
    let stop = unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Stop"),
            Some(sel!(trayStop:)),
            ns_string!(""),
        );
        item.setTarget(Some(app.delegate.as_ref()));
        item
    };
    let folder = unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Show in folder"),
            Some(sel!(trayFolder:)),
            ns_string!(""),
        );
        item.setTarget(Some(app.delegate.as_ref()));
        item
    };
    menu.addItem(&stop);
    menu.addItem(&folder);
    item.setMenu(Some(&menu));
    app.tray = Some(item);
}

fn update_tray(app: &App) {
    let Some(item) = app.tray.as_ref() else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(button) = item.button(mtm) {
        if let Some(eng) = app.engine.as_ref() {
            let title = format!("● {}", fmt_elapsed(eng.meters.elapsed_ms()));
            button.setTitle(&NSString::from_str(&title));
        }
    }
}

fn change_folder(app: &mut App) {
    let mtm = MainThreadMarker::new().unwrap();
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(true);
    panel.setTitle(Some(ns_string!("Saving to")));
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(url) = panel.URL() else {
        return;
    };
    let Some(path) = url.path() else {
        return;
    };
    let dir = PathBuf::from(path.to_string());
    match paths::set_recordings_dir(dir) {
        Ok(dir) => {
            if let Some(p) = &app.last_path {
                if !p.starts_with(&dir) {
                    app.last_path = None;
                }
            }
            if !app.recording {
                app.status = Status::Ready;
            }
        }
        Err(e) => app.status = Status::Error(format!("Cannot use that folder: {e}")),
    }
}

fn open_folder() {
    let dir = paths::ensure_recordings_dir().unwrap_or_else(|_| paths::recordings_dir());
    let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
    unsafe {
        let _ = NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

fn open_privacy_settings() {
    let url = NSURL::URLWithString(ns_string!(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
    ));
    if let Some(url) = url {
        unsafe {
            let _ = NSWorkspace::sharedWorkspace().openURL(&url);
        }
    }
}
