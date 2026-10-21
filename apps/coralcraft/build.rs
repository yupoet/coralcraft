//! Windows only: embed the app icon and version info (VERSIONINFO) into `coralcraft.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `CORALCRAFT_REQUIRE_WINRES=1` turns it into
//! an error (for release builds).

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/coralcraft.ico");
    println!("cargo:rerun-if-env-changed=CORALCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let mut res = winresource::WindowsResource::new();
    // CoralCraft: fork identity; the copyright keeps the upstream authors alongside ours.
    res.set_icon("../../assets/app-icon/coralcraft.ico")
        .set("ProductName", "CoralCraft")
        .set("FileDescription", "CoralCraft vector illustration editor")
        .set("CompanyName", "The CoralCraft project authors")
        .set("LegalCopyright", "Copyright (c) the CoralCraft and VectorCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "coralcraft.exe")
        .set("InternalName", "coralcraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("CORALCRAFT_REQUIRE_WINRES").is_some() {
            return Err(format!("embedding Windows resources failed: {e}"));
        }
        println!("cargo:warning=coralcraft.exe built without icon/version resources: {e}");
    }
    Ok(())
}
