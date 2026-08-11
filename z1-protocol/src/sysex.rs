//! Korg Z1 SysEx message codec.
//!
//! Byte layouts follow the tables in `specs/korg_z1_midi.txt` exactly
//! (sections 1-3, 1-4 and 2-2). This module does no I/O: callers pass in and
//! receive complete SysEx byte strings.

use core::fmt;

/// Korg's MIDI manufacturer ID (second byte of the exclusive header).
pub const KORG_ID: u8 = 0x42;
/// The Z1's family ID (fourth byte of the exclusive header).
pub const Z1_ID: u8 = 0x46;

const SYSEX_START: u8 = 0xF0;
const SYSEX_END: u8 = 0xF7;
/// High nibble of the header's third byte (`3g`, g = global channel).
const FORMAT_HIGH: u8 = 0x30;

/// A MIDI channel, 0–15 — the Z1's "global channel" from its MIDI page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channel(u8);

impl Channel {
    /// `None` unless `n` is 0–15.
    #[must_use]
    pub const fn new(n: u8) -> Option<Self> {
        if n < 16 {
            Some(Self(n))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Parameter group of a Parameter Change (function 41h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    GlobalMidi,
    Program,
    Pattern,
    Multi,
}

impl Group {
    const fn code(self) -> u8 {
        match self {
            Self::GlobalMidi => 0,
            Self::Program => 1,
            Self::Pattern => 2,
            Self::Multi => 3,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::GlobalMidi),
            1 => Some(Self::Program),
            2 => Some(Self::Pattern),
            3 => Some(Self::Multi),
            _ => None,
        }
    }
}

/// A 14-bit parameter ID (0–16383), sent as two 7-bit bytes LSB-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamId(u16);

impl ParamId {
    /// `None` unless `id` fits in 14 bits.
    #[must_use]
    pub const fn new(id: u16) -> Option<Self> {
        if id < 0x4000 {
            Some(Self(id))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A 14-bit signed parameter value (−8192–8191), sent as two 7-bit bytes
/// LSB-first in two's complement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Value(i16);

impl Value {
    pub const MIN: i16 = -0x2000;
    pub const MAX: i16 = 0x1FFF;

    /// `None` unless `v` fits in signed 14 bits.
    #[must_use]
    pub const fn new(v: i16) -> Option<Self> {
        if v >= Self::MIN && v <= Self::MAX {
            Some(Self(v))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    fn to_raw14(self) -> u16 {
        u16::try_from(i32::from(self.0) & 0x3FFF).unwrap_or(0)
    }

    fn from_raw14(raw: u16) -> Self {
        // Branch-free 14-bit sign extension: (x ^ 0x2000) - 0x2000.
        let signed = (i32::from(raw & 0x3FFF) ^ 0x2000).wrapping_sub(0x2000);
        Self(i16::try_from(signed).unwrap_or(0))
    }
}

/// A program slot number, 0–127.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgramNo(u8);

impl ProgramNo {
    /// `None` unless `n` is 0–127.
    #[must_use]
    pub const fn new(n: u8) -> Option<Self> {
        if n < 128 {
            Some(Self(n))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Program bank A or B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bank {
    A,
    B,
}

impl Bank {
    const fn code(self) -> u8 {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::A),
            1 => Some(Self::B),
            _ => None,
        }
    }
}

/// What a Program Data Dump Request (function 1Ch) asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DumpScope {
    /// One program.
    Single { bank: Bank, program: ProgramNo },
    /// One full bank.
    Bank(Bank),
    /// Both banks.
    All,
}

impl DumpScope {
    /// Encode to the `00uu000b` unit/bank byte and program byte of the spec.
    const fn unit_and_program(self) -> (u8, u8) {
        match self {
            Self::Single { bank, program } => (bank.code(), program.get()),
            Self::Bank(bank) => (0x10 | bank.code(), 0),
            Self::All => (0x20, 0),
        }
    }

    fn decode(unit_bank: u8, program: u8) -> Option<Self> {
        let bank = Bank::from_code(unit_bank & 0x01);
        match unit_bank & 0x30 {
            0x00 => Some(Self::Single {
                bank: bank?,
                program: ProgramNo::new(program)?,
            }),
            0x10 => Some(Self::Bank(bank?)),
            0x20 => Some(Self::All),
            _ => None,
        }
    }
}

/// A Z1-exclusive SysEx message (`F0 42 3g 46 … F7`).
///
/// Function IDs and payload layouts are from spec sections 1-4 / 2-2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Function 41h — one parameter edit, in either direction.
    ParameterChange {
        group: Group,
        id: ParamId,
        value: Value,
    },
    /// Function 10h — ask for the edit buffer; the Z1 answers with
    /// [`Message::CurrentProgramDump`].
    CurrentProgramDumpRequest,
    /// Function 1Ch — ask for stored programs; the Z1 answers with a
    /// Program Data Dump (function 4Ch, not yet modeled).
    ProgramDumpRequest(DumpScope),
    /// Function 11h — write the edit buffer to a program slot.
    ProgramWriteRequest { bank: Bank, program: ProgramNo },
    /// Function 40h — the edit buffer. `data` is the unpacked payload
    /// (8-bit bytes; the 7-in-8 wire packing is handled by the codec).
    CurrentProgramDump(Vec<u8>),
    /// Function 26h.
    DataFormatError,
    /// Function 23h — ACK for received data.
    DataLoadCompleted,
    /// Function 24h — NAK for received data.
    DataLoadError,
    /// Function 21h — ACK for a write request.
    WriteCompleted,
    /// Function 22h — NAK for a write request.
    WriteError,
}

impl Message {
    /// Encode to a complete SysEx byte string for the given global channel.
    #[must_use]
    pub fn encode(&self, channel: Channel) -> Vec<u8> {
        [SYSEX_START, KORG_ID, FORMAT_HIGH | channel.get(), Z1_ID]
            .into_iter()
            .chain(self.function_and_payload())
            .chain([SYSEX_END])
            .collect()
    }

    fn function_and_payload(&self) -> Vec<u8> {
        match self {
            Self::ParameterChange { group, id, value } => {
                let (id_l, id_m) = split14(id.get());
                let (v_l, v_m) = split14(value.to_raw14());
                vec![0x41, group.code(), id_l, id_m, v_l, v_m]
            }
            Self::CurrentProgramDumpRequest => vec![0x10, 0x00],
            Self::ProgramDumpRequest(scope) => {
                let (unit_bank, program) = scope.unit_and_program();
                vec![0x1C, unit_bank, program, 0x00]
            }
            Self::ProgramWriteRequest { bank, program } => {
                vec![0x11, bank.code(), program.get()]
            }
            Self::CurrentProgramDump(data) => {
                [0x40, 0x01].into_iter().chain(pack_7in8(data)).collect()
            }
            Self::DataFormatError => vec![0x26],
            Self::DataLoadCompleted => vec![0x23],
            Self::DataLoadError => vec![0x24, 0x00],
            Self::WriteCompleted => vec![0x21],
            Self::WriteError => vec![0x22, 0x00],
        }
    }

    /// Decode a complete SysEx byte string.
    ///
    /// Returns the global channel from the header alongside the message.
    ///
    /// # Errors
    ///
    /// [`DecodeError::NotSysEx`] / [`DecodeError::NotKorgZ1`] when the
    /// framing or header identifies some other message; [`DecodeError::
    /// Truncated`], [`DecodeError::UnknownFunction`] or
    /// [`DecodeError::Malformed`] when it is Z1-shaped but does not match
    /// the spec's layout; [`DecodeError::HighBitInData`] when dump data
    /// contains bytes with the top bit set.
    pub fn decode(bytes: &[u8]) -> Result<(Channel, Self), DecodeError> {
        match bytes {
            [SYSEX_START, KORG_ID, format, Z1_ID, function, payload @ .., SYSEX_END] => {
                let channel = Channel::new(*format & 0x0F)
                    .filter(|_| *format & 0xF0 == FORMAT_HIGH)
                    .ok_or(DecodeError::NotKorgZ1)?;
                Ok((channel, Self::decode_function(*function, payload)?))
            }
            [SYSEX_START, KORG_ID, _, Z1_ID, ..] => Err(DecodeError::Truncated),
            [SYSEX_START, ..] => Err(DecodeError::NotKorgZ1),
            _ => Err(DecodeError::NotSysEx),
        }
    }

    fn decode_function(function: u8, payload: &[u8]) -> Result<Self, DecodeError> {
        match (function, payload) {
            (0x41, [group, id_l, id_m, v_l, v_m]) => Ok(Self::ParameterChange {
                group: Group::from_code(*group).ok_or(DecodeError::Malformed)?,
                id: ParamId::new(u14(*id_l, *id_m)).ok_or(DecodeError::Malformed)?,
                value: Value::from_raw14(u14(*v_l, *v_m)),
            }),
            (0x10, [0x00]) => Ok(Self::CurrentProgramDumpRequest),
            (0x1C, [unit_bank, program, 0x00]) => DumpScope::decode(*unit_bank, *program)
                .map(Self::ProgramDumpRequest)
                .ok_or(DecodeError::Malformed),
            (0x11, [bank, program]) => Ok(Self::ProgramWriteRequest {
                bank: Bank::from_code(*bank).ok_or(DecodeError::Malformed)?,
                program: ProgramNo::new(*program).ok_or(DecodeError::Malformed)?,
            }),
            (0x40, [0x01, data @ ..]) => Ok(Self::CurrentProgramDump(unpack_7in8(data)?)),
            (0x26, []) => Ok(Self::DataFormatError),
            (0x23, []) => Ok(Self::DataLoadCompleted),
            (0x24, [0x00]) => Ok(Self::DataLoadError),
            (0x21, []) => Ok(Self::WriteCompleted),
            (0x22, [0x00]) => Ok(Self::WriteError),
            (0x41 | 0x10 | 0x1C | 0x11 | 0x40 | 0x26 | 0x23 | 0x24 | 0x21 | 0x22, _) => {
                Err(DecodeError::Malformed)
            }
            (other, _) => Err(DecodeError::UnknownFunction(other)),
        }
    }
}

/// Why a byte string failed to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Does not start with `F0` / end with `F7`.
    NotSysEx,
    /// Valid SysEx, but not a Korg Z1 exclusive header.
    NotKorgZ1,
    /// Z1 header without a complete function/EOX.
    Truncated,
    /// A function ID the spec does not define (or one not yet modeled).
    UnknownFunction(u8),
    /// Known function, payload does not match the spec's layout.
    Malformed,
    /// Dump data contained a byte with the top bit set.
    HighBitInData,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSysEx => f.write_str("not a SysEx message"),
            Self::NotKorgZ1 => f.write_str("not a Korg Z1 exclusive message"),
            Self::Truncated => f.write_str("truncated Z1 message"),
            Self::UnknownFunction(id) => write!(f, "unknown Z1 function {id:#04x}"),
            Self::Malformed => f.write_str("payload does not match the spec layout"),
            Self::HighBitInData => f.write_str("dump data byte has the high bit set"),
        }
    }
}

impl core::error::Error for DecodeError {}

/// The Z1's reply to a Universal Device Inquiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The global channel the Z1 answered on — use it for all further
    /// [`Message::encode`] calls.
    pub channel: Channel,
    pub major_version: u16,
    pub minor_version: u16,
}

/// Universal Device Inquiry request (`F0 7E 0g 06 01 F7`).
#[must_use]
pub fn device_inquiry_request(channel: Channel) -> Vec<u8> {
    vec![SYSEX_START, 0x7E, channel.get(), 0x06, 0x01, SYSEX_END]
}

/// Parse a Device Inquiry reply; `Some` only if it identifies a Korg Z1.
#[must_use]
pub fn parse_device_inquiry_reply(bytes: &[u8]) -> Option<DeviceInfo> {
    match bytes {
        [SYSEX_START, 0x7E, ch, 0x06, 0x02, KORG_ID, Z1_ID, 0x00, 0x01, 0x00, min_l, min_m, maj_l, maj_m, SYSEX_END] => {
            Some(DeviceInfo {
                channel: Channel::new(*ch)?,
                major_version: u14(*maj_l, *maj_m),
                minor_version: u14(*min_l, *min_m),
            })
        }
        _ => None,
    }
}

/// Pack 8-bit data into the Z1's 7-in-8 dump format: each group of up to
/// 7 data bytes is preceded by a byte carrying their top bits (bit *i* of
/// the lead byte = bit 7 of the *i*-th following byte).
#[must_use]
pub fn pack_7in8(data: &[u8]) -> Vec<u8> {
    data.chunks(7)
        .flat_map(|chunk| {
            let lead = chunk
                .iter()
                .enumerate()
                .filter(|(_, byte)| *byte & 0x80 != 0)
                .fold(0u8, |acc, (i, _)| acc | bit(i));
            core::iter::once(lead).chain(chunk.iter().map(|byte| byte & 0x7F))
        })
        .collect()
}

/// Reverse of [`pack_7in8`].
///
/// # Errors
///
/// [`DecodeError::HighBitInData`] if any wire byte has its top bit set —
/// packed dump data must be pure 7-bit.
pub fn unpack_7in8(wire: &[u8]) -> Result<Vec<u8>, DecodeError> {
    wire.chunks(8)
        .map(unpack_chunk)
        .collect::<Result<Vec<_>, _>>()
        .map(|chunks| chunks.concat())
}

fn unpack_chunk(chunk: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let (lead, rest) = chunk.split_first().ok_or(DecodeError::Malformed)?;
    let lead = seven_bit(*lead)?;
    rest.iter()
        .enumerate()
        .map(|(i, &byte)| {
            seven_bit(byte).map(|low| low | u8::from(lead & bit(i) != 0).wrapping_shl(7))
        })
        .collect()
}

const fn seven_bit(byte: u8) -> Result<u8, DecodeError> {
    if byte & 0x80 == 0 {
        Ok(byte)
    } else {
        Err(DecodeError::HighBitInData)
    }
}

fn bit(i: usize) -> u8 {
    1u8.wrapping_shl(u32::try_from(i).unwrap_or(0))
}

fn low7(v: u16) -> u8 {
    u8::try_from(v & 0x7F).unwrap_or(0)
}

fn split14(v: u16) -> (u8, u8) {
    (low7(v), low7(v.wrapping_shr(7)))
}

fn u14(lsb: u8, msb: u8) -> u16 {
    u16::from(msb & 0x7F).wrapping_shl(7) | u16::from(lsb & 0x7F)
}

/// Byte fixtures are hand-built from the tables in `specs/korg_z1_midi.txt`;
/// each test cites its table.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::arithmetic_side_effects,
    clippy::as_conversions
)]
mod tests {
    use super::*;

