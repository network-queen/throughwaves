//! VST3 plugin hosting — load, instantiate, and process audio through VST3 plugins.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};

use crossbeam_channel::{bounded, Sender, Receiver};
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

// --- Parameter change forwarding ---
/// A parameter change from the editor UI to the audio processor.
pub struct ParamChange {
    pub id: u32,
    pub value: f64,
}

/// Receiver for parameter changes — polled by the audio engine.
pub type ParamChangeRx = Receiver<ParamChange>;

// --- IComponentHandler implementation ---
// Captures parameter edits from the plugin UI and forwards them via a channel.

#[repr(C)]
struct HostComponentHandler {
    vtbl: *const IComponentHandlerVtbl,
    ref_count: AtomicI32,
    param_tx: Sender<ParamChange>,
}

unsafe extern "system" fn handler_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid_slice = unsafe { &*iid };
    if *iid_slice == IComponentHandler_iid || *iid_slice == FUnknown_iid {
        handler_add_ref(this);
        *obj = this as *mut c_void;
        0
    } else {
        *obj = ptr::null_mut();
        -1
    }
}

unsafe extern "system" fn handler_add_ref(this: *mut FUnknown) -> u32 {
    let handler = this as *mut HostComponentHandler;
    (*handler).ref_count.fetch_add(1, Ordering::SeqCst) as u32 + 1
}

unsafe extern "system" fn handler_release(this: *mut FUnknown) -> u32 {
    let handler = this as *mut HostComponentHandler;
    let prev = (*handler).ref_count.fetch_sub(1, Ordering::SeqCst);
    if prev <= 1 {
        drop(Box::from_raw(handler));
        0
    } else {
        (prev - 1) as u32
    }
}

unsafe extern "system" fn handler_begin_edit(
    _this: *mut IComponentHandler, _id: ParamID,
) -> tresult { 0 }

unsafe extern "system" fn handler_perform_edit(
    this: *mut IComponentHandler, id: ParamID, value: ParamValue,
) -> tresult {
    let handler = this as *mut HostComponentHandler;
    let _ = (*handler).param_tx.try_send(ParamChange { id, value });
    0
}

unsafe extern "system" fn handler_end_edit(
    _this: *mut IComponentHandler, _id: ParamID,
) -> tresult { 0 }

unsafe extern "system" fn handler_restart_component(
    _this: *mut IComponentHandler, _flags: int32,
) -> tresult { 0 }

static HOST_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
    base: FUnknownVtbl {
        queryInterface: handler_query_interface,
        addRef: handler_add_ref,
        release: handler_release,
    },
    beginEdit: handler_begin_edit,
    performEdit: handler_perform_edit,
    endEdit: handler_end_edit,
    restartComponent: handler_restart_component,
};

/// Create a component handler that forwards parameter changes to the returned receiver.
fn create_component_handler() -> (*mut IComponentHandler, ParamChangeRx) {
    let (tx, rx) = bounded::<ParamChange>(256);
    let handler = Box::new(HostComponentHandler {
        vtbl: &HOST_HANDLER_VTBL,
        ref_count: AtomicI32::new(1),
        param_tx: tx,
    });
    (Box::into_raw(handler) as *mut IComponentHandler, rx)
}

// --- End IComponentHandler ---

/// A hosted VST3 plugin instance ready for audio processing.
pub struct Vst3Plugin {
    pub name: String,
    pub path: PathBuf,
    pub loaded: bool,
    pub processing: bool,
    pub error: Option<String>,
    pub num_params: i32,
    pub has_editor: bool,
    /// Plugin-reported latency in samples (from IAudioProcessor::getLatencySamples).
    pub latency_samples: u32,
    _lib: Option<libloading::Library>,
    component: Option<*mut IComponent>,
    processor: Option<*mut IAudioProcessor>,
    controller: Option<*mut IEditController>,
    sample_rate: f64,
    block_size: i32,
    /// Receives parameter changes from the editor UI's IComponentHandler
    pub param_change_rx: Option<ParamChangeRx>,
}

unsafe impl Send for Vst3Plugin {}

