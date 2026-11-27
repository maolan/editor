# Maolan Edit

[![crates.io](https://img.shields.io/crates/v/maolan-editor.svg)](https://crates.io/crates/maolan-editor)

Maolan Edit is a standalone waveform editor built on Maolan's shared Rust audio
stack. It opens audio, provides non-destructive preview edits and undo/redo,
and exports rendered audio. Work is held in memory; saving writes audio and
does not create a reopenable editing project.

## Features

- Open audio formats supported by Maolan's shared decode path.
- View multichannel waveforms, select ranges, zoom, scroll, and audition audio.
- Cut, copy, paste, delete, trim to selection, normalize, fade, adjust gain,
  reverse, and undo or redo edits.
- Optionally snap selection endpoints to nearby zero crossings with `S`.
- Add markers manually or detect them from silence, then export marker ranges
  as separate files.
- Bound waveform display memory for long files with a min/max envelope. Audio
  samples remain full resolution for editing and playback.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Space | Play or stop |
| `Z` | Move to next zero crossing |
| `S` | Toggle zero crossing snap |
| Delete | Delete selection |
| Ctrl/Cmd + C, X, V | Copy, cut, paste |
| Ctrl/Cmd + T | Trim to selection |
| Ctrl/Cmd + N | Normalize selection |
| Ctrl/Cmd + Z / Shift + Ctrl/Cmd + Z | Undo / redo |

## Audio I/O

Opening and saving use Maolan's shared OxideAV-backed audio codecs:

- WAV (32-bit float for ordinary save; configurable bit depth for marker export)
- FLAC and Ogg FLAC
- MP3

## Build and run

Run commands from this directory:

```bash
cargo run
cargo check
cargo test --all-targets
```

The editor is a standalone Cargo crate and uses the sibling Maolan engine and
widgets crates.
