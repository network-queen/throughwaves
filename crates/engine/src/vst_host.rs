use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

use serde::{Deserialize, Serialize};

/// Metadata about a discovered VST plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VstPluginInfo {
    pub name: String,
    pub path: PathBuf,
    pub vendor: String,
    pub category: VstCategory,
    pub is_instrument: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VstCategory {
    Effect,
    Instrument,
    Analyzer,
    Unknown,
}

/// Scans the system for VST plugins and returns metadata.
pub struct VstScanner;

impl VstScanner {
    /// Scan default VST directories for plugins.
    pub fn scan() -> Vec<VstPluginInfo> {
        let mut plugins = Vec::new();
        let dirs = Self::default_vst_dirs();

        for dir in &dirs {
            if dir.exists() {
                Self::scan_dir(dir, &mut plugins);
            }
        }

        plugins.sort_by(|a, b| a.name.cmp(&b.name));
        println!("VST scan: found {} plugins", plugins.len());
        plugins
    }

    fn default_vst_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        #[cfg(target_os = "macos")]
        {
            // VST3
            dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join("Library/Audio/Plug-Ins/VST3"));
            }
            // VST2
            dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join("Library/Audio/Plug-Ins/VST"));
            }
            // AU
            dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/Components"));
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join("Library/Audio/Plug-Ins/Components"));
            }
        }

        #[cfg(target_os = "windows")]
        {
            dirs.push(PathBuf::from("C:\\Program Files\\Common Files\\VST3"));
            dirs.push(PathBuf::from("C:\\Program Files\\VSTPlugins"));
            dirs.push(PathBuf::from("C:\\Program Files (x86)\\Common Files\\VST3"));
        }

        #[cfg(target_os = "linux")]
        {
            dirs.push(PathBuf::from("/usr/lib/vst3"));
            dirs.push(PathBuf::from("/usr/local/lib/vst3"));
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join(".vst3"));
                dirs.push(home.join(".vst"));
            }
        }

        dirs
    }

    fn scan_dir(dir: &Path, plugins: &mut Vec<VstPluginInfo>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            if path.is_dir() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "vst3" => {
                        plugins.push(VstPluginInfo {
                            name: name.clone(),
                            path: path.clone(),
                            vendor: "Unknown".into(),
                            category: VstCategory::Effect,
                            is_instrument: false,
                        });
                    }
                    "component" => {
                        plugins.push(VstPluginInfo {
                            name: name.clone(),
                            path: path.clone(),
                            vendor: "Unknown".into(),
                            category: VstCategory::Effect,
                            is_instrument: false,
                        });
                    }
                    _ => {
                        // Recurse into subdirectories
                        Self::scan_dir(&path, plugins);
                    }
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "vst" || ext == "dll" || ext == "so" {
                    plugins.push(VstPluginInfo {
                        name,
                        path: path.clone(),
                        vendor: "Unknown".into(),
                        category: VstCategory::Effect,
                        is_instrument: false,
                    });
                }
            }
        }
    }
}

/// Categorize plugins by name heuristics.
pub fn guess_category(name: &str) -> VstCategory {
    let lower = name.to_lowercase();
    if lower.contains("synth") || lower.contains("keys") || lower.contains("piano")
        || lower.contains("sampler") || lower.contains("drum")
    {
        VstCategory::Instrument
    } else if lower.contains("analyzer") || lower.contains("meter")
        || lower.contains("scope") || lower.contains("tuner")
    {
        VstCategory::Analyzer
    } else {
        VstCategory::Effect
    }
}
