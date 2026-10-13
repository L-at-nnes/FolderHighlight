#![windows_subsystem = "windows"]

mod colors;
mod icon;
mod ui;
mod win;

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use colors::{COLORS, EXTRA};
use win::*;

const TITLE: &str = "FolderHighlight";
/// Explorer caps a menu cascade at 16 entries in total, so only 12 colors fit next to
/// Custom/Clean/Infotip/About (any other color: "Custom color...").
/// Explorer sorts static verbs by key name: the "zz" puts the menu at the bottom of the first section.
const MAIN_KEY: &str = "zzFolderHighlight";
const OLD_KEY: &str = "FolderHighlight"; // legacy, only removed
const MORE_KEY: &str = "FolderHighlightMore"; // legacy, only removed
const CLASSES: [(&str, &str); 2] = [
    ("Software\\Classes\\Directory\\shell", "%1"),
    ("Software\\Classes\\Directory\\Background\\shell", "%V"),
];

const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const INVALID_ATTRS: u32 = u32::MAX;

fn exe_path() -> PathBuf {
    let p = std::env::current_exe().unwrap_or_default();
    PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\"))
}

/// Menu icon: the folder icon embedded in this exe (see build.rs).
fn menu_icon(rgb: u32) -> String {
    let i = COLORS.iter().position(|c| c.2 == rgb).unwrap_or(0);
    format!("\"{}\",-{}", exe_path().display(), 101 + i)
}

fn parse_color(id: &str) -> Option<u32> {
    if let Some(hex) = id.strip_prefix('#') {
        return u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6);
    }
    COLORS.iter().chain(EXTRA).find(|c| c.0 == id).map(|c| c.2)
}

// ---------- desktop.ini editing ----------

/// Sets (or removes, with None) keys in the folder's desktop.ini; deletes the file once empty.
fn edit_folder(folder: &Path, edits: &[(&str, Option<&str>)]) {
    let ini = folder.join("desktop.ini");
    let existed = ini.exists();
    if !existed {
        if edits.iter().all(|e| e.1.is_none()) {
            return;
        }
        // UTF-16 BOM so the profile API stores non-ASCII text correctly
        if fs::write(&ini, [0xFF, 0xFE]).is_err() {
            message(&format!("Cannot write in:\n{}", folder.display()), TITLE, 0x10);
            return;
        }
    } else {
        set_attrs(&ini, 0x80);
    }
    for (key, val) in edits {
        ini_write(&ini, key, *val);
    }
    if has_entries(&ini) {
        set_attrs(&ini, FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
        let a = attrs(folder);
        if a != INVALID_ATTRS {
            set_attrs(folder, a | FILE_ATTRIBUTE_READONLY);
        }
    } else {
        let _ = fs::remove_file(&ini);
        let a = attrs(folder);
        if a != INVALID_ATTRS {
            set_attrs(folder, a & !FILE_ATTRIBUTE_READONLY);
        }
    }
    notify(folder);
}

fn has_entries(ini: &Path) -> bool {
    let Ok(bytes) = fs::read(ini) else { return false };
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        #[allow(clippy::chunks_exact_to_as_chunks)]
        let u: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    text.lines().any(|l| l.contains('='))
}

const ICON_PREFIX: &str = "FolderHighlight_";

/// Icon files FolderHighlight stored inside a folder ("FolderHighlight_<rrggbb>.ico").
fn folder_icon_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.starts_with(&ICON_PREFIX.to_lowercase()) && n.ends_with(".ico")
        })
        .map(|e| e.path())
        .collect()
}

fn remove_icon_files(dir: &Path, keep: Option<&Path>) {
    for f in folder_icon_files(dir) {
        if Some(f.as_path()) != keep {
            set_attrs(&f, 0x80);
            let _ = fs::remove_file(f);
        }
    }
}

