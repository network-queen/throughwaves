# Contributing to ThroughWaves

Thank you for your interest in contributing to ThroughWaves! This document covers how to get started, our development workflow, and coding guidelines.

## Getting started

### Prerequisites

- **Rust 1.75+** — install via [rustup](https://rustup.rs/)
- **Platform audio libraries**:
  - macOS: included with Xcode Command Line Tools
  - Linux: `sudo apt install libasound2-dev libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libgtk-3-dev libatk1.0-dev`
  - Windows: no extra dependencies
- **PostgreSQL** (only for server development)

### Building

```bash
git clone https://github.com/throughwaves/throughwaves.git
cd throughwaves
cargo build
```

### Running

```bash
# Run the DAW
cargo run --release --bin throughwaves

# Run the web/jam server (requires PostgreSQL)
DATABASE_URL=postgres://localhost/jamhub cargo run --release --bin throughwaves-server
```

## Project structure

ThroughWaves is a Cargo workspace with 5 crates:

| Crate | Purpose |
|-------|---------|
| `crates/model` | Data types (Project, Track, Clip, etc.) — no audio dependencies |
| `crates/engine` | Audio engine — mixer, effects, VST3 host, recording, export |
| `crates/app` | Desktop GUI (egui/eframe) — timeline, mixer, session view |
| `crates/network` | WebSocket collaboration protocol |
| `crates/server` | Web platform API (axum + PostgreSQL) |

See [CLAUDE.md](CLAUDE.md) for the full architecture overview.

## Development workflow

1. **Fork** the repository and create a feature branch from `main`.
2. **Make your changes** — keep commits focused and well-described.
3. **Test your changes** — run `cargo build` and `cargo clippy` at minimum.
4. **Open a pull request** against `main` with a clear description.

### Code style

- Run `cargo fmt` before committing.
- Run `cargo clippy` and address all warnings.
- Follow existing naming conventions — see [CLAUDE.md](CLAUDE.md) for details.

### Audio engine guidelines

- **Zero allocations in the render loop.** Pre-allocate buffers. Never call `Vec::push`, `String::new`, or `Box::new` in the mixer's hot path.
- Use `parking_lot::RwLock` for shared state between the audio thread and UI thread.
- Send commands to the engine via `EngineCommand` — never mutate engine state directly from the UI.

### UI guidelines

- Follow egui's immediate-mode pattern — no retained widget state outside of `DawApp`.
- Each panel/view gets its own module with a `show()` or `ui()` method.
- Use the project's theme colors (defined in `main.rs` `setup_theme()`), not hardcoded values.

## Reporting bugs

Open an issue with:
- Steps to reproduce
- Expected vs. actual behavior
- OS, audio interface, and Rust version
- Any crash output or error messages

## License

By contributing, you agree that your contributions will be licensed under the [GPL-3.0-or-later](LICENSE) license.