    fn ch(n: u8) -> Channel {
        Channel::new(n).unwrap()
    }

    #[test]
    fn parameter_change_round_trip() {
        // Spec (20): F0 42 3g 46 41 <group> <idL> <idM> <valL> <valM> F7.
        // Filter1 Cutoff is program parameter 263 = 0x107 -> LSB 07, MSB 02.
        let msg = Message::ParameterChange {
            group: Group::Program,
            id: ParamId::new(263).unwrap(),
            value: Value::new(50).unwrap(),
        };
        let bytes = msg.encode(ch(0));
        assert_eq!(
            bytes,
            [0xF0, 0x42, 0x30, 0x46, 0x41, 0x01, 0x07, 0x02, 0x32, 0x00, 0xF7]
        );
        assert_eq!(Message::decode(&bytes).unwrap(), (ch(0), msg));
    }

    #[test]
    fn negative_values_use_14_bit_twos_complement() {
        // -99 in 14-bit two's complement is 0x3F9D -> LSB 0x1D, MSB 0x7F.
        let msg = Message::ParameterChange {
            group: Group::Program,
            id: ParamId::new(31).unwrap(), // EG1 Start Level, -99..+99
            value: Value::new(-99).unwrap(),
        };
        let bytes = msg.encode(ch(3));
        assert_eq!(
            bytes,
            [0xF0, 0x42, 0x33, 0x46, 0x41, 0x01, 0x1F, 0x00, 0x1D, 0x7F, 0xF7]
        );
        let (channel, decoded) = Message::decode(&bytes).unwrap();
        assert_eq!(channel, ch(3));
        assert_eq!(decoded, msg);
    }

