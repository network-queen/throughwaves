use std::sync::Arc;
use parking_lot::Mutex;
use midir::{MidiInput, MidiInputConnection};

/// A captured MIDI event with timing.
#[derive(Debug, Clone)]
pub struct MidiEvent {
    pub timestamp_us: u64,
    pub status: u8,
    pub note: u8,
    pub velocity: u8,
}

impl MidiEvent {
    pub fn is_note_on(&self) -> bool {
        (self.status & 0xF0) == 0x90 && self.velocity > 0
    }
    pub fn is_note_off(&self) -> bool {
        (self.status & 0xF0) == 0x80 || ((self.status & 0xF0) == 0x90 && self.velocity == 0)
    }
    pub fn channel(&self) -> u8 {
        self.status & 0x0F
    }
}

/// Info about an available MIDI input port.
#[derive(Debug, Clone)]
pub struct MidiPortInfo {
    pub name: String,
    pub index: usize,
}

/// MIDI input handler — connects to a MIDI device and captures events.
pub struct MidiRecorder {
    connection: Option<MidiInputConnection<()>>,
    buffer: Arc<Mutex<Vec<MidiEvent>>>,
    is_recording: bool,
}

impl MidiRecorder {
    pub fn new() -> Self {
        Self {
            connection: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: false,
        }
    }

    /// List available MIDI input ports.
    pub fn list_ports() -> Vec<MidiPortInfo> {
        let midi_in = match MidiInput::new("JamHub MIDI Scanner") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to create MIDI input: {e}");
                return Vec::new();
            }
        };

        let ports = midi_in.ports();
        ports
            .iter()
            .enumerate()
            .map(|(i, port)| MidiPortInfo {
                name: midi_in.port_name(port).unwrap_or_else(|_| format!("Port {i}")),
                index: i,
            })
            .collect()
    }

    /// Start recording MIDI from the given port index.
    pub fn start(&mut self, port_index: usize) -> Result<(), String> {
        let midi_in = MidiInput::new("JamHub MIDI")
            .map_err(|e| format!("Failed to create MIDI input: {e}"))?;

        let ports = midi_in.ports();
        let port = ports
            .get(port_index)
            .ok_or("Invalid MIDI port index")?;

        let _port_name = midi_in.port_name(port).unwrap_or_default();

        let buffer = self.buffer.clone();
        buffer.lock().clear();

        let conn = midi_in
            .connect(
                port,
                "jamhub-midi-in",
                move |timestamp_us, message, _| {
                    if message.len() >= 3 {
                        let event = MidiEvent {
                            timestamp_us,
                            status: message[0],
                            note: message[1],
                            velocity: message[2],
                        };
                        buffer.lock().push(event);
                    }
                },
                (),
            )
            .map_err(|e| format!("Failed to connect to MIDI port: {e}"))?;

        self.connection = Some(conn);
        self.is_recording = true;
        Ok(())
    }

    /// Stop recording and return captured events.
    pub fn stop(&mut self) -> Vec<MidiEvent> {
        self.connection = None;
        self.is_recording = false;
        let mut buf = self.buffer.lock();
        let events = std::mem::take(&mut *buf);
        events
    }

    /// Peek at current events without consuming.
    pub fn peek_events(&self) -> Vec<MidiEvent> {
        self.buffer.lock().clone()
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }
}

/// Convert raw MIDI events to MidiNote format (note on/off pairs).
pub fn events_to_notes(
    events: &[MidiEvent],
    ticks_per_beat: u64,
    us_per_beat: f64,
) -> Vec<jamhub_model::MidiNote> {
    let mut notes = Vec::new();
    let mut pending: Vec<(u8, u64, u8)> = Vec::new(); // (note, start_tick, velocity)

    for event in events {
        let tick = (event.timestamp_us as f64 / us_per_beat * ticks_per_beat as f64) as u64;

        if event.is_note_on() {
            pending.push((event.note, tick, event.velocity));
        } else if event.is_note_off() {
            if let Some(idx) = pending.iter().position(|&(n, _, _)| n == event.note) {
                let (pitch, start_tick, velocity) = pending.remove(idx);
                let duration = tick.saturating_sub(start_tick).max(1);
                notes.push(jamhub_model::MidiNote {
                    pitch,
                    velocity,
                    start_tick,
                    duration_ticks: duration,
                });
            }
        }
    }

    // Close any remaining open notes
    let last_tick = events.last().map(|e| {
        (e.timestamp_us as f64 / us_per_beat * ticks_per_beat as f64) as u64
    }).unwrap_or(0);

    for (pitch, start_tick, velocity) in pending {
        notes.push(jamhub_model::MidiNote {
            pitch,
            velocity,
            start_tick,
            duration_ticks: last_tick.saturating_sub(start_tick).max(ticks_per_beat),
        });
    }

    notes
}
