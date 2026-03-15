//! VST3 plugin hosting — load, instantiate, and process audio through VST3 plugins.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

/// A hosted VST3 plugin instance ready for audio processing.
pub struct Vst3Plugin {
    pub name: String,
    pub path: PathBuf,
    pub loaded: bool,
    pub processing: bool,
    pub error: Option<String>,
    pub num_params: i32,
    _lib: Option<libloading::Library>,
    component: Option<*mut IComponent>,
    processor: Option<*mut IAudioProcessor>,
    sample_rate: f64,
    block_size: i32,
}

unsafe impl Send for Vst3Plugin {}

impl Vst3Plugin {
    pub fn load(path: &Path, sample_rate: f64, block_size: i32) -> Self {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".into());

        let lib_path = match find_vst3_binary(path) {
            Some(p) => p,
            None => return Self::failed(name, path, "Could not find plugin binary"),
        };

        let lib = match unsafe { libloading::Library::new(&lib_path) } {
            Ok(l) => l,
            Err(e) => return Self::failed(name, path, &format!("Load failed: {e}")),
        };

        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> =
            match unsafe { lib.get(b"GetPluginFactory") } {
                Ok(f) => f,
                Err(_) => return Self::failed(name, path, "Not a VST3 plugin"),
            };

        let factory_raw = unsafe { get_factory() };
        if factory_raw.is_null() {
            return Self::failed(name, path, "Null factory");
        }

        let factory = factory_raw as *mut IPluginFactory;
        let class_count = unsafe { ((*(*factory).vtbl).countClasses)(factory) };
        println!("VST3: '{name}' — {class_count} class(es)");

        // Try to create a component instance from each class
        let mut component_raw: *mut c_void = ptr::null_mut();
        let mut found_name = name.clone();

        for i in 0..class_count {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            unsafe { ((*(*factory).vtbl).getClassInfo)(factory, i, &mut info) };

            let cname: String = info.name.iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8 as char)
                .collect();
            if !cname.is_empty() {
                found_name = cname;
            }

            let res = unsafe {
                ((*(*factory).vtbl).createInstance)(
                    factory,
                    info.cid.as_ptr() as *const i8,
                    IComponent_iid.as_ptr() as *const i8,
                    &mut component_raw,
                )
            };

            if res == 0 && !component_raw.is_null() {
                println!("VST3: created '{found_name}'");
                break;
            }
        }

        if component_raw.is_null() {
            return Self::failed(found_name, path, "Failed to create component");
        }

        let component = component_raw as *mut IComponent;

        // Initialize
        unsafe {
            ((*(*component).vtbl).base.initialize)(component as *mut IPluginBase, ptr::null_mut());
        }

        // Get bus info
        let num_in = unsafe {
            ((*(*component).vtbl).getBusCount)(component, MediaTypes_::kAudio as i32, BusDirections_::kInput as i32)
        };
        let num_out = unsafe {
            ((*(*component).vtbl).getBusCount)(component, MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32)
        };

        println!("VST3: '{found_name}' — {num_in} in, {num_out} out");

        // Activate audio buses
        for i in 0..num_in {
            unsafe {
                ((*(*component).vtbl).activateBus)(component, MediaTypes_::kAudio as i32, BusDirections_::kInput as i32, i, 1);
            }
        }
        for i in 0..num_out {
            unsafe {
                ((*(*component).vtbl).activateBus)(component, MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32, i, 1);
            }
        }

        // Set active
        unsafe { ((*(*component).vtbl).setActive)(component, 1); }

        // Query IAudioProcessor — use FUnknown::queryInterface via vtbl
        let mut processor_raw: *mut c_void = ptr::null_mut();
        let qr = unsafe {
            ((*(*component).vtbl).base.base.queryInterface)(
                component as *mut FUnknown,
                &IAudioProcessor_iid as *const _ as *const _,
                &mut processor_raw,
            )
        };

        if qr != 0 || processor_raw.is_null() {
            return Self {
                name: found_name,
                path: path.to_path_buf(),
                loaded: true,
                processing: false,
                error: Some("No audio processor interface".into()),
                num_params: 0,
                _lib: Some(lib),
                component: Some(component),
                processor: None,
                sample_rate,
                block_size,
            };
        }

        let processor = processor_raw as *mut IAudioProcessor;

        // Setup processing
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: block_size,
            sampleRate: sample_rate,
        };
        unsafe { ((*(*processor).vtbl).setupProcessing)(processor, &mut setup); }
        unsafe { ((*(*processor).vtbl).setProcessing)(processor, 1); }

        println!("VST3: '{found_name}' — processing active!");

        Self {
            name: found_name,
            path: path.to_path_buf(),
            loaded: true,
            processing: true,
            error: None,
            num_params: 0,
            _lib: Some(lib),
            component: Some(component),
            processor: Some(processor),
            sample_rate,
            block_size,
        }
    }

    fn failed(name: String, path: &Path, error: &str) -> Self {
        Self {
            name, path: path.to_path_buf(),
            loaded: false, processing: false,
            error: Some(error.to_string()),
            num_params: 0,
            _lib: None, component: None, processor: None,
            sample_rate: 44100.0, block_size: 256,
        }
    }

    /// Process audio through the plugin.
    pub fn process(&mut self, samples: &mut [f32]) {
        let processor = match self.processor {
            Some(p) if self.processing => p,
            _ => return,
        };

        let n = samples.len() as i32;
        if n == 0 { return; }

        let mut in_ptr = samples.as_mut_ptr();
        let mut out_buf = vec![0.0f32; samples.len()];
        let mut out_ptr = out_buf.as_mut_ptr();

        let mut input_bus = AudioBusBuffers {
            numChannels: 1,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: &mut in_ptr,
            },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: 1,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: &mut out_ptr,
            },
        };

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: n,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_bus,
            outputs: &mut output_bus,
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        let result = unsafe { ((*(*processor).vtbl).process)(processor, &mut data) };
        if result == 0 {
            samples.copy_from_slice(&out_buf);
        }
    }

    pub fn is_loaded(&self) -> bool { self.loaded }
    pub fn is_processing(&self) -> bool { self.processing }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        if let Some(p) = self.processor {
            unsafe { ((*(*p).vtbl).setProcessing)(p, 0); }
        }
        if let Some(c) = self.component {
            unsafe {
                ((*(*c).vtbl).setActive)(c, 0);
                ((*(*c).vtbl).base.terminate)(c as *mut IPluginBase);
            }
        }
    }
}

fn find_vst3_binary(path: &Path) -> Option<PathBuf> {
    let name = path.file_stem().and_then(|s| s.to_str())?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "vst3" {
        return if path.is_file() { Some(path.to_path_buf()) } else { None };
    }
    #[cfg(target_os = "macos")]
    {
        let b = path.join("Contents").join("MacOS").join(name);
        if b.exists() { return Some(b); }
    }
    #[cfg(target_os = "windows")]
    {
        let b = path.join("Contents").join("x86_64-win").join(format!("{name}.vst3"));
        if b.exists() { return Some(b); }
    }
    #[cfg(target_os = "linux")]
    {
        let b = path.join("Contents").join("x86_64-linux").join(format!("{name}.so"));
        if b.exists() { return Some(b); }
    }
    None
}
