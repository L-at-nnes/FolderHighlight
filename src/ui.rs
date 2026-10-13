//! Task-dialog styled windows: text input and folder-color palette.

use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win::{remove_custom_color, w};

const SS_NOPREFIX: u32 = 0x80;
const ID_MAIN: i32 = 10; // headline
const ID_HINT: i32 = 11; // secondary text
const ID_EDIT: i32 = 100;
const ID_OTHER: i32 = 3;
const BAR_H: i32 = 52; // bottom button bar
const BLUE: u32 = 0x0099_3300; // COLORREF (BGR) of the headline color, as in task dialogs

fn dpi(v: i32) -> i32 {
    v * unsafe { GetDpiForSystem() } as i32 / 96
}

unsafe fn ui_font(px: i32) -> HFONT {
    CreateFontW(-dpi(px), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w("Segoe UI").as_ptr())
}

unsafe fn register(name: &[u16], proc_: WNDPROC) {
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = proc_;
    wc.hInstance = GetModuleHandleW(null());
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.hbrBackground = (COLOR_WINDOW + 1) as HBRUSH;
    wc.lpszClassName = name.as_ptr();
    RegisterClassW(&wc);
}

/// Centered, topmost dialog-like window with the given client size (in pixels).
unsafe fn create_window(class: &[u16], title: &str, cw: i32, ch: i32) -> HWND {
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let ex = WS_EX_TOPMOST | WS_EX_DLGMODALFRAME;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: cw,
        bottom: ch,
    };
    AdjustWindowRectEx(&mut r, style, 0, ex);
    let (ww, wh) = (r.right - r.left, r.bottom - r.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - ww) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - wh) / 3;
    CreateWindowExW(
        ex,
        class.as_ptr(),
        w(title).as_ptr(),
        style,
        x,
        y,
        ww,
        wh,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    )
}

/// Child control; `r` is [x, y, width, height] at 96 dpi.
#[allow(clippy::too_many_arguments)]
unsafe fn child(parent: HWND, font: HFONT, class: &str, text: &str, style: u32, ex: u32, r: [i32; 4], id: i32) -> HWND {
    let h = CreateWindowExW(
        ex,
        w(class).as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        dpi(r[0]),
        dpi(r[1]),
        dpi(r[2]),
        dpi(r[3]),
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, font as WPARAM, 1);
    h
}

/// Draws the grey button bar at the bottom of the client area.
unsafe fn paint_bar(hdc: HDC, hwnd: HWND) {
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    rc.top = rc.bottom - dpi(BAR_H);
    FillRect(hdc, &rc, GetSysColorBrush(COLOR_BTNFACE));
    rc.bottom = rc.top + 1;
    FillRect(hdc, &rc, GetSysColorBrush(COLOR_3DLIGHT));
}

/// Headline in blue, hint in grey, both on the white background.
unsafe fn static_color(wp: WPARAM, lp: LPARAM) -> LRESULT {
    let hdc = wp as HDC;
    let color = if GetDlgCtrlID(lp as HWND) == ID_MAIN {
        BLUE
    } else {
        GetSysColor(COLOR_GRAYTEXT)
    };
    SetTextColor(hdc, color);
    SetBkMode(hdc, TRANSPARENT as i32);
    GetSysColorBrush(COLOR_WINDOW) as LRESULT
}

/// Message loop; Esc cancels (id 2) and, with `enter`, Enter confirms (id 1).
unsafe fn pump(hwnd: HWND, enter: bool) {
    let mut m: MSG = std::mem::zeroed();
    while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
        let esc = m.message == WM_KEYDOWN && m.wParam == VK_ESCAPE as usize;
        let ret = enter && m.message == WM_KEYDOWN && m.wParam == VK_RETURN as usize;
        if esc || ret {
            let cancel = esc || GetDlgCtrlID(GetFocus()) == 2;
            SendMessageW(hwnd, WM_COMMAND, if cancel { 2 } else { 1 }, 0);
        } else if IsDialogMessageW(hwnd, &m) == 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}

// ---------- text input ----------

thread_local! {
    static TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
    static EDIT: Cell<HWND> = const { Cell::new(null_mut()) };
}

