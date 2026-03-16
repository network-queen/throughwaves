# JamHub

**Collaborative DAW built in Rust**

A professional-grade digital audio workstation with real-time collaboration, VST3 plugin hosting, and a modern GPU-accelerated interface. Built entirely in Rust for safety, speed, and low-latency audio performance.

---

## Features

### Audio Engine
- Low-latency audio I/O via **cpal** with configurable device selection
- Real-time mixing with per-track volume, pan (equal-power pan law), mute, solo, and exclusive solo
- Plugin Delay Compensation (PDC) across the entire signal chain with per-track delay buffers
- Audio recording with punch-in/out, count-in (configurable bars), and input monitoring
- MIDI recording from hardware controllers with real-time note capture
- Overlap-Add (OLA) time-stretching with pitch preservation
- Non-destructive clip editing: fade in/out, gain, reverse, transpose, looping, slip editing, playback rate
- Auto-crossfade between overlapping clips
- Soft-clipping on master output to prevent harsh digital distortion
- Bounded audio ring buffer (~0.5s cap) to prevent unbounded memory growth

### Export
- Export to **WAV** (16/24/32-bit), **FLAC** (16/24-bit), and **AIFF** (16/24/32-bit)
- Configurable sample rate, bit depth, mono/stereo, and normalization
- Stem export: render each track individually to separate files
- Track bouncing with progress reporting and cancellation support
- Range bouncing for selected time regions
- Effects tail rendering (configurable seconds)

### Effects & Processing
- Built-in effects: Gain, Low Pass, High Pass, Delay, Reverb, Compressor, Chorus, Distortion
- 8-band parametric EQ with 6 filter types: Peak, Low Shelf, High Shelf, Low Pass, High Pass, Notch
- Interactive EQ frequency response visualization
- Sidechain compression with per-track routing
- Full effect parameter automation with interpolated curves
- Master bus effect chain (applied before master volume)
- Global FX bypass with per-slot state preservation
- Phase invert and mono summing per track
- **VST3 plugin hosting** with full COM hosting:
  - Dynamic library loading and factory instantiation
  - Audio processing via IAudioProcessor with catch_unwind crash protection
  - Native plugin editor windows (macOS NSView embedding)
  - Parameter change forwarding via IComponentHandler
  - Component/controller connection for separate-controller plugins
  - Plugin latency reporting for PDC
  - VSTi instrument hosting: MIDI-to-audio via IEventList
  - Graceful crash recovery: crashed plugins are disabled without crashing the host

### Arrangement & Editing
- Multi-track timeline with mip-mapped waveform display (5 resolution levels)
- Clip splitting, duplicating, copy/paste, nudge, and consolidation
- Multi-clip selection with rubber-band (marquee) tool
- Take lanes with swipe comping and flatten
- Ripple editing mode: moving/deleting clips shifts subsequent clips
- Snap-to-grid with 8 modes: Free, 1/2 Beat, Triplet, Beat, 1/16, 1/32, Bar, Marker
- Magnetic snap to clip edges with visual indicator
- Configurable grid display (independent of snap mode)
- Automation lanes for volume, pan, mute, and all effect parameters
- Markers with colors, naming, and drag repositioning
- Named regions (loop areas) with colors
- Tempo map with time signature changes and tempo automation
- Tap tempo
- 50-level undo/redo with history browser and jump-to-any-state
- Insert silence at playhead
- Clip properties: gain, speed, transpose, reverse, loop count, color
- Inline clip and track renaming
- Track height resizing with separator drag
- Track reordering (move up/down)

### Routing & Mixing
- Flexible send routing (pre/post-fader) to any track
- Output target routing (submix buses)
- Track grouping/folders with collapse/expand, group mute/solo
- Track freeze for CPU optimization (with unfreeze restore)
- Hardware input channel selection per track

### MIDI
- MIDI input from hardware controllers (port selection, device listing)
- MIDI CC learn: click a parameter, move a knob, mapping is created
- MIDI CC mapping to track volume, pan, effect parameters, and master volume
- 8 assignable macro controls with multi-parameter targeting and independent ranges
- Piano roll editor for MIDI note editing
- MIDI note events routed to built-in synth or VST3 instruments
- Built-in synthesizer with Saw/Sine/Square/Triangle waveforms, ADSR envelope, and low-pass filter

### Metering & Analysis
- EBU R128 LUFS loudness metering: momentary (400ms), short-term (3s), and integrated (gated)
- Clipping detection with visual indicator
- Real-time FFT spectrum analyzer with Hann windowing
- Per-track level meters (L/R peak) with smooth decay
- Master level meters
- CPU usage estimation

### Session View
- Ableton-style clip launcher with scenes
- Session clips with per-slot triggering
- Scene management (add, delete, rename)

### Collaboration
- Real-time networked sessions via WebSocket
- Project sharing with synchronized state
- Session panel with connection management

