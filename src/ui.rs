//! Windows desktop shell. See `docs/spec/desktop-shell.md` and ADR-0010.
//!
//! Compact dark recorder: one control, live meters, visible save path.
//! Title bar is tinted to match the client (Win11 DWM caption color).

#![allow(unused_must_use)]

use crate::discord;
use crate::paths::{self, DiskStatus};
use crate::session::{Engine, Event};

use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;

use windows::core::{w, Result as WinResult, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse,
    EndPaint, FillRect, GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode,
    SetTextColor, TextOutW, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DRAW_TEXT_FORMAT, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_PATH_ELLIPSIS, DT_SINGLELINE,
    FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HFONT, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
    PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, Shell_NotifyIconW, ShellExecuteW, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    LoadCursorW, LoadImageW, MessageBoxW, PostQuitMessage, RegisterClassW, SetCursor,
    SetForegroundWindow, SetMenuDefaultItem, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HICON,
    IDC_ARROW, IDC_HAND, IMAGE_ICON, LR_LOADFROMFILE, MB_ICONERROR, MB_OK, MENU_ITEM_FLAGS,
    MESSAGEBOX_STYLE, MSG, SW_SHOW, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOZORDER, TPM_RIGHTBUTTON,
    WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CREATE, WM_DESTROY,
    WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP, WM_SETCURSOR,
    WM_TIMER, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};

const ID_TRAY_STOP: i32 = 1101;
const ID_TRAY_FOLDER: i32 = 1102;
const TIMER_ID: usize = 1;
const WM_TRAY: u32 = WM_APP + 2;
const VK_SPACE: u16 = 0x20;

const WIN_W: i32 = 400;
const WIN_H: i32 = 520;

const COL_BG: (u8, u8, u8) = (18, 18, 20);
const COL_TEXT: (u8, u8, u8) = (245, 245, 247);
const COL_MUTED: (u8, u8, u8) = (138, 138, 144);
const COL_METER_BG: (u8, u8, u8) = (36, 36, 40);
const COL_METER: (u8, u8, u8) = (90, 210, 150);
const COL_REC: (u8, u8, u8) = (226, 59, 59);

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

#[derive(Copy, Clone, PartialEq, Eq)]
enum Hit {
    None,
    Record,
    Change,
    Open,
    Continue,
}

struct App {
    hwnd: HWND,
    dpi: u32,
    font: HFONT,
    font_title: HFONT,
    font_small: HFONT,
    bg: HBRUSH,
    meter_bg: HBRUSH,
    meter_fg: HBRUSH,
    rec_brush: HBRUSH,
    engine: Option<Engine>,
    pid: Option<u32>,
    status: Status,
    recording: bool,
    last_path: Option<PathBuf>,
    discord_disp: f32,
    mic_disp: f32,
    tray: bool,
    icon: HICON,
    hover: Hit,
}

pub fn attach_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
    }
}

/// Tell Windows we draw at the monitor's real DPI. Without this, the OS
/// rasterizes at 96 DPI and stretches the bitmap — the classic blurry Win32 look,
/// including in-process dialogs and Explorer windows we launch.
pub fn enable_dpi_awareness() {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err()
            && SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE).is_err()
        {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        }
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn px(dpi: u32, v: i32) -> i32 {
    let dpi = dpi.max(96) as i64;
    ((v as i64 * dpi + 48) / 96) as i32
}

pub fn run() -> WinResult<()> {
    unsafe { run_inner() }
}

unsafe fn run_inner() -> WinResult<()> {
    enable_dpi_awareness();
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let instance = GetModuleHandleW(None)?;
    let class = w!("DiscRecWindow");
    let dpi = GetDpiForSystem().max(96);
    let icon = load_app_icon(dpi);

    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: icon,
        lpszClassName: class,
        hbrBackground: CreateSolidBrush(rgb(COL_BG.0, COL_BG.1, COL_BG.2)),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE;
    let mut wr = RECT {
        left: 0,
        top: 0,
        right: px(dpi, WIN_W),
        bottom: px(dpi, WIN_H),
    };
    let _ = AdjustWindowRectExForDpi(&mut wr, style, false, WINDOW_EX_STYLE::default(), dpi);

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        w!("DiscRec"),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        wr.right - wr.left,
        wr.bottom - wr.top,
        None,
        None,
        Some(instance.into()),
        None,
    )?;

    apply_dark_titlebar(hwnd);
    ShowWindow(hwnd, SW_SHOW);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        let _ = TranslateMessage(&msg);
        let _ = DispatchMessageW(&msg);
    }
    Ok(())
}

