# Maolan Edit

[![crates.io](https://img.shields.io/crates/v/maolan-editor.svg)](https://crates.io/crates/maolan-editor)

Maolan Edit is a small audio editor built on Maolan's shared Rust audio stack.
It is intended to grow into a waveform editor in the spirit of classic tools
such as Sound Forge or Audacity, while reusing Maolan's codecs and UI widgets.

The current version is deliberately minimal:

- Open audio files supported by Maolan's decode path.
- Display the decoded waveform.
- Save the loaded audio using Maolan's OxideAV-backed export path.
- No editing tools yet.

## Supported Audio I/O

Opening uses `maolan_engine::audio_codec::decode_audio_to_f32_interleaved_sync`,
which is backed by Symphonia. The open dialog includes:

- WAV
- FLAC
- MP3
- Ogg / Vorbis
- M4A / AAC / ALAC

Saving uses `maolan_engine::audio_codec::encode_audio_to_file`, which uses the
same OxideAV-backed encoders as Maolan:

- `*.wav` - 32-bit float WAV
- `*.flac` - 24-bit FLAC
- `*.ogg` - 24-bit Ogg FLAC
- `*.mp3` - MP3

Saving to other extensions is rejected.

## Build And Run

This repository is a standalone Cargo crate. Build and run commands should be
executed from this directory:

```bash
cargo run
```

Check and test:

```bash
cargo check
cargo test --all-targets
```

Before finishing changes, run:

```bash
cargo clippy --all-targets --fix --allow-dirty
cargo fmt
```

## Repository Layout

```text
.
├── Cargo.toml
├── README.md
├── AGENTS.md
└── src/
    └── main.rs
```
