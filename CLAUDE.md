# CLAUDE.md — ThroughWaves

## What is this project?

ThroughWaves is a professional digital audio workstation (DAW) built entirely in Rust. It features real-time audio mixing, VST3 plugin hosting, MIDI sequencing, AI-powered stem separation, live collaborative jam sessions, and a web platform for sharing music. The GUI is GPU-accelerated via egui/eframe.

## Architecture

Cargo workspace with 5 crates + a Python microservice:

```
crates/
  model/     → Data model (Project, Track, Clip, Effect, Automation, MIDI, Tempo, Versioning)
  engine/    → Audio engine (mixer, effects, VST3 host, recording, export, transport, synth)
  app/       → GUI layer (egui/eframe) — timeline, mixer, session view, plugin windows
  network/   → WebSocket collaboration (client/server, session sync messages)
  server/    → Web platform API (axum + PostgreSQL — auth, tracks, projects, social, jam rooms)
tools/
  stem_separator/  → Python FastAPI service wrapping Facebook's Demucs for AI stem separation
```

### Dependency graph

```
model  ←  engine  ←  app (binary: throughwaves)
  ↑         ↑
  └── network ──→ server (binary: throughwaves-server)
```

- **`jamhub-model`** — Pure data types, zero audio dependencies. Serializable with serde. Contains `Project`, `Track`, `Clip`, `ClipSource`, `EffectSlot`, `TrackEffect`, `Automation`, `Tempo`, `TempoMap`, `TimeSignature`, `TransportState`, `Scene`, `SessionClip`, `Marker`, `TrackGroup`, `MidiMapping`, `MacroControl`, `Region`, `ProjectVersion`.
- **`jamhub-engine`** — Real-time audio. Owns the `AudioBackend` (cpal), `Mixer` (parallel via rayon), `EffectProcessor` (15 built-in + parametric EQ), `Recorder`, `Vst3Plugin` host, `Synth`, `WaveformCache`, `LevelMeters`, `LufsMeter`, `SpectrumBuffer`, `Metronome`, `InputMonitor`. Communicates with the app via `EngineHandle` + `crossbeam-channel` commands (`EngineCommand` enum).
- **`throughwaves` (app)** — The desktop GUI binary. Uses `eframe` for windowing, `egui` for immediate-mode UI. Modules: `timeline`, `mixer_view`, `session_view`, `piano_roll`, `effects_panel`, `transport_bar`, `spectrum`, `analysis_tools`, `audio_settings`, `stem_separator`, `jam_session`, `version_control`, `undo`, `templates`, `plugin_window` (native VST3 editor embedding via NSView on macOS).
- **`jamhub-network`** — WebSocket-based collaboration. `SessionMessage` enum (tagged JSON) for peer join/leave, transport sync, track ops, clip ops, chat, audio streaming.
- **`throughwaves-server`** — Axum web server with PostgreSQL (sqlx). Modules: `auth` (JWT + bcrypt), `tracks`, `projects`, `social`, `bands`, `cloud`, `jam`, `admin`. Runs schema migration on startup via embedded `schema.sql`.

## Tech stack

| Layer | Technology |
|-------|-----------|
| Language | Rust 2021 edition |
| GUI | egui 0.31 / eframe 0.31 |
| Audio I/O | cpal 0.15 |
| Audio decoding | symphonia 0.5 (MP3, OGG, FLAC, WAV) |
| WAV writing | hound 3.5 |
| VST3 hosting | vst3 0.3 + libloading 0.8 |
| MIDI input | midir 0.10 |
| Parallelism | rayon 1.10 (mixer), crossbeam-channel 0.5 (engine commands) |
| Concurrency | parking_lot 0.12 (RwLock), tokio 1 (async networking) |
| Serialization | serde + serde_json |
| Web server | axum 0.8 + tower-http 0.6 |
| Database | PostgreSQL via sqlx 0.8 |
| Auth | bcrypt 0.16, jsonwebtoken 9 |
| WebSocket | tokio-tungstenite 0.26 |
| macOS native | objc2 0.6 + objc2-app-kit 0.3 (NSView for VST3 plugin windows) |
| File dialogs | rfd 0.15 |
| AI stems | Python FastAPI + Demucs (separate service) |
| HTTP client | ureq 3 |
| IDs | uuid v4 |

## Build & run