### Interface
- GPU-accelerated UI via **egui/eframe**
- Premium dark studio theme with 3 variants: Dark, Darker, Midnight
- Configurable UI scale
- Arrange, Mixer, and Session view modes
- Docked mixer panel in arrange view
- Minimap overview bar for timeline navigation
- Auto-follow playhead during playback
- FX browser with VST3 plugin scanning
- Media browser for audio file browsing
- Audio pool manager for project audio buffers
- Piano roll for MIDI editing
- Drag-and-drop audio file import from Finder
- Project templates (save/load track configurations)
- FX chain presets
- Track color palette with custom RGB picker
- Welcome screen with recent projects
- Keyboard shortcuts panel with search
- Project info panel (name, notes, creation date, statistics)
- Autosave with recovery dialog
- Recent projects list
- Persistent layout saving
- 9 locator memory positions (Shift+1-9 to save, 1-9 to recall)

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
| `Alt+Up / Down` | Move track up / down |
| `Cmd+Z` | Undo |
| `Cmd+Shift+Z` | Redo |
| `Cmd+S` | Save project |
| `Cmd+D` | Duplicate track or clips |
| `Cmd+C / Cmd+V` | Copy / Paste clips |
| `Cmd+I` | Import audio file |
| `Cmd+B` | Bounce track |
| `Cmd+J` | Consolidate clips |
| `Cmd+M` | Add marker at playhead |
| `Shift+1-9` | Save locator position |
| `1-9` | Recall locator position |

### Navigation & View
| Key | Action |
|-----|--------|
| `Up / Down` | Switch tracks |
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
    app/       UI layer (egui/eframe) -- timeline, mixer, session views, plugin windows
    engine/    Audio engine -- mixer, effects, VST3 hosting, recording, export, transport
    model/     Data model -- project, tracks, clips, effects, automation, MIDI
    network/   Collaboration -- WebSocket client/server, session sync
    server/    Standalone collaboration server binary
```

### Thread Safety Model
- **UI thread** (main): owns `DawApp` state, reads engine state via `Arc<RwLock<EngineState>>`
- **Engine thread**: owns the `Mixer`, audio buffers, and VST3 plugins; communicates via bounded crossbeam channels
- **Audio callback** (cpal): bounded `VecDeque` ring buffer prevents unbounded growth
- **Shared state**: `parking_lot::RwLock` for level meters, LUFS readings, spectrum data, PDC info; `parking_lot::Mutex` for recording buffer and input monitor
- **No data races**: all cross-thread communication uses `Arc<RwLock>`, `Arc<Mutex>`, or bounded channels; no raw shared mutable state

### Memory Management
- Undo history capped at 50 entries with automatic pruning
- Audio ring buffer capped at ~0.5s of audio
- LUFS integrated measurement blocks capped at ~1 hour
- Spectrum analyzer uses a fixed 4096-sample ring buffer
- Input monitor ring buffer capped at 4096 samples
- Waveform cache entries removed when clips are deleted
- VST3 plugin windows intentionally kept alive (hidden) on close to avoid JUCE teardown crashes; OS reclaims on exit

### Technology Stack
- **Frontend**: egui (immediate-mode GPU-accelerated UI) via eframe
- **Audio I/O**: cpal for cross-platform low-latency audio
- **Audio Decode**: symphonia for WAV, MP3, OGG, FLAC decoding
- **Audio Encode**: hound for WAV writing; custom pure-Rust FLAC and AIFF writers
- **VST3 Hosting**: Direct COM vtable calls via the `vst3` crate, with libloading for dynamic library loading
- **MIDI**: midir for hardware MIDI input
- **Concurrency**: `parking_lot` RwLock/Mutex for lock-free audio thread access; crossbeam channels for command passing
- **Parallelism**: rayon for parallel track rendering in the mixer
- **Serialization**: serde with JSON for project files

---

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) for details.

---

## Credits

Built with:
- [egui](https://github.com/emilk/egui) -- Immediate-mode GUI
- [cpal](https://github.com/RustAudio/cpal) -- Cross-platform audio I/O
- [vst3](https://crates.io/crates/vst3) -- VST3 SDK bindings
- [symphonia](https://github.com/pdeljanov/Symphonia) -- Audio decoding
- [hound](https://github.com/ruuda/hound) -- WAV reading/writing
- [midir](https://github.com/Boddlnagg/midir) -- MIDI I/O
- [rayon](https://github.com/rayon-rs/rayon) -- Data parallelism
- [parking_lot](https://github.com/Amanieu/parking_lot) -- Fast synchronization primitives
- [crossbeam](https://github.com/crossbeam-rs/crossbeam) -- Lock-free data structures
- [serde](https://serde.rs/) -- Serialization framework
- [uuid](https://github.com/uuid-rs/uuid) -- Unique identifiers
