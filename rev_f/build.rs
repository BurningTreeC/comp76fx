//! Every revision is an Audio Unit of its own, and a host loads them all into
//! one process, so each compiles the Cocoa view factory under a class name of
//! its own. See `vendor/nice-plug-au2/PATCHES.md`. Does nothing off macOS.

include!("../vendor/nice-plug-au2/view_factory.rs");

fn main() {
    compile_view_factory("../vendor/nice-plug-au2/src/bridge/shim.m");
}
