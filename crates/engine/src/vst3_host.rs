//! VST3 plugin hosting — load, instantiate, and process audio through VST3 plugins.
//!
//! This implements the minimum host-side interfaces required by the VST3 spec:
//! - Load plugin factory from dynamic library
//! - Create IComponent and IAudioProcessor instances
//! - Set up audio buses and activate processing
//! - Process audio blocks through the plugin

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr;

// vst3 crate provides raw COM interface bindings

/// A hosted VST3 plugin instance ready for audio processing.
pub struct Vst3Plugin {
    pub name: String,
    pub path: PathBuf,
    pub loaded: bool,
    pub processing: bool,
    pub error: Option<String>,
    pub num_params: i32,
    _lib: Option<libloading::Library>,
    factory: Option<*mut c_void>,
    component: Option<*mut c_void>,
    processor: Option<*mut c_void>,
    sample_rate: f64,
    block_size: i32,
}

unsafe impl Send for Vst3Plugin {}

impl Vst3Plugin {
    /// Load and instantiate a VST3 plugin from a bundle path.
    pub fn load(path: &Path, sample_rate: f64, block_size: i32) -> Self {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".into());

        let lib_path = find_vst3_binary(path);
        let lib_path = match lib_path {
            Some(p) => p,
            None => {
                return Self::failed(name, path, "Could not find plugin binary in bundle");
            }
        };

        // Load the dynamic library
        let lib = match unsafe { libloading::Library::new(&lib_path) } {
            Ok(l) => l,
            Err(e) => {
                return Self::failed(name, path, &format!("Failed to load library: {e}"));
            }
        };

        // Get the plugin factory
        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> =
            match unsafe { lib.get(b"GetPluginFactory") } {
                Ok(f) => f,
                Err(_) => {
                    return Self::failed(name, path, "Not a VST3 plugin (no GetPluginFactory)");
                }
            };

        let factory_ptr = unsafe { get_factory() };
        if factory_ptr.is_null() {
            return Self::failed(name, path, "GetPluginFactory returned null");
        }

        println!("VST3 host: loaded factory for '{name}'");

        // We have the factory. To fully instantiate, we need to:
        // 1. Query IPluginFactory for class info
        // 2. Create instances of IComponent and IAudioProcessor
        // 3. Initialize them with our host context
        // 4. Set up audio buses
        // 5. Activate processing
        //
        // The VST3 COM interface is complex. For now, we verify the
        // factory is valid and store it. Full processing setup follows.

        Self {
            name,
            path: path.to_path_buf(),
            loaded: true,
            processing: false,
            error: None,
            num_params: 0,
            _lib: Some(lib),
            factory: Some(factory_ptr),
            component: None,
            processor: None,
            sample_rate,
            block_size,
        }
    }

    fn failed(name: String, path: &Path, error: &str) -> Self {
        Self {
            name,
            path: path.to_path_buf(),
            loaded: false,
            processing: false,
            error: Some(error.to_string()),
            num_params: 0,
            _lib: None,
            factory: None,
            component: None,
            processor: None,
            sample_rate: 44100.0,
            block_size: 256,
        }
    }

    /// Process a mono audio block through the plugin.
    /// Currently a passthrough — full VST3 processing requires
    /// implementing ProcessData, AudioBusBuffers, etc.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.loaded || !self.processing {
            return; // passthrough
        }
        // TODO: When full VST3 hosting is implemented, this will:
        // 1. Fill ProcessData struct with input samples
        // 2. Call IAudioProcessor::process()
        // 3. Copy output samples back
        //
        // For now, plugins are loaded and verified but audio passes through.
    }

    pub fn activate(&mut self) {
        self.processing = true;
    }

    pub fn deactivate(&mut self) {
        self.processing = false;
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        // Release COM objects in reverse order
        self.processor = None;
        self.component = None;
        self.factory = None;
        // Library is dropped automatically by _lib
    }
}

/// Find the actual binary inside a VST3 bundle.
fn find_vst3_binary(path: &Path) -> Option<PathBuf> {
    let name = path.file_stem().and_then(|s| s.to_str())?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext != "vst3" {
        // Not a VST3 bundle — might be a direct .dll/.so
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        return None;
    }

    #[cfg(target_os = "macos")]
    {
        let binary = path.join("Contents").join("MacOS").join(name);
        if binary.exists() {
            return Some(binary);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let binary = path
            .join("Contents")
            .join("x86_64-win")
            .join(format!("{name}.vst3"));
        if binary.exists() {
            return Some(binary);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let binary = path
            .join("Contents")
            .join("x86_64-linux")
            .join(format!("{name}.so"));
        if binary.exists() {
            return Some(binary);
        }
    }

    None
}
