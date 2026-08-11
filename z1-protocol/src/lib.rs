//! Korg Z1 MIDI protocol.
//!
//! Source of truth: `specs/korg_z1_midi.txt` (official Korg Z1 MIDI
//! Implementation Rev 1.0). Pure codec, no I/O.

pub mod sysex;

pub use sysex::{
    device_inquiry_request, pack_7in8, parse_device_inquiry_reply, unpack_7in8, Bank, Channel,
    DecodeError, DeviceInfo, DumpScope, Group, Message, ParamId, ProgramNo, Value,
};
