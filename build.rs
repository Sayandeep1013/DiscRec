// Embed the app icon and version info in discrec.exe, so Explorer, the Start
// menu and the taskbar show the DiscRec logo instead of the generic exe icon.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/app.ico")
        .set("ProductName", "DiscRec")
        .set("FileDescription", "DiscRec");
    res.compile().expect("failed to embed the Windows icon resource");
}