unsafe extern "system" fn input_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (wp & 0xFFFF) as i32 {
                1 => {
                    let mut buf = [0u16; 2048];
                    let n = GetWindowTextW(EDIT.get(), buf.as_mut_ptr(), buf.len() as i32);
                    TEXT.set(Some(String::from_utf16_lossy(&buf[..n as usize])));
                    DestroyWindow(hwnd);
                }
                2 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CTLCOLORSTATIC => static_color(wp, lp),
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_bar(hdc, hwnd);
            EndPaint(hwnd, &ps);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Text prompt: `headline` and `hint` above an edit box. None if cancelled.
pub fn input(title: &str, headline: &str, hint: &str, initial: &str) -> Option<String> {
    unsafe {
        let class = w("FolderHighlightInput");
        register(&class, Some(input_proc));
        let hwnd = create_window(&class, title, dpi(460), dpi(176));
        let (font, big) = (ui_font(12), ui_font(16));
        let by = 176 - BAR_H + 12;
        child(
            hwnd,
            big,
            "STATIC",
            headline,
            SS_NOPREFIX,
            0,
            [20, 14, 420, 24],
            ID_MAIN,
        );
        child(hwnd, font, "STATIC", hint, SS_NOPREFIX, 0, [20, 44, 420, 18], ID_HINT);
        let edit = child(
            hwnd,
            font,
            "EDIT",
            initial,
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            WS_EX_CLIENTEDGE,
            [20, 72, 420, 26],
            ID_EDIT,
        );
        child(
            hwnd,
            font,
            "BUTTON",
            "OK",
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            0,
            [268, by, 84, 28],
            1,
        );
        child(hwnd, font, "BUTTON", "Cancel", WS_TABSTOP, 0, [360, by, 80, 28], 2);
        EDIT.set(edit);
        TEXT.set(None);
        SendMessageW(edit, 0xC5, 1000, 0); // EM_LIMITTEXT
        SendMessageW(edit, 0xB1, 0, -1); // EM_SETSEL: select all
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(edit);
        pump(hwnd, true);
        DeleteObject(font);
        DeleteObject(big);
    }
    TEXT.take()
}

// ---------- color palette ----------

struct Palette {
    items: Vec<Swatch>,
    hover: Option<usize>,
    result: Option<(u32, bool)>,
    font: HFONT,
}

/// One palette cell; `custom` ones were saved by the user and can be removed.
struct Swatch {
    name: String,
    rgb: u32,
    icon: HICON,
    custom: bool,
}

thread_local! {
    static PAL: RefCell<Palette> = const { RefCell::new(Palette { items: Vec::new(), hover: None, result: None, font: null_mut() }) };
}

const COLS: usize = 6;
const CELL_W: i32 = 88;
const CELL_H: i32 = 84;
const MARGIN: i32 = 16;
const HEADER: i32 = 72;
const ICON: i32 = 48;

fn cell_at(x: i32, y: i32, n: usize) -> Option<usize> {
    let (cx, cy) = (x - dpi(MARGIN), y - dpi(HEADER));
    if cx < 0 || cy < 0 {
        return None;
    }
    let (col, row) = (cx / dpi(CELL_W), cy / dpi(CELL_H));
    let i = row as usize * COLS + col as usize;
    (col < COLS as i32 && i < n).then_some(i)
}

fn mouse_xy(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as i16 as i32, (lp >> 16 & 0xFFFF) as i16 as i32)
}

