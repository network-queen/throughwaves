//! VST3 plugin editor windows — opens the plugin's native GUI in a separate macOS window.

use std::collections::HashMap;
use uuid::Uuid;

/// Manages open plugin editor windows.
pub struct PluginWindowManager {
    pub windows: HashMap<Uuid, PluginEditorWindow>,
}

impl Default for PluginWindowManager {
    fn default() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }
}

impl PluginWindowManager {
    /// Open an editor window for a plugin. Takes ownership of the Vst3Plugin
    /// to keep the library loaded while the editor is open.
    pub fn open(&mut self, slot_id: Uuid, plugin: jamhub_engine::vst3_host::Vst3Plugin) -> bool {
        if let Some(win) = self.windows.get(&slot_id) {
            // Already exists (possibly hidden) — show and bring to front
            if !win.is_open() {
                win.bring_to_front(); // unhide
            } else {
                win.bring_to_front();
            }
            return true;
        }

        let view = match plugin.create_editor_view() {
            Some(v) => v,
            None => return false,
        };

        let (w, h) = jamhub_engine::vst3_host::Vst3Plugin::get_editor_size(view);
        let title = format!("{} — ThroughWaves", plugin.name);

        match PluginEditorWindow::new(view, w, h, &title, plugin) {
            Some(win) => {
                self.windows.insert(slot_id, win);
                true
            }
            None => {
                // View will be cleaned up by the plugin's Drop
                false
            }
        }
    }

    /// Hide an editor window (doesn't destroy it — avoids JUCE animation crashes).
    pub fn close(&mut self, slot_id: &Uuid) {
        if let Some(win) = self.windows.get(slot_id) {
            win.hide();
        }
    }

    /// Hide a window when removing the effect from the chain.
    ///
    /// INTENTIONAL MEMORY LEAK: The window and its Vst3Plugin are kept alive
    /// (hidden) rather than destroyed. Dropping Vst3Plugin on the main thread
    /// deadlocks JUCE-based plugins because their internal threads need the
    /// run loop. The window stays hidden and the plugin stays loaded until
    /// the application exits, at which point the OS reclaims all resources.
    pub fn destroy(&mut self, slot_id: &Uuid) {
        if let Some(win) = self.windows.get(slot_id) {
            win.hide();
        }
        // Don't remove from map — dropping Vst3Plugin on main thread deadlocks JUCE.
        // The window stays hidden and the plugin stays loaded until app exit.
    }

    /// Check if an editor is currently visible for this slot.
    pub fn is_open(&self, slot_id: &Uuid) -> bool {
        self.windows.get(slot_id).map_or(false, |w| w.is_open())
    }

    /// Close all windows.
    pub fn close_all(&mut self) {
        self.windows.clear();
    }

    /// Remove windows that have been closed by the user clicking the X button.
    /// Hidden windows (closed via our close() method) are kept alive.
    pub fn cleanup_closed(&mut self) {
        // Don't cleanup — windows are hidden, not destroyed.
        // They'll be destroyed when the effect is removed from the chain.
    }
}

/// A native macOS window hosting a VST3 plugin editor.
pub struct PluginEditorWindow {
    /// Keep the plugin instance alive so the dynamic library stays loaded
    _plugin: jamhub_engine::vst3_host::Vst3Plugin,
    #[cfg(target_os = "macos")]
    inner: Option<MacOsPluginWindow>,
    #[cfg(not(target_os = "macos"))]
    _phantom: std::marker::PhantomData<()>,
}

