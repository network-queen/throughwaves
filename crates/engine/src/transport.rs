use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use jamhub_model::{ClipBufferId, Project, TransportState};

use crate::audio::AudioBackend;
use crate::mixer::Mixer;

pub enum EngineCommand {
    Play,
    Stop,
    SetPosition(u64),
    UpdateProject(Project),
    LoadAudioBuffer { id: ClipBufferId, samples: Vec<f32> },
}

pub struct EngineHandle {
    cmd_tx: Sender<EngineCommand>,
    pub state: Arc<RwLock<EngineState>>,
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
        let (audio_tx, audio_rx) = bounded::<Vec<f32>>(64);

        backend.start(audio_rx)?;

        let state = Arc::new(RwLock::new(EngineState {
            transport: TransportState::Stopped,
            position_samples: 0,
            sample_rate,
        }));

        let state_clone = state.clone();

        thread::Builder::new()
            .name("engine-thread".into())
            .spawn(move || {
                engine_loop(cmd_rx, audio_tx, state_clone, sample_rate, channels);
            })
            .map_err(|e| format!("Failed to spawn engine thread: {e}"))?;

        Ok(Self { cmd_tx, state })
    }

    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

fn engine_loop(
    cmd_rx: Receiver<EngineCommand>,
    audio_tx: Sender<Vec<f32>>,
    state: Arc<RwLock<EngineState>>,
    sample_rate: u32,
    channels: u16,
) {
    let block_size: usize = 512;
    let mixer = Mixer::new(sample_rate, channels);
    let mut project = Project::default();
    let mut audio_buffers: HashMap<ClipBufferId, Vec<f32>> = HashMap::new();
    let mut transport = TransportState::Stopped;
    let mut position: u64 = 0;

    loop {
        // Process commands
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                EngineCommand::Play => {
                    transport = TransportState::Playing;
                }
                EngineCommand::Stop => {
                    transport = TransportState::Stopped;
                }
                EngineCommand::SetPosition(pos) => {
                    position = pos;
                }
                EngineCommand::UpdateProject(p) => {
                    project = p;
                }
                EngineCommand::LoadAudioBuffer { id, samples } => {
                    audio_buffers.insert(id, samples);
                }
            }
        }

        // Update shared state
        {
            let mut s = state.write();
            s.transport = transport;
            s.position_samples = position;
        }

        if transport == TransportState::Playing {
            let block = mixer.render_block(&project, position, block_size, &audio_buffers);
            position += block_size as u64;

            // Try to send the block; if the audio output is backed up, drop it
            let _ = audio_tx.try_send(block);
        } else {
            // When stopped, sleep a bit to avoid burning CPU
            thread::sleep(std::time::Duration::from_millis(5));
        }

        // Pace the render loop to roughly match real-time
        if transport == TransportState::Playing {
            let block_duration_us = (block_size as f64 / sample_rate as f64 * 1_000_000.0) as u64;
            thread::sleep(std::time::Duration::from_micros(block_duration_us / 2));
        }
    }
}
