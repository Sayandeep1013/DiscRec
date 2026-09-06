//! Windows desktop shell. See `docs/spec/desktop-shell.md` and ADR-0010.
//!
//! Drawn with GDI against the `windows` crate we already compile. No GPU, no
//! webview. macOS will need its own file behind the same spec.

#![allow(unused_must_use)]

use crate::discord;
use crate::paths::{self, DiskStatus};
use crate::session::{Engine, Event};

use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;

use windows::core::{w, Result as WinResult, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DRAW_TEXT_FORMAT, DT_LEFT, DT_NOPREFIX,
    FW_NORMAL, HBRUSH, HDC, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadCursorW,
    LoadIconW, MessageBoxW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    SetMenuDefaultItem, SetTimer, SetWindowLongPtrW, SetWindowTextW, ShowWindow, TrackPopupMenu,
    TranslateMessage, BS_PUSHBUTTON, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HMENU,
    IDC_ARROW, IDI_APPLICATION, MB_ICONERROR, MB_OK, MENU_ITEM_FLAGS, MESSAGEBOX_STYLE, MSG,
    SW_HIDE, SW_SHOW, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_COMMAND,
    WM_CONTEXTMENU, WM_CREATE, WM_DESTROY, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WM_TIMER,
    WNDCLASSW, WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE,
};

const ID_RECORD: i32 = 1001;
const ID_FOLDER: i32 = 1002;
const ID_CONTINUE: i32 = 1003;
const ID_TRAY_STOP: i32 = 1101;
const ID_TRAY_FOLDER: i32 = 1102;
const TIMER_ID: usize = 1;
const WM_TRAY: u32 = WM_APP + 2;

const WIN_W: i32 = 380;
const WIN_H: i32 = 460;

#[derive(Clone, Debug)]
enum Status {
    Notice,
    Ready,
    Starting,
    DiscordMissing,
    DiskRefuse,
    Recording,
    NoSignal,
    Saved(String),
    Error(String),
}

struct App {
    hwnd: HWND,
    record_btn: HWND,
    folder_btn: HWND,
    continue_btn: HWND,
    font: HFONT,
    bg: HBRUSH,
    meter_bg: HBRUSH,
    meter_fg: HBRUSH,
    engine: Option<Engine>,
    pid: Option<u32>,
    status: Status,
    recording: bool,
    last_path: Option<PathBuf>,
    discord_disp: f32,
    mic_disp: f32,
    tray: bool,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
}

pub fn attach_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
    }
}

pub fn run() -> WinResult<()> {
    unsafe { run_inner() }
}

