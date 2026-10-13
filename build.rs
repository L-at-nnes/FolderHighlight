//! Embeds app.manifest and a generated application icon (hand-written .res, no rc.exe needed).

#[allow(dead_code)]
#[path = "src/colors.rs"]
mod colors;
#[path = "src/icon.rs"]
mod icon;

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// Appends one .res entry with numeric type and name ids.
fn res_entry(out: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    le32(out, data.len() as u32);
    le32(out, 32);
    for id in [kind, name] {
        le16(out, 0xFFFF);
        le16(out, id);
    }
    le32(out, 0); // data version
    le16(out, 0x1030); // memory flags
    le16(out, 0); // language neutral
    le32(out, 0); // version
    le32(out, 0); // characteristics
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let mut res = Vec::new();
    res_entry(&mut res, 0, 0, &[]); // mandatory empty first entry
    let mut next_icon = 1u16;
    // group 1 = application icon (full size); groups 101.. = small menu icons, one per color
    let mut add_group = |res: &mut Vec<u8>, group_id: u16, rgb: u32, sizes: &[usize]| {
        let ico = icon::folder_ico(rgb, sizes);
        let mut group = vec![0, 0, 1, 0];
        le16(&mut group, sizes.len() as u16);
        for i in 0..sizes.len() {
            let e = &ico[6 + 16 * i..22 + 16 * i];
            let size = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
            let off = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
            res_entry(res, 3, next_icon, &ico[off..off + size]); // RT_ICON
            group.extend_from_slice(&e[..12]);
            le16(&mut group, next_icon);
            next_icon += 1;
        }
        res_entry(res, 14, group_id, &group); // RT_GROUP_ICON
    };
    add_group(&mut res, 1, 0x2EAE3B, &icon::SIZES);
    for (i, c) in colors::COLORS.iter().enumerate() {
        add_group(&mut res, 101 + i as u16, c.2, &[16, 24, 32]);
    }
    let res_path = format!("{out_dir}/app.res");
    std::fs::write(&res_path, res).unwrap();

    println!("cargo:rustc-link-arg-bins={res_path}");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/app.manifest");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=src/colors.rs");
}
