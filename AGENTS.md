# z1-editor

Open-source **editor/librarian for the Korg Z1** (1997 physical-modeling synth,
MOSS engine) in Rust, running **inside any DAW** as a plugin (plus standalone).
In the spirit of Ctrlr, but architected so a config-driven multi-synth version
can grow out of it later — the Z1 is hand-coded first; do **not** design the
generic schema from this single data point.

Why: no current tool supports the Z1 — Korg's own editor is OS 9-era, "Z1
Editor 2004" is dead Windows software, Ctrlr/KnobKraft/Edisyn have no Z1
support, Patch Base is only "considering" it. The sole exception is the
commercial Midi Quest. And nothing runs as a DAW plugin — that is this
project's differentiator.

## Constraints (apply to all work in this repo)

- **Host-agnostic core**: no feature may rely on host MIDI routing or
  host-specific behavior. The plugin does its own MIDI I/O via `midir`
  (CoreMIDI on macOS) on a background thread. Mandatory for Ableton Live
  (the primary test bench), which passes no SysEx to/from plugins.
- **Plugin type**: audio effect with pass-through, no DSP. The DAW audio
  thread stays untouched; all work happens on GUI and MIDI threads.
- **Sequencing**: librarian core (connect, dump, save/send, patch list)
  before the full per-parameter editor UI.
- **The hard 20%**: the Z1's oscillator parameters change meaning depending
  on which of its 13 oscillator models is selected. UI and data model must
  support conditional/variant parameter groups, never assume a flat list.
  (The generated `Section` enum in `z1-protocol` carries one section per
  oscillator model for exactly this reason.)
- **License hygiene**: GPL/AGPL projects (Ctrlr, gearmulator, KnobKraft,
  Edisyn) are read-for-understanding and screenshot reference **only** —
  never copy code.

## Protocol

Source of truth: [specs/korg_z1_midi.txt](specs/korg_z1_midi.txt) — text
extraction of the official Korg Z1 MIDI Implementation Rev 1.0 (original PDF:
https://www.deepsonic.ch/deep/docs_manuals/korg_z1_v1.0_midi.pdf). This is a
documented spec, not reverse engineering. Read it before writing protocol
code; transcribe parameter IDs, ranges, and bit-packing from it — never
guess. Where to look:

- Sections 1-4 / 2-2: SysEx header format and function IDs (parameter change
  `41h`, dump request `10h`, current program dump `40h`, ACK/NAK replies)
- Dump footnote `[*1]`: Korg 7-in-8 byte packing for bulk data
- "Program Parameters" tables (second half): every parameter's ID, range,
  display values, size/bit-packing — including the per-oscillator-model and
  per-effect-type overlay tables and the Mod Source / footnote enum lists

Our design on top of the spec: connect flow is Device Inquiry (`F0 7E …`) to
confirm a Z1 and learn its channel → dump request → populate UI. Bidirectional
sync requires the Z1's Global settings SysEx Transmit + Receive ON (memory
protect OFF for writes).

## Architecture

```
z1-protocol/      # pure library, no I/O, unit-tested:
                  #   sysex codec + declarative parameter table (params.rs)
z1-vst/           # plugin crate: egui GUI, midir manager, state, host params
specs/            # Korg Z1 MIDI implementation (source of truth)
```

- `z1-protocol/src/params.rs` will hold the full program parameter table
  (all oscillator-model and effect overlays), transcribed from the spec.
  The spec is frozen 1997 hardware — the table is maintained by hand, and
  every edit must be justified against the spec or verified hardware
  behavior, with invariants enforced by tests (ordering,
  label-count-equals-range, mod-source bounds).
- DAW integration: a curated subset (~32) of params also exposed as
  automatable host parameters; GUI edits and automation feed the same
  debounced, rate-limited SysEx send path. Current patch serialized into
  plugin state so the DAW session recalls it (push-to-Z1 on load).
- Patch storage (librarian): `.syx` files first; `serde`/JSON or `rusqlite`
  library later if browsing needs it.
- Future multi-synth split (`sysex-core` / `synth-config` / generic UI
  walker) is deferred until a second synth's quirks are on the table.

## Plugin framework: nih-plug vs truce (decide in milestone 1)

- **nih-plug** (git dep; crates.io release is outdated) + `nih_plug_egui`:
  battle-tested, VST3 + CLAP + standalone; AU needs
  [clap-wrapper](https://github.com/free-audio/clap-wrapper).
- **[truce](https://github.com/truce-audio/truce)**: younger but real
  (~1.9k commits); VST3 + CLAP default, native AU v2/v3, AAX, LV2, VST2;
  egui backend; **`cargo truce screenshot`** renders the editor to PNG
  headlessly — automates the UI-iteration loop (agents here can compile and
  test but not render a GUI; otherwise the loop is: agent writes code → user
  builds and screenshots → agent adjusts).
- Milestone 1 builds the same minimal scaffold in whichever looks best after
  a spike; truce's screenshot tooling and native AU are the tiebreaker
  arguments, nih-plug's maturity is the counterargument.

## DAW coverage

| Format | Covers |
|---|---|
| VST3 | Ableton Live (primary test bench), Cubase, Studio One, FL Studio, Reaper, Bitwig |
| CLAP | Bitwig, Reaper, FL Studio |
| AU | Logic Pro, GarageBand (truce native, or clap-wrapper around the CLAP) |
| Standalone | dev/test harness, no DAW needed |
| AAX | Pro Tools — only if truce is adopted; otherwise out of scope |

macOS first; `midir` abstracts CoreMIDI/WinMM/ALSA so Windows/Linux are a
later CI build-matrix task, not a redesign.

## Milestones

1. **Scaffold + loads in Live** — framework spike (nih-plug vs truce);
   `.vst3` loads in Ableton with a GUI showing the MIDI port picker; Device
   Inquiry round-trip with the Z1 works
2. **Protocol crate** — encode/decode 41h and 10h/40h incl. 7↔8 packing;
   parameter table; unit tests from spec byte fixtures; validate dump
   offsets against a real dump early
3. **Librarian core** — connect, request/receive dumps, save/send `.syx`,
   patch list
4. **Editor UI, v1 subset** (~100 params) — Filter 1/2, EG1–4 + Amp EG,
   LFO1–4, mixer, effect sends, program common; egui panels in signal-flow
   order; edits → 41h; incoming 41h moves controls
5. **DAW integration** — automatable host params → rate-limited SysEx;
   session recall with push-to-Z1 on load
6. **Oscillator-model panels** — the conditional/variant UI for all 13
   oscillator models (the hard 20%)
7. **Universal packaging** — AU (and AAX if truce); smoke-test matrix (Live,
   Logic, Reaper/Bitwig); later Windows/Linux CI

## Build & test

- CI (`.github/workflows/ci.yml`) gates every push/PR on exactly:
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` — run these locally before pushing
- `cargo test -p z1-protocol` — codec round-trips and parameter-table
  invariants
- Bundle: `cargo xtask bundle z1-vst --release` (nih-plug) or `cargo truce`
  equivalents → `~/Library/Audio/Plug-Ins/VST3/` (may need ad-hoc
  `codesign -s -`)
- Hardware verification: connect → dump populates UI → move a slider, change
  is audible on the Z1 → turn a Z1 knob, UI follows. Inspect traffic with
  macOS MIDI Monitor.