    #[test]
    fn current_program_dump_request_layout() {
        // Spec (1): F0 42 3g 46 10 00 F7.
        let bytes = Message::CurrentProgramDumpRequest.encode(ch(0));
        assert_eq!(bytes, [0xF0, 0x42, 0x30, 0x46, 0x10, 0x00, 0xF7]);
        assert_eq!(
            Message::decode(&bytes).unwrap().1,
            Message::CurrentProgramDumpRequest
        );
    }

    #[test]
    fn program_dump_request_scopes() {
        // Spec (2): F0 42 3g 46 1C <00uu000b> <prog> 00 F7.
        let single = Message::ProgramDumpRequest(DumpScope::Single {
            bank: Bank::B,
            program: ProgramNo::new(5).unwrap(),
        });
        assert_eq!(
            single.encode(ch(0)),
            [0xF0, 0x42, 0x30, 0x46, 0x1C, 0x01, 0x05, 0x00, 0xF7]
        );

        let bank = Message::ProgramDumpRequest(DumpScope::Bank(Bank::A));
        assert_eq!(
            bank.encode(ch(0)),
            [0xF0, 0x42, 0x30, 0x46, 0x1C, 0x10, 0x00, 0x00, 0xF7]
        );

        let all = Message::ProgramDumpRequest(DumpScope::All);
        assert_eq!(
            all.encode(ch(0)),
            [0xF0, 0x42, 0x30, 0x46, 0x1C, 0x20, 0x00, 0x00, 0xF7]
        );

        for msg in [single, bank, all] {
            let bytes = msg.encode(ch(9));
            assert_eq!(Message::decode(&bytes).unwrap(), (ch(9), msg));
        }
    }

