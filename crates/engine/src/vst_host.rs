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
    /// Plugin format: "VST3", "AU", "VST", etc.
    #[serde(default)]
    pub format: String,
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
        for p in &plugins {
            println!("  plugin: {} [{}] vendor='{}' ({})", p.name, p.format, p.vendor, p.path.display());
        }
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
                        let vendor = Self::get_bundle_vendor(&path)
                            .or_else(|| Self::get_vst3_vendor(&path))
                            .unwrap_or_else(|| "Unknown".into());
                        plugins.push(VstPluginInfo {
                            name: name.clone(),
                            path: path.clone(),
                            vendor,
                            category: VstCategory::Effect,
                            is_instrument: false,
                            format: "VST3".into(),
                        });
                    }
                    "component" => {
                        let vendor = Self::get_bundle_vendor(&path)
                            .unwrap_or_else(|| "Unknown".into());
                        plugins.push(VstPluginInfo {
                            name: name.clone(),
                            path: path.clone(),
                            vendor,
                            category: VstCategory::Effect,
                            is_instrument: false,
                            format: "AU".into(),
                        });
                    }
                    _ => {
                        Self::scan_dir(&path, plugins);
                    }
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "vst" || ext == "dll" || ext == "so" {
                    let fmt = match ext {
                        "vst" => "VST",
                        "dll" => "VST",
                        "so" => "VST",
                        _ => "Unknown",
                    };
                    plugins.push(VstPluginInfo {
                        name,
                        path: path.clone(),
                        vendor: "Unknown".into(),
                        category: VstCategory::Effect,
                        is_instrument: false,
                        format: fmt.into(),
                    });
                }
            }
        }
    }

    /// Extract vendor from any macOS plugin bundle (VST3, AU) via Info.plist.
    /// Tries multiple strategies in order of reliability.
    fn get_bundle_vendor(path: &Path) -> Option<String> {
        let plist_path = path.join("Contents").join("Info.plist");
        let content = fs::read_to_string(&plist_path).ok()?;
        let lines: Vec<&str> = content.lines().collect();

        // Strategy 1: AU AudioComponents > name field (format: "Vendor: Plugin Name")
        // This is the most reliable — it's the official AU vendor name
        let mut in_audio_components = false;
        for pair in lines.windows(2) {
            if pair[0].contains("<key>AudioComponents</key>") {
                in_audio_components = true;
            }
            if in_audio_components && pair[0].trim() == "<key>name</key>" {
                let val = pair[1].trim();
                let val = val.strip_prefix("<string>").unwrap_or(val);
                let val = val.strip_suffix("</string>").unwrap_or(val);
                if let Some(colon_pos) = val.find(':') {
                    let vendor = val[..colon_pos].trim();
                    if !vendor.is_empty() {
                        return Some(vendor.to_string());
                    }
                }
            }
        }

        // Strategy 2: NSHumanReadableCopyright — parse "Copyright YYYY Vendor Name"
        for pair in lines.windows(2) {
            if pair[0].contains("NSHumanReadableCopyright") {
                let val = pair[1].trim();
                let val = val.strip_prefix("<string>").unwrap_or(val);
                let val = val.strip_suffix("</string>").unwrap_or(val);
                return Self::parse_vendor_from_copyright(val);
            }
        }

        None
    }

    /// Parse vendor from a copyright string like "Copyright 2025 XLN Audio AB"
    fn parse_vendor_from_copyright(copyright: &str) -> Option<String> {
        let s = copyright.trim();
        // Strip "Copyright" prefix (case insensitive)
        let s = if s.to_lowercase().starts_with("copyright") {
            s[9..].trim_start()
        } else {
            s
        };
        // Strip (c) or ©
        let s = s.strip_prefix("(c)").or_else(|| s.strip_prefix("©")).unwrap_or(s).trim_start();
        // Skip year (4 digits)
        let s = s.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start();
        if s.is_empty() { return None; }
        // Strip common corporate suffixes
        let s = Self::strip_corporate_suffix(s);
        if s.is_empty() { None } else { Some(s.to_string()) }
    }

    /// Remove common corporate suffixes like "AB", "LLC", "Inc", "Ltd", etc.
    fn strip_corporate_suffix(name: &str) -> &str {
        let suffixes = [
            " AB", " LLC", " Inc.", " Inc", " Ltd.", " Ltd",
            " GmbH", " S.A.", " SA", " BV", " B.V.",
            " Pty", " Co.", " Corp.", " Corp",
            " All Rights Reserved", " All rights reserved",
            ", All Rights Reserved",
        ];
        let mut result = name.trim();
        // Strip trailing dot
        result = result.strip_suffix('.').unwrap_or(result).trim();
        for suffix in &suffixes {
            if let Some(stripped) = result.strip_suffix(suffix) {
                result = stripped.trim();
                break;
            }
        }
        result
    }

    /// Extract vendor name from a VST3 bundle by loading its factory info.
    fn get_vst3_vendor(path: &Path) -> Option<String> {
        use std::ffi::c_void;
        use vst3::Steinberg::*;

        let name = path.file_stem().and_then(|s| s.to_str())?;

        // Find the binary
        #[cfg(target_os = "macos")]
        let bin = path.join("Contents").join("MacOS").join(name);
        #[cfg(target_os = "windows")]
        let bin = path.join("Contents").join("x86_64-win").join(format!("{name}.vst3"));
        #[cfg(target_os = "linux")]
        let bin = path.join("Contents").join("x86_64-linux").join(format!("{name}.so"));

        if !bin.exists() {
            return None;
        }

        let lib = unsafe { libloading::Library::new(&bin) }.ok()?;
        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> =
            unsafe { lib.get(b"GetPluginFactory") }.ok()?;

        let factory_raw = unsafe { get_factory() };
        if factory_raw.is_null() {
            return None;
        }

        let factory = factory_raw as *mut IPluginFactory;
        let mut info: PFactoryInfo = unsafe { std::mem::zeroed() };
        let res = unsafe { ((*(*factory).vtbl).getFactoryInfo)(factory, &mut info) };
        if res != 0 {
            return None;
        }

        let vendor: String = info.vendor.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8 as char)
            .collect();

        if vendor.is_empty() { None } else { Some(vendor) }
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
