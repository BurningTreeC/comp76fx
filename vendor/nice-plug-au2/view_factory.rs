// Compiles the Cocoa view factory (`src/bridge/shim.m`) into the plugin crate
// whose build script includes this file, under a class name of that crate's
// own, for use with this crate's `external-view-factory` feature.
//
// A host loads every Audio Unit into one process, and Objective-C has one
// class namespace per process. Compiled here once under a shared name, the
// factory of whichever plugin loaded first would build every other plugin's
// editor. Compiled in each plugin crate, the name can carry the package name.
//
//     // build.rs, with `cc` as a build dependency
//     include!("path/to/nice-plug-au2/view_factory.rs");
//     fn main() {
//         compile_view_factory("path/to/nice-plug-au2/src/bridge/shim.m");
//     }
//
// `shim` is relative to the package being built. Nothing happens unless the
// target is macOS.

fn compile_view_factory(shim: &str) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    println!("cargo:rerun-if-changed={shim}");

    let package = std::env::var("CARGO_PKG_NAME").expect("Cargo always sets CARGO_PKG_NAME");
    let suffix: String = package
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let class = format!("NiceAu2CocoaViewFactory_{suffix}");

    cc::Build::new()
        .file(shim)
        .flag("-fobjc-arc")
        .define("NICE_AU2_VIEW_FACTORY", class.as_str())
        .compile("nice_plug_au2_cocoa_shim");
}