    #[test]
    fn program_write_request_layout() {
        // Spec (9): F0 42 3g 46 11 <bank> <prog> F7.
        let msg = Message::ProgramWriteRequest {
            bank: Bank::A,
            program: ProgramNo::new(127).unwrap(),
        };
        let bytes = msg.encode(ch(15));
        assert_eq!(bytes, [0xF0, 0x42, 0x3F, 0x46, 0x11, 0x00, 0x7F, 0xF7]);
        assert_eq!(Message::decode(&bytes).unwrap(), (ch(15), msg));
    }

    #[test]
    fn acknowledgement_messages() {
        // Spec (21)-(25).
        let cases: [(Message, &[u8]); 5] = [
            (Message::DataFormatError, &[0x26]),
            (Message::DataLoadCompleted, &[0x23]),
            (Message::DataLoadError, &[0x24, 0x00]),
            (Message::WriteCompleted, &[0x21]),
            (Message::WriteError, &[0x22, 0x00]),
        ];
        for (msg, tail) in cases {
            let mut expected = vec![0xF0, 0x42, 0x30, 0x46];
            expected.extend_from_slice(tail);
            expected.push(0xF7);
            assert_eq!(msg.encode(ch(0)), expected);
            assert_eq!(Message::decode(&expected).unwrap(), (ch(0), msg));
        }
    }

