# Local nice-plug-au2 changes

Upstream: [fazibear/nice-plug-addons](https://codeberg.org/fazibear/nice-plug-addons),
`nice-plug-au2` 0.1.1, ported to nice-plug-core
`e60b5db09d8606c3dc3e93f982847a00bce17325`.

This copy starts from the one vendored in the sibling GainStageFx repository,
which carries that project's fixes to the build (the Cocoa view factory is
compiled from `src/bridge/shim.m`) and to `kAudioUnitProperty_ClassInfo` (the
component identity and version auval checks for). GainStageFx ships one Audio
Unit; Comp76Fx ships three, which a host can load into one process together,
and that needed the following changes on top.

## ClassInfo reports the plugin that registered

`kAudioUnitProperty_ClassInfo` took its type, subtype, manufacturer and name
from constants spelled out for GainStageFx. They now come from the
`Au2Config` the binary registered (`factory::plugin_config`). With the
constants, every other plugin saved presets claiming to be GainStageFx, and
auval's class-info check failed on it.

## One Cocoa view factory class per plugin

AUv2 hosts find a plugin's editor by asking its bundle for an Objective-C
class by name (`kAudioUnitProperty_CocoaUI`), and Objective-C has one class
namespace per process. Compiled once under the fixed name
`NiceAu2CocoaViewFactory`, the factory of whichever plugin loaded first would
build every later plugin's editor, with the first plugin's code and the later
plugin's instance.

- `shim.m` takes the class name from `NICE_AU2_VIEW_FACTORY` (default
  `NiceAu2CocoaViewFactory`) and exports it through
  `nice_au2_cocoa_view_factory_class()`.
- `kAudioUnitProperty_CocoaUI` reports that name. The call also references
  the shim's object file from Rust, which is what keeps the class in the link:
  nothing else refers to it, and a linker drops archive members nothing needs.
- The `external-view-factory` feature stops `build.rs` compiling the shim, and
  `view_factory.rs` is a build-script helper that compiles it into the plugin
  crate instead, named `NiceAu2CocoaViewFactory_<package>`. Each Comp76Fx
  revision crate's `build.rs` includes it. A single crate compiled once cannot
  do this itself: the three plugins link the same compiled copy of it.

## The editor view class is registered under a unique name

`EditorView`, the NSView the plugin editor is embedded in, was declared with
`define_class!` as `NiceAu2EditorView`. objc2 panics registering a class whose
name is already taken, so the second plugin to open its editor in a process
aborted the host. It is now registered at runtime with `ClassBuilder`, the
way baseview registers its own view class, under a name carrying the address
of a static -- distinct in every loaded image. Its methods, ivar handling and
lifecycle are otherwise unchanged.

## Verification

None of this compiles off macOS in full: coreaudio-sys needs the macOS SDK.
What was checked on Linux:

- `src/bridge/editor.rs` and `src/bridge/view.rs` type-check and pass clippy
  for `aarch64-apple-darwin` and `x86_64-apple-darwin` against objc2 0.6.4 and
  objc2-app-kit 0.3.2, with the bridge functions stubbed at their signatures.
- `shim.m` preprocesses and passes `clang -fsyntax-only -fobjc-arc` for
  arm64-apple-macosx against stub AppKit/AudioUnit headers, under both the
  default and a per-plugin class name.
- The revision crates' `Au2Plugin` impls and `nice_export_au2!` expansions
  type-check for `aarch64-apple-darwin` against this crate's trait, config and
  export macro.

The macOS CI job builds the components, checks their symbols and runs auval.
Loading two of them into one host and opening both editors still needs doing
on a Mac.
