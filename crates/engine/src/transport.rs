use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use uuid::Uuid;
use jamhub_model::{ClipBufferId, Project, TransportState};

use crate::audio::AudioBackend;
use crate::levels::{peak_level, LevelMeters};
use crate::lufs::{LufsCalculator, LufsMeter};
use crate::metronome::Metronome;
use crate::mixer::Mixer;
use crate::spectrum_buffer::SpectrumBuffer;

pub enum EngineCommand {
    Play,
    Stop,
    SetPosition(u64),
    UpdateProject(Project),
    LoadAudioBuffer { id: ClipBufferId, samples: Vec<f32> },
    SetMetronome(bool),
    SetLoop { enabled: bool, start: u64, end: u64 },
    SetMasterVolume(f32),
    /// Load a VST3 plugin for a specific effect slot
    LoadVst3 { slot_id: Uuid, path: PathBuf },
    /// Unload a VST3 plugin from a specific effect slot
    UnloadVst3 { slot_id: Uuid },
    /// Attach a parameter change receiver to a loaded VST3 plugin (for editor UI sync)
    AttachParamRx { slot_id: Uuid, rx: crate::vst3_host::ParamChangeRx },
    /// Load a VST3 instrument plugin for a MIDI track
    LoadVsti { track_id: Uuid, path: PathBuf },
    /// Unload a VST3 instrument plugin from a MIDI track
    UnloadVsti { track_id: Uuid },
    /// Attach a parameter change receiver to a loaded VSTi plugin
    AttachVstiParamRx { track_id: Uuid, rx: crate::vst3_host::ParamChangeRx },
    /// Reset the integrated LUFS measurement and clipping flag.
    ResetLufs,
}

pub struct EngineHandle {
    cmd_tx: Sender<EngineCommand>,
    pub state: Arc<RwLock<EngineState>>,
    pub levels: LevelMeters,
    pub lufs: LufsMeter,
    pub spectrum: SpectrumBuffer,
    pub pdc_info: crate::mixer::PdcInfo,
    _backend: AudioBackend,
}

pub struct EngineState {
    pub transport: TransportState,
    pub position_samples: u64,
    pub sample_rate: u32,
    /// Set of VST3 effect slot IDs whose plugins have crashed during processing.
    pub crashed_plugins: HashSet<Uuid>,
}