unsafe extern "system" fn palette_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (wp & 0xFFFF) as i32 {
                ID_OTHER => {
                    if let Some(c) = pick_color(0x2EAE3B) {
                        PAL.with_borrow_mut(|p| p.result = Some((c, true)));
                        DestroyWindow(hwnd);
                    }
                }
                2 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CTLCOLORSTATIC => static_color(wp, lp),
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lp);
            let changed = PAL.with_borrow_mut(|p| {
                let h = cell_at(x, y, p.items.len());
                std::mem::replace(&mut p.hover, h) != h
            });
            if changed {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut t);
                InvalidateRect(hwnd, null(), 1);
            }
            0
        }
        0x02A3 /* WM_MOUSELEAVE */ => {
            PAL.with_borrow_mut(|p| p.hover = None);
            InvalidateRect(hwnd, null(), 1);
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = mouse_xy(lp);
            let hit = PAL.with_borrow_mut(|p| {
                let i = cell_at(x, y, p.items.len())?;
                p.result = Some((p.items[i].rgb, false));
                Some(())
            });
            if hit.is_some() {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_RBUTTONUP => {
            let (x, y) = mouse_xy(lp);
            let hit = PAL.with_borrow(|p| cell_at(x, y, p.items.len()).filter(|&i| p.items[i].custom));
            if let Some(i) = hit {
                let menu = CreatePopupMenu();
                AppendMenuW(menu, MF_STRING, 1, w("Remove").as_ptr());
                let mut pt = POINT { x, y };
                ClientToScreen(hwnd, &mut pt);
                SetForegroundWindow(hwnd); // required for the menu to dismiss properly
                let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
                DestroyMenu(menu);
                if cmd == 1 {
                    PAL.with_borrow_mut(|p| {
                        let sw = p.items.remove(i);
                        remove_custom_color(sw.rgb);
                        DestroyIcon(sw.icon);
                        p.hover = None;
                    });
                    InvalidateRect(hwnd, null(), 1);
                }
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_bar(hdc, hwnd);
            PAL.with_borrow(|p| {
                let old = SelectObject(hdc, p.font);
                SetBkMode(hdc, TRANSPARENT as i32);
                let hot = CreateSolidBrush(0x00FF_E8CD);
                for (i, sw) in p.items.iter().enumerate() {
                    let (col, row) = ((i % COLS) as i32, (i / COLS) as i32);
                    let (x, y) = (dpi(MARGIN + col * CELL_W), dpi(HEADER + row * CELL_H));
                    let cell = RECT { left: x, top: y, right: x + dpi(CELL_W), bottom: y + dpi(CELL_H) };
                    if p.hover == Some(i) {
                        FillRect(hdc, &cell, hot);
                    }
                    DrawIconEx(hdc, x + (dpi(CELL_W) - dpi(ICON)) / 2, y + dpi(6), sw.icon, dpi(ICON), dpi(ICON), 0, null_mut(), DI_NORMAL);
                    let mut label = RECT { left: x, top: y + dpi(ICON + 10), right: x + dpi(CELL_W), bottom: y + dpi(CELL_H) };
                    DrawTextW(hdc, w(&sw.name).as_ptr(), -1, &mut label, DT_CENTER | DT_TOP | DT_SINGLELINE | DT_NOPREFIX);
                }
                DeleteObject(hot);
                SelectObject(hdc, old);
            });
            EndPaint(hwnd, &ps);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// System color dialog, used by the palette's "Other...". Returns 0xRRGGBB.
fn pick_color(initial: u32) -> Option<u32> {
    let bgr = |c: u32| (c & 0xFF) << 16 | (c & 0xFF00) | (c >> 16 & 0xFF);
    let mut custom = [0xFFFFFFu32; 16];
    let mut cc: CHOOSECOLORW = unsafe { std::mem::zeroed() };
    cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    cc.rgbResult = bgr(initial);
    cc.lpCustColors = custom.as_mut_ptr();
    cc.Flags = CC_RGBINIT | CC_FULLOPEN;
    (unsafe { ChooseColorW(&mut cc) } != 0).then(|| bgr(cc.rgbResult))
}

/// Folder-color palette (name, 0xRRGGBB, user-saved). "Other..." opens the system color dialog.
/// Returns the chosen color and whether it came from "Other...".
pub fn palette(title: &str, colors: &[(String, u32, bool)]) -> Option<(u32, bool)> {
    unsafe {
        let class = w("FolderHighlightPalette");
        register(&class, Some(palette_proc));
        let rows = colors.len().div_ceil(COLS) as i32;
        let client_h = HEADER + rows * CELL_H + 12 + BAR_H;
        let hwnd = create_window(&class, title, dpi(2 * MARGIN + COLS as i32 * CELL_W), dpi(client_h));
        let (font, big) = (ui_font(12), ui_font(16));
        let size = dpi(ICON);
        let items = colors
            .iter()
            .map(|(name, rgb, custom)| {
                let ico = crate::icon::folder_ico(*rgb, &[size as usize]);
                let png = &ico[22..]; // single image: 6-byte header + 16-byte entry
                let icon = CreateIconFromResourceEx(
                    png.as_ptr(),
                    png.len() as u32,
                    1,
                    0x0003_0000,
                    size,
                    size,
                    LR_DEFAULTCOLOR,
                );
                Swatch {
                    name: name.clone(),
                    rgb: *rgb,
                    icon,
                    custom: *custom,
                }
            })
            .collect();
        PAL.set(Palette {
            items,
            hover: None,
            result: None,
            font,
        });
        child(
            hwnd,
            big,
            "STATIC",
            "Choose a folder color",
            SS_NOPREFIX,
            0,
            [MARGIN + 4, 16, 500, 24],
            ID_MAIN,
        );
        child(
            hwnd,
            font,
            "STATIC",
            "Click a color, or pick any other one with \"Other...\". Right-click your own colors to remove them.",
            SS_NOPREFIX,
            0,
            [MARGIN + 4, 44, 520, 18],
            ID_HINT,
        );
        let by = client_h - BAR_H + 12;
        let right = COLS as i32 * CELL_W + MARGIN;
        child(
            hwnd,
            font,
            "BUTTON",
            "Other...",
            WS_TABSTOP,
            0,
            [right - 84 - 8 - 84, by, 84, 28],
            ID_OTHER,
        );
        child(
            hwnd,
            font,
            "BUTTON",
            "Cancel",
            WS_TABSTOP,
            0,
            [right - 84, by, 84, 28],
            2,
        );
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        pump(hwnd, false);
        DeleteObject(font);
        DeleteObject(big);
    }
    PAL.with_borrow_mut(|p| {
        for sw in p.items.drain(..) {
            unsafe { DestroyIcon(sw.icon) };
        }
        p.result.take()
    })
}