    #[test]
    fn current_program_dump_round_trip() {
        // Spec (12): F0 42 3g 46 40 01 <7-in-8 packed data> F7.
        let payload: Vec<u8> = (0..=255u16).map(|b| (b % 256) as u8).collect();
        let msg = Message::CurrentProgramDump(payload.clone());
        let bytes = msg.encode(ch(1));
        assert_eq!(bytes[..6], [0xF0, 0x42, 0x31, 0x46, 0x40, 0x01]);
        assert!(bytes[6..bytes.len() - 1].iter().all(|b| b & 0x80 == 0));
        let (_, decoded) = Message::decode(&bytes).unwrap();
        assert_eq!(decoded, Message::CurrentProgramDump(payload));
    }

    #[test]
    fn pack_7in8_lead_byte_carries_top_bits() {
        // Spec dump-data footnote [*1]: bit i of the lead byte is bit 7 of
        // the i-th following byte.
        assert_eq!(pack_7in8(&[0x80]), [0x01, 0x00]);
        assert_eq!(pack_7in8(&[0x00, 0xFF]), [0x02, 0x00, 0x7F]);
        assert_eq!(
            pack_7in8(&[0xFF; 7]),
            [0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F]
        );
        assert_eq!(unpack_7in8(&[0x01, 0x00]).unwrap(), [0x80]);
    }