unsafe fn run_inner() -> WinResult<()> {
    let instance = GetModuleHandleW(None)?;
    let class = w!("DiscRecWindow");

    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: class,
        hbrBackground: CreateSolidBrush(rgb(22, 22, 24)),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        w!("DiscRec"),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        WIN_W,
        WIN_H,
        None,
        None,
        Some(instance.into()),
        None,
    )?;

    let _ = ShowWindow(hwnd, SW_SHOW);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        let _ = TranslateMessage(&msg);
        let _ = DispatchMessageW(&msg);
    }
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CREATE {
        match create_app(hwnd) {
            Ok(app) => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(app) as isize);
                let _ = SetTimer(Some(hwnd), TIMER_ID, 66, None);
            }
            Err(e) => {
                let text = wide(&format!("Could not start DiscRec: {e}"));
                let _ = MessageBoxW(
                    Some(hwnd),
                    PCWSTR(text.as_ptr()),
                    w!("DiscRec"),
                    MESSAGEBOX_STYLE(MB_OK.0 | MB_ICONERROR.0),
                );
                PostQuitMessage(1);
            }
        }
        return LRESULT(0);
    }

    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let app = &mut *ptr;

    match msg {
        WM_TIMER => {
            tick(app);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(app);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xffff;
            on_command(app, id as i32);
            LRESULT(0)
        }
        WM_TRAY => {
            let mouse = lparam.0 as u32;
            if mouse == WM_RBUTTONUP || mouse == WM_CONTEXTMENU {
                tray_menu(app);
            } else if mouse == WM_LBUTTONUP {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if app.recording {
                if let Some(eng) = app.engine.as_ref() {
                    eng.end_recording();
                }
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            teardown(app);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_app(hwnd: HWND) -> WinResult<Box<App>> {
    let font = CreateFontW(
        18,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    );

    let record_btn = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("BUTTON"),
        w!("Record"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        90,
        250,
        180,
        40,
        Some(hwnd),
        Some(HMENU(ID_RECORD as *mut c_void)),
        None,
        None,
    )?;

    let folder_btn = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("BUTTON"),
        w!("Recordings"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        90,
        300,
        180,
        32,
        Some(hwnd),
        Some(HMENU(ID_FOLDER as *mut c_void)),
        None,
        None,
    )?;

    let continue_btn = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("BUTTON"),
        w!("Continue"),
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        90,
        300,
        180,
        40,
        Some(hwnd),
        Some(HMENU(ID_CONTINUE as *mut c_void)),
        None,
        None,
    )?;

    let notice = !paths::notice_was_shown();
    if notice {
        let _ = ShowWindow(record_btn, SW_HIDE);
        let _ = ShowWindow(folder_btn, SW_HIDE);
        let _ = ShowWindow(continue_btn, SW_SHOW);
    }

    let icon = LoadIconW(None, IDI_APPLICATION)?;

    Ok(Box::new(App {
        hwnd,
        record_btn,
        folder_btn,
        continue_btn,
        font,
        bg: CreateSolidBrush(rgb(22, 22, 24)),
        meter_bg: CreateSolidBrush(rgb(40, 40, 44)),
        meter_fg: CreateSolidBrush(rgb(80, 200, 140)),
        engine: None,
        pid: None,
        status: if notice {
            Status::Notice
        } else {
            Status::DiscordMissing
        },
        recording: false,
        last_path: None,
        discord_disp: 0.0,
        mic_disp: 0.0,
        tray: false,
        icon,
    }))
}

fn on_command(app: &mut App, id: i32) {
    match id {
        ID_CONTINUE => {
            paths::mark_notice_shown();
            unsafe {
                let _ = ShowWindow(app.continue_btn, SW_HIDE);
                let _ = ShowWindow(app.record_btn, SW_SHOW);
                let _ = ShowWindow(app.folder_btn, SW_SHOW);
            }
            app.status = Status::DiscordMissing;
        }
        ID_RECORD | ID_TRAY_STOP => toggle_record(app),
        ID_FOLDER | ID_TRAY_FOLDER => open_folder(app),
        _ => {}
    }
}

fn toggle_record(app: &mut App) {
    if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            eng.end_recording();
        }
        return;
    }
    if matches!(app.status, Status::Notice) {
        return;
    }
    match paths::disk_status(&paths::recordings_dir()) {
        DiskStatus::Refuse => {
            app.status = Status::DiskRefuse;
            return;
        }
        DiskStatus::Warn | DiskStatus::Ok => {}
    }
    let Some(pid) = app.pid.or_else(|| discord::find().map(|f| f.pid)) else {
        app.status = Status::DiscordMissing;
        return;
    };
    ensure_engine(app, pid);
    let path = match paths::new_recording_path() {
        Ok(p) => p,
        Err(e) => {
            app.status = Status::Error(format!("Cannot create folder: {e}"));
            return;
        }
    };
    if let Some(eng) = app.engine.as_ref() {
        eng.begin_recording(path);
        set_record_label(app, true);
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

fn open_folder(app: &App) {
    let target = app.last_path.clone().unwrap_or_else(paths::recordings_dir);
    let _ = paths::ensure_recordings_dir();
    let mut cmd = std::process::Command::new("explorer");
    if target.is_file() {
        cmd.arg(format!("/select,{}", target.display()));
    } else {
        cmd.arg(&target);
    }
    let _ = cmd.spawn();
}

fn tick(app: &mut App) {
    if matches!(app.status, Status::Notice) {
        return;
    }

    let found = discord::find().map(|f| f.pid);
    match found {
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
                app.discord_disp = 0.0;
                app.mic_disp = 0.0;
            }
        }
    }

    let mut events = Vec::new();
    if let Some(eng) = app.engine.as_ref() {
        while let Some(ev) = eng.poll_event() {
            events.push(ev);
        }
        app.discord_disp = (app.discord_disp * 0.82).max(eng.meters.discord());
        app.mic_disp = (app.mic_disp * 0.82).max(eng.meters.mic());
    }
    for ev in events {
        handle_event(app, ev);
    }
    if !matches!(app.status, Status::Notice) {
        set_record_label(app, app.recording);
    }

    if app.recording {
        update_tray(app);
    }
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
            set_record_label(app, true);
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
            set_record_label(app, false);
            remove_tray(app);
        }
        Event::NoSignal => {
            app.recording = false;
            app.status = Status::NoSignal;
            set_record_label(app, false);
            remove_tray(app);
        }
        Event::Failed(msg) => {
            app.recording = false;
            app.status = Status::Error(msg);
            set_record_label(app, false);
            remove_tray(app);
        }
    }
}

fn set_record_label(app: &App, recording: bool) {
    let label = if recording { w!("Stop") } else { w!("Record") };
    unsafe {
        let _ = SetWindowTextW(app.record_btn, label);
        let enable = !matches!(
            app.status,
            Status::Notice | Status::DiscordMissing | Status::DiskRefuse
        ) || recording;
        let _ = EnableWindow(app.record_btn, enable);
    }
}

