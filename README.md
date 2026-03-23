# ThroughWaves

**Professional DAW built in Rust — Collaborative Music Platform**

A professional-grade digital audio workstation with real-time collaboration, live jam sessions, version branching, AI-powered stem separation, and a modern GPU-accelerated interface. Built entirely in Rust for safety, speed, and low-latency audio performance.

![ThroughWaves Screenshot](docs/screenshots/arrange-view.png)
*Arrange view with timeline, track headers, and minimap*

![Mixer View](docs/screenshots/mixer-view.png)
*Full mixer with channel strips, sends, and level meters*

![Session View](docs/screenshots/session-view.png)
*Ableton-style clip launcher with scenes*

---

## Features

### Audio Engine
- Low-latency audio I/O via **cpal** with configurable device selection
- Real-time mixing with per-track volume, pan (equal-power pan law), mute, solo, and exclusive solo
- Parallel track rendering via **rayon** for multi-core utilization
- Plugin Delay Compensation (PDC) across the entire signal chain with per-track delay buffers
- Audio recording with punch-in/out, count-in (configurable bars), and input monitoring
- MIDI recording from hardware controllers with real-time note capture
- Overlap-Add (OLA) time-stretching with pitch preservation
- Non-destructive clip editing: fade in/out (5 curve types), gain, reverse, transpose, looping, slip editing, playback rate
- Auto-crossfade between overlapping clips
- Soft-clipping on master output to prevent harsh digital distortion
- Pre-allocated audio buffers in the render path — zero heap allocations in the hot loop

### Export
- Export to **WAV** (16/24/32-bit), **FLAC** (16/24-bit), and **AIFF** (16/24/32-bit)
- Configurable sample rate, bit depth, mono/stereo, and normalization
- Stem export: render each track individually to separate files
- Track bouncing with progress reporting and cancellation support
- Range bouncing for selected time regions

### Effects & Processing
- 15 built-in effects: Gain, Low Pass, High Pass, Delay, Reverb, Compressor, Chorus, Distortion, Limiter, Gate, Phaser, Flanger, Tremolo, EQ Band, Parametric EQ
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
  - VSTi instrument hosting: MIDI-to-audio via IEventList
  - Graceful crash recovery: crashed plugins are disabled without crashing the host

### Arrangement & Editing
- Multi-track timeline with mip-mapped waveform display (5 resolution levels)
- Waveform vertical zoom (visual amplification without changing audio)
- Clip splitting, duplicating, copy/paste, nudge, and consolidation
- Multi-clip selection with rubber-band (marquee) tool
- Take lanes with swipe comping and flatten
- Ripple editing mode: moving/deleting clips shifts subsequent clips
- Snap-to-grid with 8 modes: Free, 1/2 Beat, Triplet, Beat, 1/16, 1/32, Bar, Marker
- Crossfade curves: Linear, Exponential, Logarithmic, S-Curve, Equal Power
- 50-level undo/redo with history browser
- Tap tempo with beat flash indicator

### Routing & Mixing
- Flexible send routing (pre/post-fader) to any track
- Track grouping/folders with collapse/expand, group mute/solo
- Track freeze for CPU optimization (with unfreeze restore)

### MIDI
- MIDI input from hardware controllers
- MIDI CC learn and mapping to any parameter
- 8 assignable macro controls
- Piano roll editor with velocity and CC lanes
- Built-in synthesizer with Saw/Sine/Square/Triangle, ADSR, and filter

### Metering & Analysis
- EBU R128 LUFS loudness metering (momentary, short-term, integrated)
- True peak metering with 4x oversampled interpolation
- Stereo phase correlation meter
- Real-time FFT spectrum analyzer
- Reference track A/B comparison with loudness matching
- Chord detection overlay on timeline clips

### AI Stem Separation
- Neural network-based stem separation powered by Demucs
- Separate any audio into vocals, drums, bass, and other stems
- Import separated stems as individual tracks

### Live Jam Sessions
- Real-time multi-user jam rooms via WebSocket
- Low-latency audio streaming between participants
- Per-participant volume control and in-session chat
- Shared BPM synchronization

### Version Branching
- Git-style version control for project snapshots
- Create named branches for alternative arrangements
- Browse version history and restore any previous version

### Platform & Collaboration
- Web platform with track uploads, streaming, and social features
- Import tracks from platform directly into DAW
- Project templates, track templates, FX presets
- Autosave with recovery, persistent layout

---

## Download

Visit **throughwaves.com** to download installers for macOS, Windows, and Linux.

Or build from source:

```bash
git clone https://github.com/throughwaves/throughwaves.git
cd throughwaves
cargo build --release --bin jamhub-app
cargo run --release --bin jamhub-app
```

---

## System Requirements

| Requirement | Details |
|-------------|---------|
| OS | macOS 13+, Windows 10+, Linux (Ubuntu 22.04+) |
| RAM | 4 GB minimum, 8 GB recommended |
| Audio | Built-in audio or USB/Thunderbolt interface |
| Rust | 1.75+ (for building from source) |

---

## Architecture

```
throughwaves/
  crates/
    app/       UI layer (egui/eframe) — timeline, mixer, session views, plugin windows
    engine/    Audio engine — mixer, effects, VST3 hosting, recording, export, transport
    model/     Data model — project, tracks, clips, effects, automation, MIDI, versions
    network/   Collaboration — WebSocket client/server, session sync
    server/    Web platform + jam session server
  tools/
    stem_separator/  Python FastAPI service wrapping Demucs for AI stem separation
```

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding guidelines, and how to submit pull requests.

For detailed architecture docs, see [CLAUDE.md](CLAUDE.md).

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a full list of features and changes.

---

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) for details.

---

## Credits

Built with [egui](https://github.com/emilk/egui), [cpal](https://github.com/RustAudio/cpal), [vst3](https://crates.io/crates/vst3), [symphonia](https://github.com/pdeljanov/Symphonia), [rayon](https://github.com/rayon-rs/rayon), [axum](https://github.com/tokio-rs/axum), [Demucs](https://github.com/facebookresearch/demucs), and more.
