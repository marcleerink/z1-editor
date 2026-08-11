# z1-editor

Rust plugin (`z1-vst`) that edits a **Korg Z1** hardware synth over MIDI SysEx
from inside any DAW — a friendly GUI replacing the Z1's tedious onboard menus, bidirectionally
synced with the hardware. Prior art (Korg's classic-Mac editor, "Z1 Editor 2004",
Patch Base) is standalone-only and/or closed; living in the DAW, free and open,
is this project's point.

## Constraints (apply to all work in this repo)

- **Host-agnostic core**: no feature may rely on host MIDI routing or
  host-specific behavior. The plugin does its own MIDI I/O via `midir`
  (CoreMIDI on macOS) on a background thread. This is mandatory for Ableton,
  which passes no SysEx to/from plugins, and is what makes the plugin universal.
- **Plugin type**: audio effect with pass-through, no DSP. The DAW audio thread
  stays untouched; all work happens on GUI and MIDI threads (channel messaging
  between them).
- **Framework**: nih-plug as a **git dependency** (crates.io release is
  outdated) + `nih_plug_egui` for the GUI.
## Protocol

Source of truth: [specs/korg_z1_midi.txt](specs/korg_z1_midi.txt) — text
extraction of the official Korg Z1 MIDI Implementation Rev 1.0 (original PDF:
https://www.deepsonic.ch/deep/docs_manuals/korg_z1_v1.0_midi.pdf). Read it
before writing protocol code; transcribe parameter IDs, ranges, and
bit-packing from it — never guess. Where to look:

- Sections 1-4 / 2-2: SysEx header format and function IDs (parameter change
  `41h`, dump request `10h`, current program dump `40h`, ACK/NAK replies)
- Dump footnote `[*1]`: Korg 7-in-8 byte packing for bulk data
- "Program Parameters" table (second half of the doc): every parameter's ID,
  range, display values, and size/bit-packing

Our design on top of the spec: connect flow is Device Inquiry (`F0 7E …`) to
confirm a Z1 and learn its channel → dump request → populate UI. Bidirectional
sync requires the Z1's Global settings SysEx Transmit + Receive ON (memory
protect OFF for writes).

## Architecture

```
z1-protocol/      # pure codec + static param table — no I/O, unit-tested
z1-vst/           # nih-plug plugin: egui GUI, midir manager, state
specs/            # Korg Z1 MIDI implementation (source of truth)
```

- Parameter model: static `&[ParamDef]` (id, name, range, display/bit-packing)
  transcribed from the spec; adding parameter groups is data entry, not code.
- DAW integration: a curated subset (~32) of params also exposed as automatable
  host parameters; GUI edits and automation feed the same debounced,
  rate-limited SysEx send path. Current patch serialized into plugin state so
  the DAW session recalls it (push-to-Z1 on load).

## DAW coverage

| Format | Produced by | Covers |
|---|---|---|
| VST3 | nih-plug xtask | Ableton Live (primary test bench), Cubase, Studio One, FL Studio, Reaper, Bitwig |
| CLAP | same build, free | Bitwig, Reaper, FL Studio |
| AU | [clap-wrapper](https://github.com/free-audio/clap-wrapper) around the CLAP | Logic Pro, GarageBand |
| Standalone | same build, free | dev/test harness, no DAW needed |
| AAX | — | Pro Tools: out of scope (Avid agreement + PACE signing) |

macOS first; `midir` abstracts CoreMIDI/WinMM/ALSA so Windows/Linux are a later
CI build-matrix task, not a redesign.

## Milestones

1. **Scaffold + loads in Live** — workspace builds; `.vst3` loads in Ableton
   with a GUI showing the MIDI port picker; Device Inquiry round-trip with the
   Z1 works (standalone for fast iteration)
2. **Protocol crate** — encode/decode 41h and 10h/40h incl. 7↔8 packing; unit
   tests from spec byte fixtures; validate dump offsets against a real dump early
3. **Parameter model, v1 subset** (~100 params) — Filter 1/2, EG1–4 + Amp EG,
   LFO1–4, mixer, effect sends, program common
4. **Editor UI** — egui panels in signal-flow order; edits → 41h;
   dump-on-connect populates UI; incoming 41h moves controls
5. **DAW integration** — automatable host params → rate-limited SysEx; session
   recall with push-to-Z1 on load; `.syx` save/load
6. **Universal packaging** — AU via clap-wrapper; smoke-test matrix (Live,
   Logic, Reaper/Bitwig); later Windows/Linux CI

## Build & test

- `cargo test -p z1-protocol` — codec round-trips (especially 7↔8 packing edges)
- `cargo xtask bundle z1-vst --release` → `.vst3`/`.clap`; copy to
  `~/Library/Audio/Plug-Ins/VST3/` (may need ad-hoc `codesign -s -`)
- Hardware verification: connect → dump populates UI → move a slider, change is
  audible on the Z1 → turn a Z1 knob, UI follows. Inspect traffic with macOS
  MIDI Monitor.
