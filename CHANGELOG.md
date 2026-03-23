# Changelog

All notable changes to ThroughWaves will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Full multi-track DAW with timeline, mixer, and session (clip launcher) views
- Low-latency audio I/O via cpal with configurable device selection
- Real-time parallel mixing with per-track volume, pan, mute, solo
- 15 built-in audio effects: Gain, Low Pass, High Pass, Delay, Reverb, Compressor, Chorus, Distortion, Limiter, Gate, Phaser, Flanger, Tremolo, EQ Band, Parametric EQ
- 8-band parametric EQ with interactive frequency response visualization
- VST3 plugin hosting with native editor windows (macOS NSView embedding)
- VSTi instrument hosting with MIDI-to-audio processing
- Plugin Delay Compensation (PDC) across the entire signal chain
- Audio recording with punch-in/out, count-in, and input monitoring
- MIDI recording from hardware controllers with real-time note capture
- Piano roll editor with velocity and CC lanes
- Built-in synthesizer (Saw/Sine/Square/Triangle, ADSR, filter)
- Export to WAV (16/24/32-bit), FLAC (16/24-bit), and AIFF (16/24/32-bit)
- Stem export — render each track to a separate file
- Track bouncing with progress reporting and cancellation
- Mip-mapped waveform display (5 resolution levels)
- Clip splitting, duplicating, copy/paste, nudge, consolidation
- Multi-clip selection with rubber-band (marquee) tool
- Take lanes with swipe comping and flatten
- Ripple editing mode
- Snap-to-grid with 8 modes (Free, 1/2 Beat, Triplet, Beat, 1/16, 1/32, Bar, Marker)
- 5 crossfade curve types (Linear, Exponential, Logarithmic, S-Curve, Equal Power)
- Overlap-Add (OLA) time-stretching with pitch preservation
- 50-level undo/redo with history browser
- Flexible send routing (pre/post-fader) to any track
- Track grouping/folders with collapse/expand, group mute/solo
- Track freeze for CPU optimization
- Sidechain compression with per-track routing
- Full effect parameter automation with interpolated curves
- Master bus effect chain
- MIDI CC learn and mapping to any parameter
- 8 assignable macro controls
- EBU R128 LUFS loudness metering (momentary, short-term, integrated)
- True peak metering with 4x oversampled interpolation
- Stereo phase correlation meter
- Real-time FFT spectrum analyzer
- Reference track A/B comparison with loudness matching
- Chord detection overlay on timeline clips
- AI stem separation via Demucs (vocals, drums, bass, other)
- Live jam sessions via WebSocket with shared BPM sync and chat
- Git-style version branching for project snapshots
- Web platform with track uploads, streaming, and social features
- Project and track templates, FX presets
- Autosave with recovery
- Tap tempo with beat flash indicator
- Metronome with configurable count-in
- macOS .app bundle with proper Info.plist
- Cross-platform CI/CD: macOS universal binary DMG, Windows ZIP, Linux .deb + tar.gz
- Custom dark theme with multiple variants (Dark, Darker, Midnight)