#[allow(unused_must_use)]
unsafe fn paint(app: &App) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(app.hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(app.hwnd, &mut rc);
    FillRect(hdc, &rc, app.bg);

    SelectObject(hdc, app.font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, rgb(240, 240, 242));

    if matches!(app.status, Status::Notice) {
        draw_notice(hdc, &rc);
        EndPaint(app.hwnd, &ps);
        return;
    }

    text_out(hdc, 24, 18, "DiscRec");
    SetTextColor(hdc, rgb(160, 160, 168));
    text_out(hdc, 24, 42, "Records Discord and your microphone.");

    SetTextColor(hdc, rgb(240, 240, 242));
    text_out(hdc, 24, 88, "Discord");
    draw_meter(hdc, app, 24, 110, rc.right - 48, app.discord_disp);
    text_out(hdc, 24, 148, "Mic");
    draw_meter(hdc, app, 24, 170, rc.right - 48, app.mic_disp);

    if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            SetTextColor(hdc, rgb(220, 80, 80));
            text_out(
                hdc,
                24,
                208,
                &format!("Recording  {}", fmt_elapsed(eng.meters.elapsed_ms())),
            );
        }
    }

    SetTextColor(hdc, rgb(200, 200, 204));
    let (line, extra) = status_text(app);
    text_out(hdc, 24, 360, line);
    if let Some(e) = extra {
        text_out(hdc, 24, 382, e);
    }

    EndPaint(app.hwnd, &ps);
}

#[allow(unused_must_use)]
unsafe fn draw_notice(hdc: HDC, rc: &RECT) {
    SetTextColor(hdc, rgb(240, 240, 242));
    text_out(hdc, 24, 24, "DiscRec");
    let mut box_rc = RECT {
        left: 24,
        top: 70,
        right: rc.right - 24,
        bottom: 260,
    };
    DrawTextW(
        hdc,
        &mut wide(
            "DiscRec records Discord's audio and your microphone into one file.\n\n\
             Everyone in the call is being recorded — tell them before you start.",
        ),
        &mut box_rc,
        DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_NOPREFIX.0),
    );
}

#[allow(unused_must_use)]
unsafe fn draw_meter(hdc: HDC, app: &App, x: i32, y: i32, w: i32, level: f32) {
    let h = 18;
    let track = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    FillRect(hdc, &track, app.meter_bg);
    let fill_w = ((level.clamp(0.0, 1.0) * w as f32) as i32).max(if level > 0.0 { 2 } else { 0 });
    if fill_w > 0 {
        let fill = RECT {
            left: x,
            top: y,
            right: x + fill_w,
            bottom: y + h,
        };
        FillRect(hdc, &fill, app.meter_fg);
    }
}

fn status_text(app: &App) -> (&str, Option<&str>) {
    match &app.status {
        Status::Notice => ("", None),
        Status::Ready => (
            "Ready. Everyone in the call will be recorded.",
            disk_warn(app),
        ),
        Status::Starting => ("Starting capture…", None),
        Status::DiscordMissing => ("Start Discord first.", None),
        Status::DiskRefuse => ("Not enough disk space (need 500 MB free).", None),
        Status::Recording => ("Everyone in the call is being recorded — tell them.", None),
        Status::NoSignal => (
            "Started, but no audio is coming through.",
            Some("Press Record to retry."),
        ),
        Status::Saved(name) => ("Saved.", Some(name.as_str())),
        Status::Error(m) => (m.as_str(), None),
    }
}

fn disk_warn(_app: &App) -> Option<&'static str> {
    match paths::disk_status(&paths::recordings_dir()) {
        DiskStatus::Warn => Some("Low disk — under 2 GB free."),
        _ => None,
    }
}

fn fmt_elapsed(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn add_tray(app: &mut App) {
    if app.tray {
        return;
    }
    let nid = tray_data(app);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
    app.tray = true;
}

fn update_tray(app: &App) {
    if !app.tray {
        return;
    }
    let nid = tray_data(app);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn remove_tray(app: &mut App) {
    if !app.tray {
        return;
    }
    let nid = tray_data(app);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
    app.tray = false;
}

fn tray_data(app: &App) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: app.hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: app.icon,
        ..Default::default()
    };
    let tip = if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            format!("Recording {}", fmt_elapsed(eng.meters.elapsed_ms()))
        } else {
            "Recording".into()
        }
    } else {
        "DiscRec".into()
    };
    let encoded: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
    let n = encoded.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&encoded[..n]);
    nid
}

unsafe fn tray_menu(app: &App) {
    let menu = CreatePopupMenu().unwrap_or_default();
    if menu.is_invalid() {
        return;
    }
    let _ = AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        ID_TRAY_STOP as usize,
        w!("Stop"),
    );
    let _ = AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        ID_TRAY_FOLDER as usize,
        w!("Show in folder"),
    );
    let _ = SetMenuDefaultItem(menu, ID_TRAY_STOP as u32, 0);
    let mut pt = windows::Win32::Foundation::POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(app.hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, app.hwnd, None);
    let _ = DestroyMenu(menu);
}

unsafe fn teardown(app: &mut App) {
    remove_tray(app);
    app.engine = None;
    let _ = DeleteObject(app.bg.into());
    let _ = DeleteObject(app.meter_bg.into());
    let _ = DeleteObject(app.meter_fg.into());
    let _ = DeleteObject(app.font.into());
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn text_out(hdc: HDC, x: i32, y: i32, s: &str) {
    let w = wide(s);
    let _ = TextOutW(hdc, x, y, &w[..w.len().saturating_sub(1)]);
}
