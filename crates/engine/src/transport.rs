use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use jamhub_model::{ClipBufferId, Project, TransportState};

use crate::audio::AudioBackend;
use crate::levels::{peak_level, LevelMeters};
use crate::metronome::Metronome;
use crate::mixer::Mixer;

pub enum EngineCommand {
    Play,
    Stop,
    SetPosition(u64),
    UpdateProject(Project),
    LoadAudioBuffer { id: ClipBufferId, samples: Vec<f32> },
    SetMetronome(bool),
}

pub struct EngineHandle {
    cmd_tx: Sender<EngineCommand>,
    pub state: Arc<RwLock<EngineState>>,
    pub levels: LevelMeters,
    _backend: AudioBackend,
}

pub struct EngineState {
    pub transport: TransportState,
    pub position_samples: u64,
    pub sample_rate: u32,
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
        }));

        let levels = LevelMeters::new();
        let levels_clone = levels.clone();
        let state_clone = state.clone();

        thread::Builder::new()
            .name("engine-thread".into())
            .spawn(move || {
                engine_loop(cmd_rx, audio_tx, state_clone, levels_clone, sample_rate, channels);
            })
            .map_err(|e| format!("Failed to spawn engine thread: {e}"))?;

        Ok(Self {
            cmd_tx,
            state,
            levels,
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
    sample_rate: u32,
    channels: u16,
) {
    let block_size: usize = 256;
    let mut mixer = Mixer::new(sample_rate, channels);
    let mut project = Project::default();
    let mut audio_buffers: HashMap<ClipBufferId, Vec<f32>> = HashMap::new();
    let mut transport = TransportState::Stopped;
    let mut position: u64 = 0;
    let mut metronome = Metronome::default();

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
            }
        }

        {
            let mut s = state.write();
            s.transport = transport;
            s.position_samples = position;
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

            // Update master level meter
            let (ml, mr) = peak_level(&block, channels as usize);
            levels.set_master_level(ml, mr);

            match audio_tx.send(block) {
                Ok(()) => {
                    position += block_size as u64;
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
