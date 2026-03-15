use std::path::{Path, PathBuf};

/// Represents a loaded VST plugin instance.
/// Currently supports loading the dynamic library and querying basic info.
/// Full audio processing requires implementing the VST3 COM interfaces,
/// which is a significant undertaking. This provides the foundation.
pub struct VstInstance {
    pub name: String,
    pub path: PathBuf,
    pub loaded: bool,
    pub error: Option<String>,
    _lib: Option<libloading::Library>,
}

impl VstInstance {
    /// Attempt to load a VST plugin from a path.
    /// For VST3: loads the bundle's dynamic library.
    /// For VST2: loads the .vst/.dll directly.
    pub fn load(path: &Path) -> Self {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".into());

        // Find the actual binary inside the bundle
        let lib_path = find_library_in_bundle(path);

        match lib_path {
            Some(lp) => {
                match unsafe { libloading::Library::new(&lp) } {
                    Ok(lib) => {
                        println!("VST loaded: {} from {}", name, lp.display());

                        // Try to find the VST3 entry point
                        let has_vst3_entry = unsafe {
                            lib.get::<unsafe extern "C" fn() -> *mut std::ffi::c_void>(
                                b"GetPluginFactory",
                            )
                            .is_ok()
                        };

                        // Try to find VST2 entry point
                        let has_vst2_entry = unsafe {
                            lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void>(
                                b"VSTPluginMain",
                            )
                            .is_ok()
                            || lib.get::<unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void>(
                                b"main_plugin",
                            )
                            .is_ok()
                        };

                        let plugin_type = if has_vst3_entry {
                            "VST3"
                        } else if has_vst2_entry {
                            "VST2"
                        } else {
                            "Unknown format"
                        };

                        println!("  Plugin type: {plugin_type}");

                        VstInstance {
                            name,
                            path: path.to_path_buf(),
                            loaded: true,
                            error: None,
                            _lib: Some(lib),
                        }
                    }
                    Err(e) => VstInstance {
                        name,
                        path: path.to_path_buf(),
                        loaded: false,
                        error: Some(format!("Failed to load library: {e}")),
                        _lib: None,
                    },
                }
            }
            None => VstInstance {
                name,
                path: path.to_path_buf(),
                loaded: false,
                error: Some("Could not find plugin binary in bundle".into()),
                _lib: None,
            },
        }
    }

    pub fn unload(&mut self) {
        self._lib = None;
        self.loaded = false;
    }
}

/// Find the actual dynamic library inside a plugin bundle.
fn find_library_in_bundle(path: &Path) -> Option<PathBuf> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    match ext {
        "vst3" => {
            // VST3 bundle: Contents/MacOS/<name> or Contents/x86_64-linux/<name>.so
            #[cfg(target_os = "macos")]
            {
                let binary = path.join("Contents").join("MacOS").join(name);
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
            None
        }
        "component" => {
            // AU bundle: Contents/MacOS/<name>
            #[cfg(target_os = "macos")]
            {
                let binary = path.join("Contents").join("MacOS").join(name);
                if binary.exists() {
                    return Some(binary);
                }
            }
            None
        }
        "vst" | "dll" | "so" => {
            // Direct library file
            if path.exists() {
                Some(path.to_path_buf())
            } else {
                None
            }
        }
        _ => None,
    }
}
