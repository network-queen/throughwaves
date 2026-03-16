# JamHub

**Collaborative DAW built in Rust**

A professional-grade digital audio workstation with real-time collaboration, VST3 plugin hosting, and a modern GPU-accelerated interface. Built entirely in Rust for safety, speed, and low-latency audio performance.

<!-- Screenshots: Add screenshots of the arrange view, mixer, and session view here -->
<!-- ![JamHub Screenshot](docs/screenshots/arrange-view.png) -->

---

## Features

### Audio Engine
- Low-latency audio I/O via **cpal** with configurable device selection
- Real-time mixing with per-track volume, pan (equal-power pan law), mute, and solo
- Plugin Delay Compensation (PDC) across the entire signal chain
- Audio recording with punch-in/out, count-in, and input monitoring
- Overlap-Add (OLA) time-stretching with pitch preservation
- Non-destructive clip editing: fade in/out, gain, reverse, transpose, looping, slip editing
- Auto-crossfade between overlapping clips
- Bounce/export to WAV, FLAC, Ogg Vorbis, and MP3
- Stem export and track bouncing with progress reporting
- LUFS loudness metering and real-time spectrum analyzer

### Effects & Processing
- Built-in effects: Gain, Low Pass, High Pass, Delay, Reverb, Compressor, Chorus, Distortion
- 8-band parametric EQ with multiple filter types (Peak, Low Shelf, High Shelf, Low Pass, High Pass, Notch)
- Sidechain compression with per-track routing
- Full effect parameter automation with interpolated curves
- Master bus effect chain
- **VST3 plugin hosting** with full COM hosting, editor UI, and parameter automation

### Arrangement & Editing
- Multi-track timeline with waveform display
- Clip splitting, duplicating, copy/paste, and nudge
- Take lanes with comp editing and flatten
- Snap-to-grid with configurable snap modes
- Automation lanes for volume, pan, and all effect parameters
- Markers, regions, and tempo map with time signature changes
- Undo/redo history

### Routing & Mixing
- Flexible send routing (pre/post-fader) to any track
- Output target routing (submix buses)
- Track grouping/folders
- Track freeze for CPU optimization
- MIDI CC learn and mapping to any parameter
- 8 assignable macro controls with multi-parameter targeting

### Session View
- Ableton-style clip launcher with scenes
- Session clips with per-slot triggering

### Collaboration
- Real-time networked sessions via WebSocket
- Project sharing with synchronized state

### Interface
- GPU-accelerated UI via **egui/eframe**
- Dark studio theme with warm color palette
- Arrange, Mixer, and Session view modes
- FX browser, media browser, and audio pool
- Piano roll for MIDI editing
- Drag-and-drop audio file import
- Project templates

---

## System Requirements

| Requirement | Details |
|-------------|---------|
| OS | macOS (primary), with Linux/Windows support planned |
| Rust | 1.75 or newer |
| Audio | Core Audio (macOS); VST3 plugins in `~/Library/Audio/Plug-Ins/VST3` |
| GPU | Metal or OpenGL for egui rendering |

---

## Building

```bash
# Clone the repository
git clone https://github.com/your-org/jamhub.git
cd jamhub

# Build in release mode (recommended for audio work)
cargo build --release

# Run the application
cargo run --bin jamhub-app

# Run the collaboration server
cargo run --bin jamhub-server
```

---

## Keyboard Shortcuts

### Transport
| Key | Action |
|-----|--------|
| `Space` | Play / Stop |
| `Home` | Rewind to start |
| `R` | Start / stop recording |
| `M` | Toggle metronome |
| `L` | Toggle loop mode |
| `C` | Toggle count-in |
| `P` | Toggle punch in/out |
| `H` | Toggle follow playhead |
| `F` | Focus / scroll to playhead |

### Editing
| Key | Action |
|-----|--------|
| `S` | Split clip at playhead |
| `Del / Backspace` | Delete selected clip or track |
| `Escape` | Clear selection |
| `G` | Cycle snap mode |
| `Alt+Left / Right` | Nudge clip(s) left / right |
| `Cmd+Z` | Undo |
| `Cmd+Shift+Z` | Redo |
| `Cmd+S` | Save project |
| `Cmd+D` | Duplicate track or clips |
| `Cmd+C / Cmd+V` | Copy / Paste clips |
| `Cmd+I` | Import audio file |
| `Cmd+B` | Bounce track |
| `Cmd+J` | Consolidate clips |
| `Cmd+M` | Add marker at playhead |

### Navigation & View
| Key | Action |
|-----|--------|
| `Up / Down` | Switch tracks |
| `1-9` | Select track by number |
| `[ / ]` | Jump to previous / next marker |
| `Z` | Zoom to fit |
| `Tab` | Cycle views (Arrange / Mixer / Session) |
| `Cmd+E` | Toggle effects panel |
| `Cmd+F` | Toggle FX browser |
| `A` | Toggle automation lanes |
| `Q` | Toggle spectrum analyzer |
| `?` | Open shortcuts panel |

---

## Architecture

```
jamhub/
  crates/
    app/       UI layer (egui/eframe) — timeline, mixer, session views, plugin windows
    engine/    Audio engine — mixer, effects, VST3 hosting, recording, export, transport
    model/     Data model — project, tracks, clips, effects, automation, MIDI
    network/   Collaboration — WebSocket client/server, session sync
    server/    Standalone collaboration server binary
```

- **Frontend**: egui (immediate-mode GPU-accelerated UI) via eframe
- **Audio I/O**: cpal for cross-platform low-latency audio
- **VST3 Hosting**: Direct COM vtable calls via the `vst3` crate, with libloading for dynamic library loading
- **State Management**: `parking_lot` RwLock for lock-free audio thread access; crossbeam channels for parameter changes
- **Serialization**: serde with JSON for project files

---

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) for details.

---

## Credits

Built with:
- [egui](https://github.com/emilk/egui) — Immediate-mode GUI
- [cpal](https://github.com/RustAudio/cpal) — Cross-platform audio I/O
- [vst3](https://crates.io/crates/vst3) — VST3 SDK bindings
- [parking_lot](https://github.com/Amanieu/parking_lot) — Fast synchronization primitives
- [crossbeam](https://github.com/crossbeam-rs/crossbeam) — Lock-free data structures
- [serde](https://serde.rs/) — Serialization framework
- [uuid](https://github.com/uuid-rs/uuid) — Unique identifiers