impl EngineHandle {
    pub fn spawn() -> Result<Self, String> {
        let mut backend = AudioBackend::new()?;
        let sample_rate = backend.sample_rate();
        let channels = backend.channels();

        let (cmd_tx, cmd_rx) = bounded::<EngineCommand>(256);
        let (audio_tx, audio_rx) = bounded::<Vec<f32>>(4);

        backend.start(audio_rx)?;

        let state = Arc::new(RwLock::new(EngineState {
            transport: TransportState::Stopped,
            position_samples: 0,
            sample_rate,
            crashed_plugins: HashSet::new(),
        }));

        let levels = LevelMeters::new();
        let levels_clone = levels.clone();
        let state_clone = state.clone();

        let lufs = LufsMeter::new();
        let lufs_clone = lufs.clone();

        let spectrum = SpectrumBuffer::new();
        let spectrum_clone = spectrum.clone();

        let pdc_info = crate::mixer::PdcInfo::new();
        let pdc_info_clone = pdc_info.clone();

        thread::Builder::new()
            .name("engine-thread".into())
            .spawn(move || {
                engine_loop(cmd_rx, audio_tx, state_clone, levels_clone, lufs_clone, spectrum_clone, pdc_info_clone, sample_rate, channels);
            })
            .map_err(|e| format!("Failed to spawn engine thread: {e}"))?;

        Ok(Self {
            cmd_tx,
            state,
            levels,
            lufs,
            spectrum,
            pdc_info,
            _backend: backend,
        })
    }

    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

fn engine_loop(
    cmd_rx: Receiver<EngineCommand>,
    audio_tx: Sender<Vec<f32>>,
    state: Arc<RwLock<EngineState>>,
    levels: LevelMeters,
    lufs_meter: LufsMeter,
    spectrum: SpectrumBuffer,
    pdc_info: crate::mixer::PdcInfo,
    sample_rate: u32,
    channels: u16,
) {
    let block_size: usize = 256;
    let mut mixer = Mixer::new(sample_rate, channels);
    // Share the PdcInfo handle so the mixer writes to it and the UI reads from it
    mixer.pdc_info = pdc_info;
    let mut project = Project::default();
    let mut audio_buffers: HashMap<ClipBufferId, Vec<f32>> = HashMap::new();
    let mut transport = TransportState::Stopped;
    let mut position: u64 = 0;
    let mut metronome = Metronome::default();
    let mut loop_enabled = false;
    let mut loop_start: u64 = 0;
    let mut loop_end: u64 = 0;
    let mut master_volume: f32 = 1.0;
    let mut lufs_calc = LufsCalculator::new(sample_rate, channels as usize);

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                EngineCommand::Play => transport = TransportState::Playing,
                EngineCommand::Stop => transport = TransportState::Stopped,
                EngineCommand::SetPosition(pos) => position = pos,
                EngineCommand::UpdateProject(p) => project = p,
                EngineCommand::LoadAudioBuffer { id, samples } => {
                    audio_buffers.insert(id, samples);
                }
                EngineCommand::SetMetronome(enabled) => metronome.enabled = enabled,
                EngineCommand::SetLoop { enabled, start, end } => {
                    loop_enabled = enabled;
                    loop_start = start;
                    loop_end = end;
                }
                EngineCommand::SetMasterVolume(vol) => master_volume = vol,
                EngineCommand::LoadVst3 { slot_id, path } => {
                    mixer.load_vst3(slot_id, &path);
                }
                EngineCommand::UnloadVst3 { slot_id } => {
                    mixer.unload_vst3(&slot_id);
                }
                EngineCommand::AttachParamRx { slot_id, rx } => {
                    mixer.attach_param_rx(&slot_id, rx);
                }
                EngineCommand::LoadVsti { track_id, path } => {
                    mixer.load_vsti(track_id, &path);
                }
                EngineCommand::UnloadVsti { track_id } => {
                    mixer.unload_vsti(&track_id);
                }
                EngineCommand::AttachVstiParamRx { track_id, rx } => {
                    mixer.attach_vsti_param_rx(&track_id, rx);
                }
                EngineCommand::ResetLufs => {
                    lufs_calc.reset();
                    lufs_meter.reset_integrated();
                }
            }
        }

        {
            let mut s = state.write();
            s.transport = transport;
            s.position_samples = position;
            // Report crashed VST3 plugins to the UI
            let crashed = mixer.crashed_plugin_ids();
            if !crashed.is_empty() {
                s.crashed_plugins = crashed;
            }
        }

        if transport == TransportState::Playing {
            let mut block = mixer.render_block(&project, position, block_size, &audio_buffers);

            metronome.render(
                &mut block,
                position,
                block_size,
                channels as usize,
                sample_rate,
                &project.tempo,
                project.time_signature.numerator,
            );

            // Apply master bus effects chain (before master volume)
            mixer.apply_master_effects(&mut block, &project);

            // Apply master volume
            if master_volume != 1.0 {
                for s in block.iter_mut() {
                    *s *= master_volume;
                }
            }

            // Update master level meter
            let (ml, mr) = peak_level(&block, channels as usize);
            levels.set_master_level(ml, mr);

            // Feed LUFS loudness meter
            let readings = lufs_calc.process(&block);
            lufs_meter.write(readings);

            // Feed spectrum analyzer buffer
            spectrum.push_block(&block, channels as usize);

            // Soft-clip before sending to output (prevents harsh digital clipping)
            for s in block.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }

            match audio_tx.send(block) {
                Ok(()) => {
                    position += block_size as u64;
                    // Loop: wrap position back to loop start
                    if loop_enabled && loop_end > loop_start && position >= loop_end {
                        position = loop_start;
                    }
                }
                Err(_) => break,
            }
        } else {
            // Decay meters when stopped
            levels.decay(0.9);
            thread::sleep(Duration::from_millis(5));
        }
    }
}