fn apply_dark_titlebar(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const c_void,
            4,
        );
        let caption = rgb(COL_BG.0, COL_BG.1, COL_BG.2);
        let text = rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const COLORREF as *const c_void,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const COLORREF as *const c_void,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &caption as *const COLORREF as *const c_void,
            4,
        );
    }
}

fn load_app_icon(dpi: u32) -> HICON {
    let bytes: &[u8] = include_bytes!("../assets/app.ico");
    let dest = paths::config_dir().join("app.ico");
    let _ = std::fs::create_dir_all(paths::config_dir());
    let _ = std::fs::write(&dest, bytes);
    let wide_path = wide(&dest.to_string_lossy());
    let size = px(dpi, 32);
    unsafe {
        LoadImageW(
            None,
            PCWSTR(wide_path.as_ptr()),
            IMAGE_ICON,
            size,
            size,
            LR_LOADFROMFILE,
        )
        .map(|h: HANDLE| HICON(h.0))
        .unwrap_or_default()
    }
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
        WM_MOUSEMOVE => {
            let (x, y) = lparam_point(lparam);
            let hit = hit_test(app, x, y);
            if hit != app.hover {
                app.hover = hit;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = if matches!(
                app.hover,
                Hit::Change | Hit::Open | Hit::Record | Hit::Continue
            ) {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            if let Ok(c) = LoadCursorW(None, cursor) {
                SetCursor(Some(c));
            }
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_point(lparam);
            match hit_test(app, x, y) {
                Hit::Record | Hit::Continue => on_primary(app),
                Hit::Open => open_folder(app),
                Hit::Change => change_folder(app),
                Hit::None => {}
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == VK_SPACE {
                on_primary(app);
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xffff;
            if id == ID_TRAY_STOP as u32 {
                toggle_record(app);
            } else if id == ID_TRAY_FOLDER as u32 {
                open_folder(app);
            }
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
        WM_DPICHANGED => {
            let dpi = ((wparam.0 as u32) >> 16).max(96);
            replace_fonts(app, dpi);
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn lparam_point(lparam: LPARAM) -> (i32, i32) {
    let v = lparam.0 as u32;
    (
        (v & 0xffff) as i16 as i32,
        ((v >> 16) & 0xffff) as i16 as i32,
    )
}

fn replace_fonts(app: &mut App, dpi: u32) {
    let dpi = dpi.max(96);
    unsafe {
        let _ = DeleteObject(app.font.into());
        let _ = DeleteObject(app.font_title.into());
        let _ = DeleteObject(app.font_small.into());
    }
    app.dpi = dpi;
    app.font = make_font(px(dpi, 16), FW_NORMAL.0 as i32);
    app.font_title = make_font(px(dpi, 22), FW_SEMIBOLD.0 as i32);
    app.font_small = make_font(px(dpi, 13), FW_NORMAL.0 as i32);
}

unsafe fn create_app(hwnd: HWND) -> WinResult<Box<App>> {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let notice = !paths::notice_was_shown();

    Ok(Box::new(App {
        hwnd,
        dpi,
        font: make_font(px(dpi, 16), FW_NORMAL.0 as i32),
        font_title: make_font(px(dpi, 22), FW_SEMIBOLD.0 as i32),
        font_small: make_font(px(dpi, 13), FW_NORMAL.0 as i32),
        bg: CreateSolidBrush(rgb(COL_BG.0, COL_BG.1, COL_BG.2)),
        meter_bg: CreateSolidBrush(rgb(COL_METER_BG.0, COL_METER_BG.1, COL_METER_BG.2)),
        meter_fg: CreateSolidBrush(rgb(COL_METER.0, COL_METER.1, COL_METER.2)),
        rec_brush: CreateSolidBrush(rgb(COL_REC.0, COL_REC.1, COL_REC.2)),
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
        icon: load_app_icon(dpi),
        hover: Hit::None,
    }))
}

fn make_font(size_px: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -size_px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        )
    }
}

struct Layout {
    record: RECT,
    change: RECT,
    open: RECT,
    continue_btn: RECT,
}

fn layout(rc: &RECT, dpi: u32) -> Layout {
    let cx = rc.right / 2;
    let rec_r = px(dpi, 34);
    let rec_y = px(dpi, 248);
    let side = px(dpi, 28);
    Layout {
        record: RECT {
            left: cx - rec_r,
            top: rec_y,
            right: cx + rec_r,
            bottom: rec_y + rec_r * 2,
        },
        change: RECT {
            left: side,
            top: px(dpi, 400),
            right: px(dpi, 140),
            bottom: px(dpi, 424),
        },
        open: RECT {
            left: rc.right - px(dpi, 140),
            top: px(dpi, 400),
            right: rc.right - side,
            bottom: px(dpi, 424),
        },
        continue_btn: RECT {
            left: cx - px(dpi, 70),
            top: px(dpi, 300),
            right: cx + px(dpi, 70),
            bottom: px(dpi, 344),
        },
    }
}

fn hit_test(app: &App, x: i32, y: i32) -> Hit {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(app.hwnd, &mut rc);
    }
    let l = layout(&rc, app.dpi);
    if matches!(app.status, Status::Notice) {
        return if pt_in(&l.continue_btn, x, y) {
            Hit::Continue
        } else {
            Hit::None
        };
    }
    if pt_in(&l.record, x, y) {
        Hit::Record
    } else if pt_in(&l.change, x, y) {
        Hit::Change
    } else if pt_in(&l.open, x, y) {
        Hit::Open
    } else {
        Hit::None
    }
}

fn pt_in(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn on_primary(app: &mut App) {
    if matches!(app.status, Status::Notice) {
        paths::mark_notice_shown();
        app.status = Status::DiscordMissing;
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
    let dir = paths::ensure_recordings_dir().unwrap_or_else(|_| paths::recordings_dir());
    let wide_path = wide(&dir.to_string_lossy());
    unsafe {
        // Ask the existing shell to open the folder. Spawning explorer.exe as
        // our child used to inherit this process's DPI-unaware context, which
        // made File Explorer look as blurry as the app.
        ShellExecuteW(
            Some(app.hwnd),
            w!("open"),
            PCWSTR(wide_path.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

fn change_folder(app: &mut App) {
    if let Some(dir) = pick_folder(app.hwnd) {
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
}

fn pick_folder(owner: HWND) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let opts = windows::Win32::UI::Shell::FILEOPENDIALOGOPTIONS(
            FOS_PICKFOLDERS.0 | FOS_FORCEFILESYSTEM.0,
        );
        dialog.SetOptions(opts).ok()?;
        dialog.Show(Some(owner)).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok()?;
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }
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
            remove_tray(app);
        }
        Event::Failed(msg) => {
            app.recording = false;
            app.status = Status::Error(msg);
            remove_tray(app);
        }
    }
}

unsafe fn paint(app: &App) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(app.hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(app.hwnd, &mut rc);
    FillRect(hdc, &rc, app.bg);
    SetBkMode(hdc, TRANSPARENT);

    if matches!(app.status, Status::Notice) {
        paint_notice(app, hdc, &rc);
        EndPaint(app.hwnd, &ps);
        return;
    }

    let dpi = app.dpi;
    let side = px(dpi, 28);

    SelectObject(hdc, app.font_title.into());
    SetTextColor(hdc, rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2));
    text_out(hdc, side, px(dpi, 20), "DiscRec");

    SelectObject(hdc, app.font_small.into());
    SetTextColor(hdc, rgb(COL_MUTED.0, COL_MUTED.1, COL_MUTED.2));
    text_out(
        hdc,
        side,
        px(dpi, 50),
        "Discord + your microphone, one file.",
    );

    SelectObject(hdc, app.font.into());
    SetTextColor(hdc, rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2));
    text_out(hdc, side, px(dpi, 92), "Discord");
    draw_meter(hdc, app, side, px(dpi, 118), rc.right - side * 2, app.discord_disp);
    text_out(hdc, side, px(dpi, 148), "Microphone");
    draw_meter(hdc, app, side, px(dpi, 174), rc.right - side * 2, app.mic_disp);

    let l = layout(&rc, dpi);
    draw_record_button(hdc, app, &l.record);

    SelectObject(hdc, app.font_small.into());
    if app.recording {
        if let Some(eng) = app.engine.as_ref() {
            SetTextColor(hdc, rgb(COL_REC.0, COL_REC.1, COL_REC.2));
            let label = format!("Recording  {}", fmt_elapsed(eng.meters.elapsed_ms()));
            center_text(hdc, &rc, l.record.bottom + px(dpi, 12), &label, dpi);
        }
    } else {
        SetTextColor(hdc, rgb(COL_MUTED.0, COL_MUTED.1, COL_MUTED.2));
        center_text(hdc, &rc, l.record.bottom + px(dpi, 12), "Record", dpi);
    }

    SetTextColor(hdc, rgb(COL_MUTED.0, COL_MUTED.1, COL_MUTED.2));
    text_out(hdc, side, px(dpi, 348), "Saving to");
    SetTextColor(hdc, rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2));
    let mut path_rc = RECT {
        left: side,
        top: px(dpi, 368),
        right: rc.right - side,
        bottom: px(dpi, 392),
    };
    let path = paths::recordings_dir().display().to_string();
    DrawTextW(
        hdc,
        &mut wide(&path),
        &mut path_rc,
        DRAW_TEXT_FORMAT(
            DT_LEFT.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0 | DT_PATH_ELLIPSIS.0 | DT_END_ELLIPSIS.0,
        ),
    );

    let change_col = if app.hover == Hit::Change {
        COL_TEXT
    } else {
        COL_MUTED
    };
    let open_col = if app.hover == Hit::Open {
        COL_TEXT
    } else {
        COL_MUTED
    };
    SetTextColor(hdc, rgb(change_col.0, change_col.1, change_col.2));
    text_out(hdc, l.change.left, l.change.top + px(dpi, 2), "Change folder");
    SetTextColor(hdc, rgb(open_col.0, open_col.1, open_col.2));
    text_out(hdc, l.open.left, l.open.top + px(dpi, 2), "Open folder");

    SetTextColor(hdc, rgb(COL_MUTED.0, COL_MUTED.1, COL_MUTED.2));
    let (line, extra) = status_text(app);
    text_out(hdc, side, px(dpi, 440), line);
    if let Some(e) = extra {
        text_out(hdc, side, px(dpi, 460), e);
    }

    EndPaint(app.hwnd, &ps);
}

unsafe fn paint_notice(app: &App, hdc: HDC, rc: &RECT) {
    let dpi = app.dpi;
    let side = px(dpi, 28);
    SelectObject(hdc, app.font_title.into());
    SetTextColor(hdc, rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2));
    text_out(hdc, side, px(dpi, 24), "DiscRec");
    SelectObject(hdc, app.font.into());
    let mut box_rc = RECT {
        left: side,
        top: px(dpi, 80),
        right: rc.right - side,
        bottom: px(dpi, 250),
    };
    DrawTextW(
        hdc,
        &mut wide(
            "Records Discord and your microphone into one file.\n\n\
             Everyone in the call is being recorded — tell them before you start.",
        ),
        &mut box_rc,
        DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_NOPREFIX.0),
    );
    let l = layout(rc, dpi);
    let hover = app.hover == Hit::Continue;
    let fill = if hover {
        rgb(40, 40, 44)
    } else {
        rgb(36, 36, 40)
    };
    let brush = CreateSolidBrush(fill);
    FillRect(hdc, &l.continue_btn, brush);
    DeleteObject(brush.into());
    SetTextColor(hdc, rgb(COL_TEXT.0, COL_TEXT.1, COL_TEXT.2));
    center_text(hdc, rc, l.continue_btn.top + px(dpi, 10), "Continue", dpi);
}

unsafe fn draw_record_button(hdc: HDC, app: &App, r: &RECT) {
    let old_pen = SelectObject(hdc, GetStockObject(NULL_BRUSH));
    let rec_pen = CreatePen(PS_SOLID, px(app.dpi, 2).max(1), rgb(COL_REC.0, COL_REC.1, COL_REC.2));
    if app.recording {
        SelectObject(hdc, rec_pen.into());
        SelectObject(hdc, app.bg.into());
        Ellipse(hdc, r.left, r.top, r.right, r.bottom);
        SelectObject(hdc, app.rec_brush.into());
        let pad = px(app.dpi, 22);
        let rr = px(app.dpi, 8);
        RoundRect(
            hdc,
            r.left + pad,
            r.top + pad,
            r.right - pad,
            r.bottom - pad,
            rr,
            rr,
        );
    } else {
        SelectObject(hdc, app.rec_brush.into());
        SelectObject(hdc, rec_pen.into());
        Ellipse(hdc, r.left, r.top, r.right, r.bottom);
    }
    SelectObject(hdc, old_pen);
    DeleteObject(rec_pen.into());
}

unsafe fn draw_meter(hdc: HDC, app: &App, x: i32, y: i32, w: i32, level: f32) {
    let h = px(app.dpi, 8);
    let rr = px(app.dpi, 8);
    RoundRect(hdc, x, y, x + w, y + h, rr, rr);
    FillRect(
        hdc,
        &RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        app.meter_bg,
    );
    let fill_w = ((level.clamp(0.0, 1.0) * w as f32) as i32).max(if level > 0.002 { 3 } else { 0 });
    if fill_w > 0 {
        FillRect(
            hdc,
            &RECT {
                left: x,
                top: y,
                right: x + fill_w,
                bottom: y + h,
            },
            app.meter_fg,
        );
    }
}

fn status_text(app: &App) -> (&str, Option<&str>) {
    match &app.status {
        Status::Notice => ("", None),
        Status::Ready => (
            "Ready. Tell people in the call before you press record.",
            disk_warn(),
        ),
        Status::Starting => ("Starting capture…", None),
        Status::DiscordMissing => ("Start Discord first.", None),
        Status::DiskRefuse => ("Not enough disk space (need 500 MB free).", None),
        Status::Recording => ("Keep talking — quiet is fine, the file keeps going.", None),
        Status::Saved(name) => ("Saved.", Some(name.as_str())),
        Status::Error(m) => (m.as_str(), None),
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
    let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0), ID_TRAY_STOP as usize, w!("Stop"));
    let _ = AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        ID_TRAY_FOLDER as usize,
        w!("Show in folder"),
    );
    let _ = SetMenuDefaultItem(menu, ID_TRAY_STOP as u32, 0);
    let mut pt = POINT::default();
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
    let _ = DeleteObject(app.rec_brush.into());
    let _ = DeleteObject(app.font.into());
    let _ = DeleteObject(app.font_title.into());
    let _ = DeleteObject(app.font_small.into());
    if !app.icon.is_invalid() {
        let _ = DestroyIcon(app.icon);
    }
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

unsafe fn center_text(hdc: HDC, rc: &RECT, y: i32, s: &str, dpi: u32) {
    let mut box_rc = RECT {
        left: 0,
        top: y,
        right: rc.right,
        bottom: y + px(dpi, 24),
    };
    DrawTextW(
        hdc,
        &mut wide(s),
        &mut box_rc,
        DRAW_TEXT_FORMAT(
            windows::Win32::Graphics::Gdi::DT_CENTER.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0,
        ),
    );
}