    #[test]
    fn pack_unpack_round_trips_all_lengths() {
        for len in 0..=32usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 % 256) as u8).collect();
            assert_eq!(
                unpack_7in8(&pack_7in8(&data)).unwrap(),
                data,
                "length {len}"
            );
        }
    }

    #[test]
    fn unpack_rejects_high_bits() {
        assert_eq!(unpack_7in8(&[0x80, 0x00]), Err(DecodeError::HighBitInData));
        assert_eq!(unpack_7in8(&[0x00, 0x80]), Err(DecodeError::HighBitInData));
    }

    #[test]
    fn decode_rejects_foreign_messages() {
        assert_eq!(Message::decode(&[]), Err(DecodeError::NotSysEx));
        assert_eq!(
            Message::decode(&[0x90, 0x3C, 0x64]),
            Err(DecodeError::NotSysEx)
        );
        // Roland manufacturer ID.
        assert_eq!(
            Message::decode(&[0xF0, 0x41, 0x30, 0x46, 0x10, 0x00, 0xF7]),
            Err(DecodeError::NotKorgZ1)
        );
        assert_eq!(
            Message::decode(&[0xF0, 0x42, 0x30, 0x46, 0x7F, 0xF7]),
            Err(DecodeError::UnknownFunction(0x7F))
        );
        assert_eq!(
            Message::decode(&[0xF0, 0x42, 0x30, 0x46, 0x41, 0x01, 0xF7]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn device_inquiry() {
        // Spec 1-3: request F0 7E 0g 06 01 F7; the reply identifies family
        // 46 00, member 01 00, then minor/major version pairs.
        assert_eq!(
            device_inquiry_request(ch(0)),
            [0xF0, 0x7E, 0x00, 0x06, 0x01, 0xF7]
        );
        let reply = [
            0xF0, 0x7E, 0x02, 0x06, 0x02, 0x42, 0x46, 0x00, 0x01, 0x00, 0x0A, 0x00, 0x01, 0x00,
            0xF7,
        ];
        let info = parse_device_inquiry_reply(&reply).unwrap();
        assert_eq!(info.channel, ch(2));
        assert_eq!(info.major_version, 1);
        assert_eq!(info.minor_version, 10);
        // A Yamaha reply must not match.
        let yamaha = [
            0xF0, 0x7E, 0x02, 0x06, 0x02, 0x43, 0x46, 0x00, 0x01, 0x00, 0x0A, 0x00, 0x01, 0x00,
            0xF7,
        ];
        assert!(parse_device_inquiry_reply(&yamaha).is_none());
    }

    #[test]
    fn newtype_bounds() {
        assert!(Channel::new(16).is_none());
        assert!(ParamId::new(0x4000).is_none());
        assert!(ProgramNo::new(128).is_none());
        assert!(Value::new(0x2000).is_none());
        assert!(Value::new(-0x2001).is_none());
        assert_eq!(Value::new(Value::MIN).unwrap().get(), -8192);
        assert_eq!(Value::new(Value::MAX).unwrap().get(), 8191);
    }

    #[test]
    fn wire_byte_helpers() {
        for v in [0u16, 1, 0x7F, 0x80, 263, 0x1FFF, 0x3FFF] {
            let (lsb, msb) = split14(v);
            assert!(lsb < 0x80 && msb < 0x80);
            assert_eq!(u14(lsb, msb), v, "value {v:#x}");
        }
        assert_eq!(Value::from_raw14(0x3F9D).get(), -99);
        assert_eq!(Value::new(-99).unwrap().to_raw14(), 0x3F9D);
    }
}