```bash
# Debug build
cargo build

# Release build (optimized: opt-level 3, thin LTO, codegen-units 1)
cargo build --release

# Run the DAW
cargo run --release --bin throughwaves

# Run the web/jam server
cargo run --release --bin throughwaves-server

# macOS .app bundle
make bundle          # release bundle
make bundle-debug    # debug bundle
make run             # build + open .app
make icon            # generate .icns from icon.png

# Release packaging (DMG, ZIP, deb, tar.gz)
./scripts/build-release.sh          # auto-detect OS
./scripts/build-release.sh macos    # macOS DMG
./scripts/build-release.sh linux    # Linux .deb + tar.gz
./scripts/build-release.sh windows  # Windows ZIP
```

## CI / Release

GitHub Actions workflow (`.github/workflows/release.yml`) triggers on `v*` tags:
- Builds macOS universal binary (aarch64 + x86_64 via lipo), creates DMG
- Builds Windows x64, creates ZIP
- Builds Linux x86_64, creates .deb + tar.gz
- Creates GitHub Release with all artifacts

## Code conventions

- **Workspace crate names** use `jamhub-` prefix (e.g. `jamhub-model`, `jamhub-engine`, `jamhub-network`); the app binary crate is named `throughwaves`, the server is `throughwaves-server`.
- **Module organization**: One file per major feature/view in the app crate. Engine modules map 1:1 to subsystems (mixer, effects, recorder, transport, etc.).
- **Immediate-mode UI**: All UI code follows egui's immediate-mode pattern — no retained widget tree. Each panel/view is a module with a `show()` or `ui()` method taking `&mut egui::Ui`.
- **Thread communication**: The audio engine runs on a dedicated thread. The app sends `EngineCommand` variants via `crossbeam-channel`. Shared state (levels, LUFS, spectrum, PDC) uses `Arc<RwLock<T>>` from `parking_lot`.
- **Audio hot path**: Zero heap allocations in the render loop. Pre-allocated buffers. Parallel track rendering via rayon. Soft-clipping on master output.
- **Serialization**: All model types derive `Serialize` + `Deserialize`. Projects save/load as JSON via `serde_json`.
- **IDs**: All entities (tracks, clips, effects, markers, etc.) use `Uuid` (v4).
- **Error handling**: Engine/audio errors return `Result<T, String>`. The app uses `unwrap`/`expect` sparingly; most fallible ops are in the engine layer.
- **`#[allow(dead_code)]`** is used on a few modules that are in-progress (e.g. `media_browser`, `platform_panel`, `version_control`).
- **Theme**: Custom dark theme with warm gold accent (#F0C040), teal selection, deep charcoal backgrounds. Multiple theme variants (Dark, Darker, Midnight).
- **License**: GPL-3.0-or-later.

## Key file locations

| What | Where |
|------|-------|
| App entry point | `crates/app/src/main.rs` |
| Server entry point | `crates/server/src/main.rs` |
| Project data model | `crates/model/src/project.rs` |
| Time/tempo types | `crates/model/src/time.rs` |
| Audio engine transport | `crates/engine/src/transport.rs` |
| Mixer (parallel render) | `crates/engine/src/mixer.rs` |
| Effects DSP | `crates/engine/src/effects.rs` |
| VST3 host | `crates/engine/src/vst3_host.rs` |
| Recording | `crates/engine/src/recorder.rs` |
| Export (WAV/FLAC/AIFF) | `crates/engine/src/export.rs` |
| Timeline UI | `crates/app/src/timeline.rs` |
| Mixer UI | `crates/app/src/mixer_view.rs` |
| Session/clip launcher UI | `crates/app/src/session_view.rs` |
| Piano roll UI | `crates/app/src/piano_roll.rs` |
| Network messages | `crates/network/src/message.rs` |
| DB schema | `crates/server/schema.sql` |
| Stem separation service | `tools/stem_separator/server.py` |
| macOS app bundle | `ThroughWaves.app/` |
| Release build script | `scripts/build-release.sh` |
| CI workflow | `.github/workflows/release.yml` |

## Codebase size

~40k lines of Rust across all crates:
- `app`: ~27k lines (GUI)
- `engine`: ~7k lines (audio DSP + transport)
- `server`: ~4k lines (web platform API)
- `model`: ~1.3k lines (data types)
- `network`: ~600 lines (collaboration protocol)
