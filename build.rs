//! Embeds assets/catalyx.ico as the program's icon (what Explorer, the taskbar and Task Manager show).
//! Writes the Windows resource file by hand, so no Windows SDK resource compiler is needed.

use std::io::Write;

fn entry(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes()); // header size
    out.extend_from_slice(&[0xFF, 0xFF]);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&[0xFF, 0xFF]);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // data version
    out.extend_from_slice(&0x1030u16.to_le_bytes()); // memory flags
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // language: English (US)
    out.extend_from_slice(&0u32.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/catalyx.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let Ok(ico) = std::fs::read("assets/catalyx.ico") else { return };
    if ico.len() < 6 {
        return;
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let mut res = Vec::new();
    entry(&mut res, 0, 0, &[]); // the mandatory empty first resource
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let e = &ico[6 + i * 16..6 + (i + 1) * 16];
        let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        let id = (i + 1) as u16;
        entry(&mut res, 3, id, &ico[off..off + size]); // RT_ICON
        group.extend_from_slice(&e[0..8]); // width, height, colours, reserved, planes, bit count
        group.extend_from_slice(&(size as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut res, 14, 1, &group); // RT_GROUP_ICON
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalyx.res");
    std::fs::File::create(&path).and_then(|mut f| f.write_all(&res)).expect("write catalyx.res");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}