/// Info about a VST3 parameter.
pub struct Vst3ParamInfo {
    pub id: u32,
    pub name: String,
    pub default_value: f64,
    pub step_count: i32,
}

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

        // Query IAudioProcessor
        let mut processor_raw: *mut c_void = ptr::null_mut();
        let qr = unsafe {
            ((*(*component).vtbl).base.base.queryInterface)(
                component as *mut FUnknown,
                &IAudioProcessor_iid as *const _ as *const _,
                &mut processor_raw,
            )
        };

        let processor = if qr == 0 && !processor_raw.is_null() {
            Some(processor_raw as *mut IAudioProcessor)
        } else {
            None
        };

        // Setup processing if we got a processor
        let mut latency_samples: u32 = 0;
        let processing = if let Some(proc) = processor {
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: block_size,
                sampleRate: sample_rate,
            };
            unsafe { ((*(*proc).vtbl).setupProcessing)(proc, &mut setup); }
            unsafe { ((*(*proc).vtbl).setProcessing)(proc, 1); }

            // Query plugin-reported latency
            latency_samples = unsafe { ((*(*proc).vtbl).getLatencySamples)(proc) };
            if latency_samples > 0 {
                println!("VST3: '{found_name}' — latency: {latency_samples} samples ({:.1}ms)",
                    latency_samples as f64 / sample_rate * 1000.0);
            }

            println!("VST3: '{found_name}' — processing active!");
            true
        } else {
            false
        };

        // Query IEditController — first try queryInterface (combined component)
        let mut controller_raw: *mut c_void = ptr::null_mut();
        let ec_result = unsafe {
            ((*(*component).vtbl).base.base.queryInterface)(
                component as *mut FUnknown,
                &IEditController_iid as *const _ as *const _,
                &mut controller_raw,
            )
        };

        let mut param_change_rx = None;
        let controller = if ec_result == 0 && !controller_raw.is_null() {
            let ctrl = controller_raw as *mut IEditController;
            let (handler, rx) = create_component_handler();
            param_change_rx = Some(rx);
            unsafe {
                ((*(*ctrl).vtbl).setComponentHandler)(ctrl, handler);
            }
            println!("VST3: '{found_name}' — edit controller (combined, handler set)");
            Some(ctrl)
        } else {
            // Separate controller: get the controller class ID from the component,
            // then create it from the factory
            let mut ctrl_cid: TUID = [0i8; 16];
            let cid_result = unsafe {
                ((*(*component).vtbl).getControllerClassId)(component, &mut ctrl_cid)
            };
            if cid_result == 0 {
                let cid_hex: String = ctrl_cid.iter()
                    .map(|b| format!("{:02X}", *b as u8))
                    .collect::<Vec<_>>().join("");
                println!("VST3: controller class ID = {cid_hex}");
                let mut ctrl_raw: *mut c_void = ptr::null_mut();
                let create_result = unsafe {
                    ((*(*factory).vtbl).createInstance)(
                        factory,
                        ctrl_cid.as_ptr(),
                        IEditController_iid.as_ptr() as *const i8,
                        &mut ctrl_raw,
                    )
                };
                if create_result == 0 && !ctrl_raw.is_null() {
                    let ctrl = ctrl_raw as *mut IEditController;
                    // Initialize the controller
                    unsafe {
                        ((*(*ctrl).vtbl).base.initialize)(ctrl as *mut IPluginBase, ptr::null_mut());
                    }
                    // Set component handler — required for many plugins to create their editor
                    let (handler, rx) = create_component_handler();
                    param_change_rx = Some(rx);
                    unsafe {
                        ((*(*ctrl).vtbl).setComponentHandler)(ctrl, handler);
                    }

                    // Connect component and controller via IConnectionPoint
                    let mut comp_cp_raw: *mut c_void = ptr::null_mut();
                    let mut ctrl_cp_raw: *mut c_void = ptr::null_mut();
                    unsafe {
                        let _ = ((*(*component).vtbl).base.base.queryInterface)(
                            component as *mut FUnknown,
                            &IConnectionPoint_iid as *const _ as *const _,
                            &mut comp_cp_raw,
                        );
                        let _ = ((*(*ctrl).vtbl).base.base.queryInterface)(
                            ctrl as *mut FUnknown,
                            &IConnectionPoint_iid as *const _ as *const _,
                            &mut ctrl_cp_raw,
                        );
                        if !comp_cp_raw.is_null() && !ctrl_cp_raw.is_null() {
                            let comp_cp = comp_cp_raw as *mut IConnectionPoint;
                            let ctrl_cp = ctrl_cp_raw as *mut IConnectionPoint;
                            ((*(*comp_cp).vtbl).connect)(comp_cp, ctrl_cp);
                            ((*(*ctrl_cp).vtbl).connect)(ctrl_cp, comp_cp);
                            println!("VST3: '{found_name}' — component <-> controller connected");
                        }
                    }

                    println!("VST3: '{found_name}' — edit controller (separate, handler set)");
                    Some(ctrl)
                } else {
                    println!("VST3: '{found_name}' — failed to create separate controller");
                    None
                }
            } else {
                println!("VST3: '{found_name}' — no controller class ID");
                None
            }
        };

        let num_params = controller.map_or(0, |c| unsafe {
            ((*(*c).vtbl).getParameterCount)(c)
        });

        // If we have a controller, assume editor is available
        // (createView will be called when user actually opens the UI)
        let has_editor = controller.is_some();
        if has_editor {
            println!("VST3: '{found_name}' — editor UI likely available (has controller)");
        }

        Self {
            name: found_name,
            path: path.to_path_buf(),
            loaded: true,
            processing,
            error: if processor.is_none() { Some("No audio processor interface".into()) } else { None },
            num_params,
            has_editor,
            latency_samples,
            _lib: Some(lib),
            component: Some(component),
            processor,
            controller,
            sample_rate,
            block_size,
            param_change_rx,
        }
    }

    fn failed(name: String, path: &Path, error: &str) -> Self {
        Self {
            name, path: path.to_path_buf(),
            loaded: false, processing: false,
            error: Some(error.to_string()),
            num_params: 0, has_editor: false,
            latency_samples: 0,
            _lib: None, component: None, processor: None, controller: None,
            sample_rate: 44100.0, block_size: 256,
            param_change_rx: None,
        }
    }

    /// Apply any pending parameter changes from the editor UI.
    /// Should be called before process() on the audio thread.
    pub fn apply_pending_param_changes(&mut self) {
        let controller = match self.controller {
            Some(c) => c,
            None => return,
        };
        let rx = match &self.param_change_rx {
            Some(rx) => rx,
            None => return,
        };
        while let Ok(change) = rx.try_recv() {
            unsafe {
                ((*(*controller).vtbl).setParamNormalized)(controller, change.id, change.value);
            }
        }
    }

    /// Process audio through the plugin (mono samples).
    /// Sends stereo to the VST3 plugin and mixes output back to mono.
    pub fn process(&mut self, samples: &mut [f32]) {
        let processor = match self.processor {
            Some(p) if self.processing => p,
            _ => return, // passthrough if not ready
        };

        let n = samples.len();
        if n == 0 { return; }

        // Duplicate mono to stereo for input
        let mut in_left = samples.to_vec();
        let mut in_right = samples.to_vec();
        // Separate output buffers
        let mut out_left = vec![0.0f32; n];
        let mut out_right = vec![0.0f32; n];

        let mut in_ptrs = [in_left.as_mut_ptr(), in_right.as_mut_ptr()];
        let mut out_ptrs = [out_left.as_mut_ptr(), out_right.as_mut_ptr()];

        let mut input_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: in_ptrs.as_mut_ptr(),
            },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: out_ptrs.as_mut_ptr(),
            },
        };

        // Provide a valid ProcessContext — some plugins (nih-plug) require it
        let mut process_context: ProcessContext = unsafe { std::mem::zeroed() };
        process_context.sampleRate = self.sample_rate;
        process_context.tempo = 120.0;
        process_context.timeSigNumerator = 4;
        process_context.timeSigDenominator = 4;
        // Set flags indicating which fields are valid
        process_context.state = ProcessContext_::StatesAndFlags_::kTempoValid as u32
            | ProcessContext_::StatesAndFlags_::kTimeSigValid as u32
            | ProcessContext_::StatesAndFlags_::kPlaying as u32;

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: n as i32,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_bus,
            outputs: &mut output_bus,
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: &mut process_context,
        };

        let result = unsafe { ((*(*processor).vtbl).process)(processor, &mut data) };

        if result == 0 {
            // Check if output has any signal
            let has_output = out_left.iter().any(|s| *s != 0.0) || out_right.iter().any(|s| *s != 0.0);
            if has_output {
                // Mix stereo output back to mono
                for i in 0..n {
                    samples[i] = (out_left[i] + out_right[i]) * 0.5;
                }
            }
            // If output is all zeros, keep original samples (passthrough)
            // This handles plugins that don't produce output until they have enough data
        }
        // If result != 0, keep original samples (passthrough)
    }

    /// Get parameter count.
    pub fn get_parameter_count(&self) -> i32 {
        self.num_params
    }

    /// Get info about a parameter.
    pub fn get_parameter_info(&self, index: i32) -> Option<Vst3ParamInfo> {
        let controller = self.controller?;
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        let res = unsafe {
            ((*(*controller).vtbl).getParameterInfo)(controller, index, &mut info)
        };
        if res != 0 { return None; }

        let name: String = info.title.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| char::from(c as u8))
            .collect();

        Some(Vst3ParamInfo {
            id: info.id,
            name,
            default_value: info.defaultNormalizedValue,
            step_count: info.stepCount,
        })
    }

    /// Get normalized parameter value (0.0 to 1.0).
    pub fn get_param_normalized(&self, id: u32) -> f64 {
        match self.controller {
            Some(c) => unsafe { ((*(*c).vtbl).getParamNormalized)(c, id) },
            None => 0.0,
        }
    }

    /// Set normalized parameter value (0.0 to 1.0).
    pub fn set_param_normalized(&self, id: u32, value: f64) {
        if let Some(c) = self.controller {
            unsafe { ((*(*c).vtbl).setParamNormalized)(c, id, value); }
        }
    }

    /// Create the plugin's editor view. Returns a raw IPlugView pointer.
    /// The caller is responsible for attaching it to a native window and releasing it.
    /// Returns None if the plugin doesn't have an editor.
    pub fn create_editor_view(&self) -> Option<*mut IPlugView> {
        let controller = self.controller?;

        // Try both "editor" (standard) view types
        let view_types: &[&[u8]] = &[b"editor\0"];

        for vt in view_types {
            println!("VST3: trying createView({:?}) for '{}'...",
                std::str::from_utf8(&vt[..vt.len()-1]).unwrap_or("?"), self.name);
            let view = unsafe {
                ((*(*controller).vtbl).createView)(controller, vt.as_ptr() as *const i8)
            };
            if !view.is_null() {
                // Check if NSView is supported
                let platform = b"NSView\0";
                let supported = unsafe {
                    ((*(*view).vtbl).isPlatformTypeSupported)(view, platform.as_ptr() as *const i8)
                };
                println!("VST3: createView succeeded, NSView supported={}", supported == 0);
                return Some(view);
            }
        }

        // If createView fails, try queryInterface for IPlugView directly on the controller
        let mut view_raw: *mut c_void = ptr::null_mut();
        let qr = unsafe {
            ((*(*controller).vtbl).base.base.queryInterface)(
                controller as *mut FUnknown,
                &IPlugView_iid as *const _ as *const _,
                &mut view_raw,
            )
        };
        if qr == 0 && !view_raw.is_null() {
            println!("VST3: got IPlugView via queryInterface for '{}'", self.name);
            return Some(view_raw as *mut IPlugView);
        }

        println!("VST3: no editor view available for '{}'", self.name);
        None
    }

    /// Get the editor view size.
    pub fn get_editor_size(view: *mut IPlugView) -> (i32, i32) {
        let mut rect: ViewRect = unsafe { std::mem::zeroed() };
        let res = unsafe { ((*(*view).vtbl).getSize)(view, &mut rect) };
        if res == 0 {
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            (800, 600) // default fallback
        }
    }

    pub fn is_loaded(&self) -> bool { self.loaded }
    pub fn is_processing(&self) -> bool { self.processing }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        // Intentionally no-op. JUCE-based plugins deadlock on setActive(0)
        // and terminate() when called from the main thread because their
        // internal threads need the run loop. The dylib stays loaded
        // (macOS doesn't unload dylibs anyway) and resources are freed on exit.
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