impl PluginEditorWindow {
    pub fn new(
        view: *mut vst3::Steinberg::IPlugView,
        width: i32,
        height: i32,
        title: &str,
        plugin: jamhub_engine::vst3_host::Vst3Plugin,
    ) -> Option<Self> {
        #[cfg(target_os = "macos")]
        {
            let inner = MacOsPluginWindow::new(view, width, height, title)?;
            Some(Self {
                _plugin: plugin,
                inner: Some(inner),
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, width, height, title, plugin);
            eprintln!("Plugin editor windows are only supported on macOS");
            None
        }
    }

    pub fn bring_to_front(&self) {
        #[cfg(target_os = "macos")]
        if let Some(ref inner) = self.inner {
            inner.bring_to_front();
        }
    }

    pub fn hide(&self) {
        #[cfg(target_os = "macos")]
        if let Some(ref inner) = self.inner {
            inner.hide();
        }
    }

    pub fn is_open(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.inner.as_ref().map_or(false, |w| w.is_open())
        }
        #[cfg(not(target_os = "macos"))]
        false
    }
}

// ---- macOS implementation ----
#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2::MainThreadOnly;
    use objc2_app_kit::{NSView, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
    use std::ffi::c_void;

    /// A native macOS window hosting a VST3 plugin editor view.
    ///
    /// INTENTIONAL MEMORY LEAK: The IPlugView pointer (`_plug_view`) is stored here
    /// to prevent its COM ref-count from being released while the window exists.
    /// On drop, we only hide the window — we do NOT call `removed()` or `release()`
    /// on the IPlugView because JUCE-based plugins crash during their internal
    /// `_NSWindowTransformAnimation` teardown. The OS reclaims all resources on
    /// process exit. This is the standard workaround for JUCE VST3 hosts.
    pub struct MacOsPluginWindow {
        window: Retained<NSWindow>,
        _view: Retained<NSView>,
        /// Kept alive to prevent use-after-free — intentionally never released (see doc above).
        _plug_view: *mut vst3::Steinberg::IPlugView,
    }

    unsafe impl Send for MacOsPluginWindow {}

    impl MacOsPluginWindow {
        pub fn new(
            plug_view: *mut vst3::Steinberg::IPlugView,
            width: i32,
            height: i32,
            title: &str,
        ) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;

            unsafe {
                let frame = NSRect::new(
                    NSPoint::new(200.0, 200.0),
                    NSSize::new(width as f64, height as f64),
                );

                // No Closable — prevent user from clicking X which triggers
                // _NSWindowTransformAnimation crash with JUCE plugins.
                // Users toggle visibility via the FX chain instead.
                let style = NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Miniaturizable;

                let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame,
                    style,
                    objc2_app_kit::NSBackingStoreType(2),
                    false,
                );

                let ns_title = NSString::from_str(title);
                window.setTitle(&ns_title);

                let content_rect = NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(width as f64, height as f64),
                );
                let view = NSView::initWithFrame(NSView::alloc(mtm), content_rect);
                window.setContentView(Some(&view));

                let ns_view_ptr = &*view as *const NSView as *mut c_void;
                let platform_type = b"NSView\0";
                let result = ((*(*plug_view).vtbl).attached)(
                    plug_view,
                    ns_view_ptr,
                    platform_type.as_ptr() as *const i8,
                );

                if result != 0 {
                    eprintln!("VST3 editor: attached() failed with code {result}");
                    let view_unknown = plug_view as *mut vst3::Steinberg::FUnknown;
                    ((*(*view_unknown).vtbl).release)(view_unknown);
                    return None;
                }

                window.makeKeyAndOrderFront(None);
                window.center();

                let _ = (width, height); // editor window opened

                Some(Self {
                    window,
                    _view: view,
                    _plug_view: plug_view,
                })
            }
        }

        pub fn bring_to_front(&self) {
            self.window.makeKeyAndOrderFront(None);
        }

        pub fn hide(&self) {
            self.window.orderOut(None);
        }

        pub fn is_open(&self) -> bool {
            self.window.isVisible()
        }
    }

    impl Drop for MacOsPluginWindow {
        fn drop(&mut self) {
            // Intentionally leak the NSWindow and IPlugView to avoid
            // JUCE _NSWindowTransformAnimation crash. The window is hidden
            // via orderOut() before drop, and the OS will clean up on exit.
            // This is the standard workaround for JUCE-based VST3 plugins.
            self.window.orderOut(None);
        }
    }
}

#[cfg(target_os = "macos")]
use macos::MacOsPluginWindow;
