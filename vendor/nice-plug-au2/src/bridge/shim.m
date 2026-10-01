// AUv2 hosts discover Cocoa editors through the statically compiled
// Objective-C class metadata for AUCocoaUIBase. A class registered from Rust
// at runtime may pass AU validation but still not be instantiated by a host,
// so this small Objective-C shim remains the host-facing factory.
//
// Objective-C has one class namespace per process, and a host loads every
// Audio Unit into the same one. Two plugins that both compiled this file
// under one name would share a single factory, and the host would build the
// second plugin's editor with the first one's code. So each plugin compiles
// it under a name of its own (`NICE_AU2_VIEW_FACTORY`, set by the build
// script that compiles it) and reports that name through
// `nice_au2_cocoa_view_factory_class`.
#import <AppKit/AppKit.h>
#import <AudioUnit/AUCocoaUIView.h>

#ifndef NICE_AU2_VIEW_FACTORY
#define NICE_AU2_VIEW_FACTORY NiceAu2CocoaViewFactory
#endif

#define NICE_AU2_STRINGIFY_(name) #name
#define NICE_AU2_STRINGIFY(name) NICE_AU2_STRINGIFY_(name)

// Rust owns the editor implementation and returns the actual NSView.
extern NSView* nice_au2_create_cocoa_view(AudioUnit audioUnit);

@interface NICE_AU2_VIEW_FACTORY : NSObject <AUCocoaUIBase>
@end

@implementation NICE_AU2_VIEW_FACTORY

- (unsigned)interfaceVersion {
    return 0;
}

// Keep the Objective-C entry point required by AUv2 hosts, then delegate all
// editor creation and lifecycle work to Rust.
- (NSView*)uiViewForAudioUnit:(AudioUnit)audioUnit withSize:(NSSize)preferredSize {
    (void)preferredSize;
    return nice_au2_create_cocoa_view(audioUnit);
}

@end

// The class name above, for `kAudioUnitProperty_CocoaUI`.
const char* nice_au2_cocoa_view_factory_class(void) {
    return NICE_AU2_STRINGIFY(NICE_AU2_VIEW_FACTORY);
}