/// The icon is copied into the folder itself, so the color survives copying it to another PC.
fn set_color(folder: &Path, rgb: Option<u32>) {
    const NO_ICON: [(&str, Option<&str>); 3] = [("IconResource", None), ("IconFile", None), ("IconIndex", None)];
    let Some(c) = rgb else {
        edit_folder(folder, &NO_ICON);
        remove_icon_files(folder, None);
        return;
    };
    let name = format!("{ICON_PREFIX}{c:06x}.ico");
    let path = folder.join(&name);
    if !path.exists() {
        if fs::write(&path, icon::folder_ico(c, &icon::SIZES)).is_err() {
            message(
                &format!(
                    "Cannot write in:
{}",
                    folder.display()
                ),
                TITLE,
                0x10,
            );
            return;
        }
        set_attrs(&path, FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
    }
    // The shell API (unlike a plain desktop.ini edit) makes Explorer refresh the icon immediately
    if !shell_set_icon(folder, &name) {
        edit_folder(
            folder,
            &[
                ("IconResource", Some(&format!("{name},0"))),
                ("IconFile", None),
                ("IconIndex", None),
            ],
        );
    }
    remove_icon_files(folder, Some(&path));
}

/// True for an icon reference created by FolderHighlight (the in-folder icon file).
fn is_our_icon(res: &str) -> bool {
    let file = res.rsplit_once(',').map_or(res, |r| r.0);
    let name = Path::new(file)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.starts_with(&ICON_PREFIX.to_lowercase())
}

/// Removes only what FolderHighlight could have set (its icons, non-resource infotips).
fn clean_tree(dir: &Path) {
    let ini = dir.join("desktop.ini");
    if ini.exists() {
        let mut edits = Vec::new();
        if is_our_icon(&ini_read(&ini, "IconResource")) {
            edits.extend([("IconResource", None), ("IconFile", None), ("IconIndex", None)]);
        }
        let tip = ini_read(&ini, "InfoTip");
        if !tip.is_empty() && !tip.starts_with('@') {
            edits.push(("InfoTip", None));
        }
        if !edits.is_empty() {
            edit_folder(dir, &edits);
        }
    }
    remove_icon_files(dir, None);
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
            clean_tree(&e.path());
        }
    }
}

// ---------- commands ----------

fn cmd_set(id: &str, folders: &[String]) {
    let Some(rgb) = parse_color(id) else { return };
    let rgb = (id != "default").then_some(rgb);
    for f in folders {
        set_color(Path::new(f), rgb);
    }
}

fn cmd_custom(folders: &[String]) {
    let Some(folders) = gather("custom", folders) else {
        return;
    };
    let mut colors: Vec<(String, u32, bool)> = COLORS[1..]
        .iter()
        .chain(EXTRA)
        .map(|c| (c.1.to_string(), c.2, false))
        .collect();
    for rgb in custom_colors() {
        colors.push((format!("#{rgb:06X}"), rgb, true));
    }
    let Some((rgb, other)) = ui::palette("FolderHighlight", &colors) else {
        return;
    };
    if other && !colors.iter().any(|c| c.1 == rgb) {
        add_custom_color(rgb);
    }
    for f in &folders {
        set_color(Path::new(f), Some(rgb));
    }
}

fn cmd_infotip(folders: &[String]) {
    for f in folders {
        let folder = Path::new(f);
        let current = ini_read(&folder.join("desktop.ini"), "InfoTip");
        let name = folder
            .file_name()
            .map_or_else(|| f.clone(), |n| n.to_string_lossy().into_owned());
        let headline = format!("InfoTip for \"{name}\"");
        let hint = "Text shown when hovering the folder. Leave empty to remove it.";
        let Some(text) = ui::input("FolderHighlight", &headline, hint, &current) else {
            continue;
        };
        let text = text.trim();
        edit_folder(folder, &[("InfoTip", (!text.is_empty()).then_some(text))]);
    }
}

fn cmd_clean(folders: &[String]) {
    let Some(folders) = gather("clean", folders) else {
        return;
    };
    let ask = if folders.len() == 1 {
        format!(
            "Remove the FolderHighlight color and infotip from this folder and all its subfolders?\n\n{}",
            folders[0]
        )
    } else {
        let list: Vec<_> = folders.iter().take(8).map(String::as_str).collect();
        let more = if folders.len() > 8 {
            format!("\n... and {} more", folders.len() - 8)
        } else {
            String::new()
        };
        format!(
            "Remove the FolderHighlight color and infotip from these {} folders and all their subfolders?\n\n{}{more}",
            folders.len(),
            list.join("\n")
        )
    };
    if message(&ask, TITLE, 0x4 | 0x30) == 6 {
        for f in &folders {
            clean_tree(Path::new(f));
        }
    }
}

fn cmd_about() {
    message(
        "FolderHighlight 2.0\n\nColor your folders from the Explorer context menu.\nA lightweight rewrite of the original FolderHighlight.\n\nEach colored folder carries its own icon file and desktop.ini, so colors survive copying to another PC.",
        "About FolderHighlight",
        0x40,
    );
}

