//! The NSView an Audio Unit's editor is embedded in.
//!
//! The class is registered at runtime under a name unique to the image it is
//! registered from, rather than declared once under a fixed name with
//! `define_class!`. A host loads every Audio Unit into one process, and
//! Objective-C has one class namespace per process: each plugin built with
//! this crate carries its own copy of this code, and registering a second
//! class under a name already taken panics inside the host's call -- the
//! second plugin's editor crashed the host as it opened.

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Ivar, Sel};
use objc2::{ClassType, MainThreadMarker, msg_send, sel};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSColor, NSRectFill, NSView};
use objc2_foundation::{NSRect, NSSize};

use crate::bridge::{
    nice_au2_destroy_editor, nice_au2_flush_editor_notifications, nice_au2_get_editor_size,
    nice_au2_spawn_editor,
};

type CVDisplayLinkRef = *mut c_void;
type CVReturn = i32;
type CVOptionFlags = u64;

type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    CVDisplayLinkRef,
    *const c_void,
    *const c_void,
    CVOptionFlags,
    *mut CVOptionFlags,
    *mut c_void,
) -> CVReturn;

unsafe extern "C" {
    fn CVDisplayLinkCreateWithActiveCGDisplays(link: *mut CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkSetOutputCallback(
        link: CVDisplayLinkRef,
        callback: Option<CVDisplayLinkOutputCallback>,
        context: *mut c_void,
    ) -> CVReturn;
    fn CVDisplayLinkStart(link: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkStop(link: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkRelease(link: CVDisplayLinkRef);
}

/// The instance variable holding a view's boxed [`EditorState`].
const STATE_IVAR: &CStr = c"nice_au2_editor_state";

#[derive(Default)]
struct EditorState {
    rust_instance: Cell<*mut c_void>,
    editor_handle: Cell<*mut c_void>,
    display_link: Cell<CVDisplayLinkRef>,
    display_link_context: Cell<*mut DisplayLinkContext>,
}

struct DisplayLinkContext {
    view: *mut NSView,
    refresh_queued: AtomicBool,
}

/// This image's editor view class, registered on first use, and its state
/// variable.
fn editor_view_class() -> (&'static AnyClass, &'static Ivar) {
    static CLASS: OnceLock<(&'static AnyClass, &'static Ivar)> = OnceLock::new();
    *CLASS.get_or_init(|| {
        // The address of this static differs in every loaded image, which is
        // exactly the uniqueness the name needs.
        let name = format!("NiceAu2EditorView_{:x}", &CLASS as *const _ as usize);
        let name = CString::new(name).expect("a formatted address has no NUL byte");
        let mut class = ClassBuilder::new(&name, NSView::class())
            .expect("the editor view class name is unique to this image");

        // SAFETY: Every signature matches its selector.
        unsafe {
            class.add_method(
                sel!(isOpaque),
                property_yes as extern "C-unwind" fn(_, _) -> _,
            );
            class.add_method(
                sel!(isFlipped),
                property_yes as extern "C-unwind" fn(_, _) -> _,
            );
            class.add_method(sel!(drawRect:), draw_rect as extern "C-unwind" fn(_, _, _));
            class.add_method(
                sel!(viewDidMoveToWindow),
                view_did_move_to_window as extern "C-unwind" fn(_, _),
            );
            class.add_method(
                sel!(setFrameSize:),
                set_frame_size as extern "C-unwind" fn(_, _, _),
            );
            class.add_method(
                sel!(niceAu2RefreshEditorView),
                refresh_editor_view as extern "C-unwind" fn(_, _),
            );
            class.add_method(
                sel!(niceAu2CloseEditorForDestroyedAudioUnit),
                close_for_destroyed_audio_unit as extern "C-unwind" fn(_, _),
            );
            class.add_method(sel!(dealloc), dealloc as extern "C-unwind" fn(_, _));
        }
        class.add_ivar::<*mut c_void>(STATE_IVAR);

        let class = class.register();
        let ivar = class
            .instance_variable(STATE_IVAR)
            .expect("the state variable was added before registering");
        (class, ivar)
    })
}

/// The state of a view of this image's editor class.
fn state(view: &NSView) -> Option<&EditorState> {
    let (_, ivar) = editor_view_class();
    // SAFETY: Only called with views of the editor class, which has the ivar.
    let raw = unsafe { *ivar.load::<*mut c_void>(view) };
    unsafe { raw.cast::<EditorState>().as_ref() }
}

/// A new editor view for an Audio Unit instance.
pub(super) fn new_view(
    _mtm: MainThreadMarker,
    frame: NSRect,
    rust_instance: *mut c_void,
) -> Retained<NSView> {
    let (class, ivar) = editor_view_class();
    let state = Box::new(EditorState {
        rust_instance: Cell::new(rust_instance),
        ..EditorState::default()
    });

    // SAFETY: `alloc` on an NSView subclass returns an allocated NSView.
    let view: Allocated<NSView> = unsafe { msg_send![class, alloc] };
    // Set before `initWithFrame:` so that nothing the initialiser calls can
    // find the view without its state. `dealloc` frees it.
    let target: &AnyObject = unsafe { &*Allocated::as_ptr(&view).cast() };
    unsafe {
        ivar.load_ptr::<*mut c_void>(target)
            .write(Box::into_raw(state).cast())
    };
    let view: Retained<NSView> = unsafe { msg_send![view, initWithFrame: frame] };
    view.setWantsLayer(true);
    view
}

extern "C-unwind" fn property_yes(_this: &NSView, _sel: Sel) -> Bool {
    Bool::YES
}

extern "C-unwind" fn draw_rect(_this: &NSView, _sel: Sel, dirty_rect: NSRect) {
    NSColor::blackColor().setFill();
    NSRectFill(dirty_rect);
}

extern "C-unwind" fn view_did_move_to_window(this: &NSView, _sel: Sel) {
    let _: () = unsafe { msg_send![super(this, NSView::class()), viewDidMoveToWindow] };
    if this.window().is_some() {
        spawn_editor_if_needed(this);
    }
}

extern "C-unwind" fn set_frame_size(this: &NSView, _sel: Sel, requested: NSSize) {
    let instance = state(this).map_or(ptr::null_mut(), |state| state.rust_instance.get());
    let mut width = 0;
    let mut height = 0;
    let size = if !instance.is_null()
        && nice_au2_get_editor_size(instance.cast(), &mut width, &mut height)
        && width > 0
        && height > 0
    {
        NSSize::new(width as f64, height as f64)
    } else {
        requested
    };
    let _: () = unsafe { msg_send![super(this, NSView::class()), setFrameSize: size] };
    layout_embedded_subviews(this);
}

extern "C-unwind" fn refresh_editor_view(this: &NSView, _sel: Sel) {
    let Some(state) = state(this) else {
        return;
    };
    let instance = state.rust_instance.get();
    if !instance.is_null() {
        nice_au2_flush_editor_notifications(instance.cast());
    }
    invalidate_view_hierarchy(this);
    let _: () = unsafe { msg_send![this, layoutSubtreeIfNeeded] };
    let _: () = unsafe { msg_send![this, displayIfNeeded] };
    let context = state.display_link_context.get();
    if !context.is_null() {
        unsafe { &*context }
            .refresh_queued
            .store(false, Ordering::Release);
    }
}

extern "C-unwind" fn close_for_destroyed_audio_unit(this: &NSView, _sel: Sel) {
    let Some(state) = state(this) else {
        return;
    };
    unregister(state.rust_instance.get(), this);
    close_editor(this, state);
    state.rust_instance.set(ptr::null_mut());
}

extern "C-unwind" fn dealloc(this: &mut AnyObject, _sel: Sel) {
    {
        // SAFETY: Instances of the editor class are NSViews.
        let view: &NSView = unsafe { &*(this as *const AnyObject).cast::<NSView>() };
        if let Some(state) = state(view) {
            unregister(state.rust_instance.get(), view);
            close_editor(view, state);
        }
    }
    let (_, ivar) = editor_view_class();
    // SAFETY: The ivar holds either null or the box `new_view` leaked.
    unsafe {
        let slot = ivar.load_ptr::<*mut c_void>(this);
        let raw = slot.read();
        slot.write(ptr::null_mut());
        if !raw.is_null() {
            drop(Box::from_raw(raw.cast::<EditorState>()));
        }
    }
    let _: () = unsafe { msg_send![super(this, NSView::class()), dealloc] };
}

fn spawn_editor_if_needed(this: &NSView) {
    let Some(state) = state(this) else {
        return;
    };
    let instance = state.rust_instance.get();
    if instance.is_null() || !state.editor_handle.get().is_null() {
        return;
    }

    if let Some(previous) = register(instance, this) {
        let _: () = unsafe { msg_send![previous, niceAu2CloseEditorForDestroyedAudioUnit] };
    }
    let handle = nice_au2_spawn_editor(instance.cast(), (this as *const NSView).cast_mut().cast());
    state.editor_handle.set(handle);
    layout_embedded_subviews(this);
    let _: () = unsafe { msg_send![this, niceAu2RefreshEditorView] };
    start_display_link(this, state);
}

fn start_display_link(this: &NSView, state: &EditorState) {
    if !state.display_link.get().is_null() {
        return;
    }
    let mut link = ptr::null_mut();
    if unsafe { CVDisplayLinkCreateWithActiveCGDisplays(&mut link) } != 0 || link.is_null() {
        return;
    }
    let view = unsafe {
        Retained::into_raw(
            Retained::retain((this as *const NSView).cast_mut())
                .expect("an Objective-C method always has a live self"),
        )
    };
    let context = Box::into_raw(Box::new(DisplayLinkContext {
        view,
        refresh_queued: AtomicBool::new(false),
    }));
    if unsafe { CVDisplayLinkSetOutputCallback(link, Some(display_link_callback), context.cast()) }
        != 0
    {
        unsafe {
            let context = Box::from_raw(context);
            drop(Retained::<NSView>::from_raw(context.view).unwrap());
            CVDisplayLinkRelease(link);
        }
        return;
    }
    unsafe { CVDisplayLinkStart(link) };
    state.display_link_context.set(context);
    state.display_link.set(link);
}

fn stop_display_link(state: &EditorState) {
    let link = state.display_link.replace(ptr::null_mut());
    if link.is_null() {
        return;
    }
    unsafe {
        CVDisplayLinkStop(link);
        CVDisplayLinkSetOutputCallback(link, None, ptr::null_mut());
        CVDisplayLinkRelease(link);
        let context = state.display_link_context.replace(ptr::null_mut());
        if !context.is_null() {
            let context = Box::from_raw(context);
            drop(Retained::<NSView>::from_raw(context.view).unwrap());
        }
    }
}

fn close_editor(this: &NSView, state: &EditorState) {
    stop_display_link(state);
    let handle = state.editor_handle.replace(ptr::null_mut());
    let instance = state.rust_instance.get();
    if !handle.is_null() && !instance.is_null() {
        nice_au2_destroy_editor(instance.cast(), handle);
    }
    let subviews: Retained<objc2_foundation::NSArray<NSView>> =
        unsafe { msg_send![this, subviews] };
    for subview in subviews.iter() {
        subview.removeFromSuperview();
    }
}

fn layout_embedded_subviews(this: &NSView) {
    let bounds = this.bounds();
    let subviews: Retained<objc2_foundation::NSArray<NSView>> =
        unsafe { msg_send![this, subviews] };
    for subview in subviews.iter() {
        subview.setFrame(bounds);
        subview.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        subview.setNeedsDisplay(true);
    }
}

unsafe extern "C" fn display_link_callback(
    _link: CVDisplayLinkRef,
    _now: *const c_void,
    _output: *const c_void,
    _input_flags: CVOptionFlags,
    _output_flags: *mut CVOptionFlags,
    context: *mut c_void,
) -> CVReturn {
    let context = unsafe { &*context.cast::<DisplayLinkContext>() };
    if !context.refresh_queued.swap(true, Ordering::AcqRel) {
        let _: () = unsafe {
            msg_send![context.view,
                performSelectorOnMainThread: sel!(niceAu2RefreshEditorView),
                withObject: ptr::null::<AnyObject>(),
                waitUntilDone: Bool::NO
            ]
        };
    }
    0
}

fn invalidate_view_hierarchy(view: &NSView) {
    view.setNeedsDisplay(true);
    let subviews: Retained<objc2_foundation::NSArray<NSView>> =
        unsafe { msg_send![view, subviews] };
    for subview in subviews.iter() {
        invalidate_view_hierarchy(&subview);
    }
}

fn active_views() -> &'static Mutex<HashMap<usize, usize>> {
    static VIEWS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    VIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register(instance: *mut c_void, view: &NSView) -> Option<*mut NSView> {
    active_views()
        .lock()
        .ok()?
        .insert(instance as usize, view as *const _ as usize)
        .map(|p| p as *mut _)
}

fn unregister(instance: *mut c_void, view: &NSView) {
    if let Ok(mut views) = active_views().lock()
        && views.get(&(instance as usize)).copied() == Some(view as *const _ as usize)
    {
        views.remove(&(instance as usize));
    }
}

pub(super) fn close_for_rust_instance(instance: *mut c_void) {
    let view = active_views()
        .lock()
        .ok()
        .and_then(|views| views.get(&(instance as usize)).copied());
    if let Some(view) = view {
        let view = view as *mut NSView;
        let _: () = unsafe {
            msg_send![view,
                performSelectorOnMainThread: sel!(niceAu2CloseEditorForDestroyedAudioUnit),
                withObject: ptr::null::<AnyObject>(),
                waitUntilDone: Bool::YES
            ]
        };
    }
}
