//! Thin Win32 helpers: registry, desktop.ini access, dialogs.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::WindowsProgramming::{GetPrivateProfileStringW, WritePrivateProfileStringW};
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Shell::*;

pub fn w(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Native task dialog. `flags` are MessageBox-style: 0x3 yes/no/cancel, 0x4 yes/no, 0x10 error,
/// 0x30 warning, 0x20/0x40 information. The text before the first blank line is the headline.
/// Returns IDOK/IDCANCEL/IDYES/IDNO (1/2/6/7).
pub fn message(text: &str, title: &str, flags: u32) -> i32 {
    let (main, content) = text
        .split_once(
            "

",
        )
        .unwrap_or((text, ""));
    let (main, content, title) = (w(main), w(content), w(title));
    let buttons = match flags & 0xF {
        3 => TDCBF_YES_BUTTON | TDCBF_NO_BUTTON | TDCBF_CANCEL_BUTTON,
        4 => TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
        _ => TDCBF_OK_BUTTON,
    };
    let icon = match flags & 0xF0 {
        0x10 => TD_ERROR_ICON,
        0x30 => TD_WARNING_ICON,
        _ => TD_INFORMATION_ICON,
    };
    let mut cfg: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
    cfg.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
    cfg.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION;
    cfg.dwCommonButtons = buttons;
    cfg.pszWindowTitle = title.as_ptr();
    cfg.Anonymous1.pszMainIcon = icon;
    cfg.pszMainInstruction = main.as_ptr();
    if !content.is_empty() && content[0] != 0 {
        cfg.pszContent = content.as_ptr();
    }
    let mut button = 0;
    unsafe { TaskDialogIndirect(&cfg, &mut button, null_mut(), null_mut()) };
    button
}

// ---------- registry (HKCU) ----------

pub fn reg_set(path: &str, name: Option<&str>, value: &str) {
    reg_write(
        path,
        name,
        REG_SZ,
        &w(value).iter().flat_map(|c| c.to_le_bytes()).collect::<Vec<_>>(),
    );
}

pub fn reg_set_dword(path: &str, name: &str, value: u32) {
    reg_write(path, Some(name), REG_DWORD, &value.to_le_bytes());
}

fn reg_write(path: &str, name: Option<&str>, kind: u32, data: &[u8]) {
    unsafe {
        let mut key: HKEY = null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w(path).as_ptr(),
            0,
            null(),
            0,
            KEY_WRITE,
            null(),
            &mut key,
            null_mut(),
        ) != 0
        {
            return;
        }
        let n = name.map(w);
        RegSetValueExW(
            key,
            n.as_ref().map_or(null(), |n| n.as_ptr()),
            0,
            kind,
            data.as_ptr(),
            data.len() as u32,
        );
        RegCloseKey(key);
    }
}

pub fn reg_get(path: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 2048];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w(path).as_ptr(),
            w(name).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (r == 0).then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

pub fn reg_delete_tree(path: &str) {
    unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, w(path).as_ptr()) };
}

pub fn reg_exists(path: &str) -> bool {
    unsafe {
        let mut key: HKEY = null_mut();
        let ok = RegOpenKeyExW(HKEY_CURRENT_USER, w(path).as_ptr(), 0, KEY_READ, &mut key) == 0;
        if ok {
            RegCloseKey(key);
        }
        ok
    }
}

// ---------- files / shell ----------

pub fn attrs(p: &Path) -> u32 {
    unsafe { GetFileAttributesW(w(p).as_ptr()) }
}

pub fn set_attrs(p: &Path, a: u32) {
    unsafe { SetFileAttributesW(w(p).as_ptr(), a) };
}

pub fn notify(p: &Path) {
    const FLUSH: u32 = SHCNF_PATHW | SHCNF_FLUSH;
    let wp = w(p);
    let ini = w(p.join("desktop.ini"));
    unsafe {
        SHChangeNotify(SHCNE_ATTRIBUTES as i32, FLUSH, wp.as_ptr().cast(), null());
        SHChangeNotify(SHCNE_UPDATEITEM as i32, FLUSH, ini.as_ptr().cast(), null());
        SHChangeNotify(SHCNE_UPDATEITEM as i32, FLUSH, wp.as_ptr().cast(), null());
        if let Some(parent) = p.parent() {
            SHChangeNotify(SHCNE_UPDATEDIR as i32, FLUSH, w(parent).as_ptr().cast(), null());
        }
    }
}

pub fn notify_assoc_changed() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST | SHCNF_FLUSH, null(), null()) };
}

// ---------- desktop.ini ([.ShellClassInfo]) ----------

pub fn ini_read(ini: &Path, key: &str) -> String {
    let mut buf = [0u16; 2048];
    let n = unsafe {
        GetPrivateProfileStringW(
            w(".ShellClassInfo").as_ptr(),
            w(key).as_ptr(),
            w("").as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            w(ini).as_ptr(),
        )
    };
    String::from_utf16_lossy(&buf[..n as usize])
}

pub fn ini_write(ini: &Path, key: &str, value: Option<&str>) {
    let v = value.map(w);
    unsafe {
        WritePrivateProfileStringW(
            w(".ShellClassInfo").as_ptr(),
            w(key).as_ptr(),
            v.as_ref().map_or(null(), |v| v.as_ptr()),
            w(ini).as_ptr(),
        );
    }
}

/// Sets the folder icon through the shell itself (writes desktop.ini and refreshes Explorer).
pub fn shell_set_icon(folder: &Path, icon: &str) -> bool {
    let mut icon_buf = w(icon);
    let mut s: SHFOLDERCUSTOMSETTINGS = unsafe { std::mem::zeroed() };
    s.dwSize = std::mem::size_of::<SHFOLDERCUSTOMSETTINGS>() as u32;
    s.dwMask = FCSM_ICONFILE;
    s.pszIconFile = icon_buf.as_mut_ptr();
    s.iIconIndex = 0;
    unsafe { SHGetSetFolderCustomSettings(&mut s, w(folder).as_ptr(), FCS_FORCEWRITE) == 0 }
}

// ---------- saved custom colors (HKCU\\Software\\FolderHighlight, "Colors" = "rrggbb,rrggbb") ----------

const SETTINGS: &str = "Software\\FolderHighlight";

pub fn custom_colors() -> Vec<u32> {
    reg_get(SETTINGS, "Colors")
        .unwrap_or_default()
        .split(',')
        .filter_map(|h| u32::from_str_radix(h, 16).ok())
        .collect()
}

fn save_custom_colors(colors: &[u32]) {
    let list: Vec<String> = colors.iter().map(|c| format!("{c:06x}")).collect();
    reg_set(SETTINGS, Some("Colors"), &list.join(","));
}

pub fn add_custom_color(rgb: u32) {
    let mut all = custom_colors();
    if !all.contains(&rgb) {
        all.push(rgb);
        save_custom_colors(&all);
    }
}

pub fn remove_custom_color(rgb: u32) {
    let mut all = custom_colors();
    all.retain(|&c| c != rgb);
    save_custom_colors(&all);
}