/// Explorer starts one process per selected folder. The first process collects them all, so that
/// the confirmation / palette appears only once; the others get None and just exit.
fn gather(cmd: &str, folders: &[String]) -> Option<Vec<String>> {
    use std::time::Duration;
    let dir = std::env::temp_dir().join(format!("FolderHighlight-{cmd}"));
    let _ = fs::create_dir_all(&dir);
    let lock = dir.join("leader.lock");
    let mine: Vec<PathBuf> = (0..folders.len())
        .map(|i| dir.join(format!("{}-{i}.txt", std::process::id())))
        .collect();
    for (f, p) in folders.iter().zip(&mine) {
        let _ = fs::write(p, f);
    }
    loop {
        let stale = fs::metadata(&lock)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|e| e.as_secs() > 10);
        if stale {
            let _ = fs::remove_file(&lock);
        }
        if fs::OpenOptions::new().write(true).create_new(true).open(&lock).is_ok() {
            break;
        }
        // another process leads: wait for it, then check that it took our entries
        for _ in 0..50 {
            if !lock.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if mine.iter().all(|p| !p.exists()) {
            return None;
        }
    }
    std::thread::sleep(Duration::from_millis(700));
    let mut all = Vec::new();
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "txt") {
            if let Ok(t) = fs::read_to_string(e.path()) {
                all.push(t);
            }
            let _ = fs::remove_file(e.path());
        }
    }
    let _ = fs::remove_file(&lock);
    all.sort();
    all.dedup();
    Some(all)
}

/// Creates a menu entry under `parent`; `cmd` is its command line (None for a submenu entry).
fn node(parent: &str, n: usize, id: &str, label: &str, icon: &str, cmd: Option<String>, sep: bool) -> String {
    let key = format!("{parent}\\shell\\{n:02}_{id}");
    reg_set(&key, None, label);
    reg_set(&key, Some("MUIVerb"), label);
    if !icon.is_empty() {
        reg_set(&key, Some("Icon"), icon);
    }
    if sep {
        reg_set_dword(&key, "CommandFlags", 0x40);
    }
    if let Some(c) = cmd {
        reg_set(&format!("{key}\\command"), None, &c);
    }
    key
}

fn install() {
    let exe = exe_path();
    let exe = exe.display();
    uninstall();
    for (base, arg) in CLASSES {
        let cmd = |a: &str| Some(format!("\"{exe}\" {a} \"{arg}\""));
        let set = |parent: &str, n: usize, c: &(&str, &str, u32)| {
            node(
                parent,
                n,
                c.0,
                c.1,
                &menu_icon(c.2),
                cmd(&format!("set {}", c.0)),
                false,
            );
        };

        let main = format!("{base}\\{MAIN_KEY}");
        reg_set(&main, Some("MUIVerb"), TITLE);
        reg_set(&main, Some("Icon"), &menu_icon(0x2EAE3B));
        reg_set(&main, Some("SubCommands"), "");
        for (i, c) in COLORS.iter().enumerate() {
            set(&main, i + 1, c);
        }
        node(&main, 13, "custom", "Custom color...", "", cmd("custom"), true);
        node(&main, 14, "clean", "Clean...", "", cmd("clean"), true);
        node(&main, 15, "infotip", "Infotip...", "", cmd("infotip"), false);
        node(&main, 16, "about", "About...", "", cmd("about"), true);
    }
    notify_assoc_changed();
}

fn uninstall() {
    for (base, _) in CLASSES {
        reg_delete_tree(&format!("{base}\\{MAIN_KEY}"));
        reg_delete_tree(&format!("{base}\\{MORE_KEY}"));
        reg_delete_tree(&format!("{base}\\{OLD_KEY}"));
    }
    // leftovers of an earlier nested-submenu layout
    reg_delete_tree("Software\\Classes\\FolderHighlight.More");
    reg_delete_tree("Software\\Classes\\FolderHighlight.MoreBg");
    notify_assoc_changed();
}

fn interactive() {
    if reg_exists(&format!("{}\\{MAIN_KEY}", CLASSES[0].0)) {
        match message(
            "FolderHighlight is installed.\n\nYes = reinstall / refresh\nNo = uninstall",
            TITLE,
            0x3 | 0x20,
        ) {
            6 => {
                install();
                message("Context menu refreshed.", TITLE, 0x40);
            }
            7 => {
                uninstall();
                message("Context menu removed.", TITLE, 0x40);
            }
            _ => {}
        }
    } else if message(
        "Add FolderHighlight to the Explorer context menu?\n\n(Windows 11: right-click > Show more options)",
        TITLE,
        0x4 | 0x20,
    ) == 6
    {
        install();
        message("Installed. Right-click a folder to try it.\n\nKeep FolderHighlight.exe and the icons folder where they are.", TITLE, 0x40);
    }
}

fn main() {
    // A quoted path ending in a backslash ("D:\\") reaches us as `D:"`
    let args: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| a.trim_end().strip_suffix('"').map_or(a.clone(), |p| format!("{p}\\")))
        .collect();
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        None => interactive(),
        Some("install") => install(),
        Some("uninstall") => uninstall(),
        Some("set") if !rest.is_empty() => cmd_set(&rest[0], &rest[1..]),
        Some("custom") => cmd_custom(rest),
        Some("infotip") => cmd_infotip(rest),
        Some("clean") => cmd_clean(rest),
        Some("about") => cmd_about(),
        _ => {}
    }
}
