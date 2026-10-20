//! Skill-use handshake messages and buffs, as seen on official servers (WP2.1, 2026-10-20).
//!
//! In lockstep the client does not run a skill locally: it sends the request and waits for the
//! server's *acknowledgement*, which carries the authoritative (absolute) target. Every client
//! skill message has exactly one reply (verified over six official sessions: counts match
//! 1:1):
//!
//! | C→S | S→C reply |
//! |---|---|
//! | `0x01a5` UseSkill | `0x01ae` [`UseSkillAck`] |
//! | `0x01a8` UpdateSkillTarget | `0x01af` [`UpdateSkillTargetAck`] |
//! | `0x01aa` UpdateSkillTargetEntity | `0x01b0` [`UpdateSkillTargetEntityAck`] |
//! | `0x01ac` [`SkillInputEnd`] | `0x01b1` [`SkillInputEndAck`] |
//! | `0x01ad` SkillReleased | `0x01b2` [`SkillReleasedAck`] |
//!
//! The acknowledgements are Player-component simulation messages (handler `0x14171cc70`,
//! vtable `0x1433a7708` +0xc0): the client skips them only when its world is in mode 1
//! (predictive; world vfunc +0x1d8), which never happens in lockstep.

use poe_net::{DecodeError, Reader, Writer};

use crate::{Direction, Message, instance::SimulationHeader};

/// The parameter block shared by C→S `0x01a5` (after the `u8` player) and S→C `0x01ae`
/// (after the header): reader `0x141ecde00`, writer `0x141ecdd00`.
///
/// `u32 skill, u16 flags, u8 unk_13, u8 target_flags, [u32 target entity if target_flags & 8],
/// i32 x, i32 y`.
///
/// Client request: `x`/`y` are **relative to the player's cell** (absolute when a target entity
/// is given). Server acknowledgement: `x`/`y` are the **absolute grid cell** (the server's
/// player cell plus the relative offset, e.g. roll start `(371, 1052)` + `(15, -35)` =
/// `(386, 1017)`), `flags`/`unk_13` echo the request and `target_flags` echo it with bit `0x04`
/// set. `target_flags` bits seen: `0x01` / `0x02` / `0x10` (set on held repeats), `0x08` (target
/// entity), `0x04` (server-confirmed, absolute). Client handler `0x141dd4270`
/// (`Player_ApplyServerSkillUse`): with `0x04` set it only stores the block in the Player
/// component (+0x214, or +0x22c for the Move skill `0x20002909`) and marks it valid; without it
/// it also starts the skill locally; with `0x02` and a running action of the same skill it
/// cancels that action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillUseParams {
    pub skill: u32,
    pub flags: u16,
    pub unk_13: u8,
    pub target_flags: u8,
    pub target_entity: Option<u32>,
    pub x: i32,
    pub y: i32,
}

impl SkillUseParams {
    /// Bit in `target_flags` that says a target entity follows.
    pub const HAS_ENTITY: u8 = 0x08;
    /// Bit the server sets in the acknowledgement: `x`/`y` are absolute and already resolved.
    pub const CONFIRMED: u8 = 0x04;

    pub fn encode(&self, w: &mut Writer) {
        w.u32(self.skill).u16(self.flags).u8(self.unk_13);
        match self.target_entity {
            Some(id) => {
                w.u8(self.target_flags | Self::HAS_ENTITY).u32(id);
            }
            None => {
                w.u8(self.target_flags & !Self::HAS_ENTITY);
            }
        }
        w.i32(self.x).i32(self.y);
    }

    pub fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let skill = r.u32()?;
        let flags = r.u16()?;
        let unk_13 = r.u8()?;
        let target_flags = r.u8()?;
        let target_entity = if target_flags & Self::HAS_ENTITY != 0 {
            Some(r.u32()?)
        } else {
            None
        };
        Ok(Self {
            skill,
            flags,
            unk_13,
            target_flags,
            target_entity,
            x: r.i32()?,
            y: r.i32()?,
        })
    }
}

/// S→C `0x01ae`: acknowledgement of a C→S `UseSkill` (reader `0x142186800`): header, then
/// [`SkillUseParams`] with absolute coordinates and `target_flags | 0x04`.
///
/// Official ordering in the reply batch of a roll: `SprintState` (if the key is down),
/// `StartAction`, `StatsUpdate(is_dodge_rolling=1)`, **`UseSkillAck`**, `StatsUpdate(stun
/// thresholds)`. The header is `(player, 0, 0, u1a)` with `u1a` an ordering value (0 for most;
/// 4..10 right after other messages of the same batch).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UseSkillAck {
    pub header: SimulationHeader,
    pub params: SkillUseParams,
}

impl Message for UseSkillAck {
    const NAME: &'static str = "UseSkillAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        self.params.encode(w);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
            params: SkillUseParams::decode(r)?,
        })
    }
}

/// S→C `0x01af`: acknowledgement of C→S `UpdateSkillTarget` `0x01a8` (reader `0x141f6d520`):
/// header, `i32 x, i32 y` (absolute grid cell = the server's player cell + the relative offset
/// the client sent), `u32 skill`. Client handler `0x141dd43d0`
/// (`Player_ApplyServerSkillTarget`): stores the position in the pending skill slot whose skill
/// matches, then re-targets the running action of that skill (Actor vfunc +0x128). This is how
/// official servers steer a running roll or held skill; they never send `0x01cb`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateSkillTargetAck {
    pub header: SimulationHeader,
    pub x: i32,
    pub y: i32,
    pub skill: u32,
}

impl Message for UpdateSkillTargetAck {
    const NAME: &'static str = "UpdateSkillTargetAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        w.i32(self.x).i32(self.y).u32(self.skill);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
            x: r.i32()?,
            y: r.i32()?,
            skill: r.u32()?,
        })
    }
}

/// S→C `0x01b0`: acknowledgement of C→S `UpdateSkillTargetEntity` `0x01aa` (reader
/// `0x1421868e0`): header, `u32 entity, i32 x, i32 y` (the entity's absolute cell), `u32
/// skill`. Same client handler as [`UpdateSkillTargetAck`] with the entity set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateSkillTargetEntityAck {
    pub header: SimulationHeader,
    pub entity: u32,
    pub x: i32,
    pub y: i32,
    pub skill: u32,
}

impl Message for UpdateSkillTargetEntityAck {
    const NAME: &'static str = "UpdateSkillTargetEntityAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        w.u32(self.entity).i32(self.x).i32(self.y).u32(self.skill);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
            entity: r.u32()?,
            x: r.i32()?,
            y: r.i32()?,
            skill: r.u32()?,
        })
    }
}

/// C→S `0x01ac`: the player's held skill input ended (sender `0x14210fd40`): `u8 local
/// player`. Sent when the dodge/sprint key is released (together with `SprintInput` held=0)
/// and when an attack button is released. The client does not apply it itself in lockstep; it
/// waits for [`SkillInputEndAck`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillInputEnd {
    pub player: u8,
}

impl Message for SkillInputEnd {
    const NAME: &'static str = "SkillInputEnd";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.player);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self { player: r.u8()? })
    }
}

/// S→C `0x01b1`: acknowledgement of [`SkillInputEnd`] (reader `0x141f4b680`, header only).
/// Client handler `0x141dd4520` (`Player_ClearHeldSkill`): clears both pending skill slots
/// (+0x214, +0x22c) and, unless the Player flags say otherwise, ends the held action. Header
/// `(player, 0, 0, 0)`; `u1a` was 4 or 10 in a few official replies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillInputEndAck {
    pub header: SimulationHeader,
}

impl Message for SkillInputEndAck {
    const NAME: &'static str = "SkillInputEndAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
        })
    }
}

/// S→C `0x01b2`: acknowledgement of C→S `SkillReleased` `0x01ad` (`u8 player, u32 skill, u8`):
/// header, `u32 skill, u8` (client handler `0x141dd4610`, `Player_ApplyServerSkillRelease`:
/// clears the slot of that skill; for the Move skill it also ends the click walk). Header
/// `(player, 0, 0, 6)` in all four official samples.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillReleasedAck {
    pub header: SimulationHeader,
    pub skill: u32,
    pub flag: u8,
}

impl Message for SkillReleasedAck {
    const NAME: &'static str = "SkillReleasedAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        w.u32(self.skill).u8(self.flag);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
            skill: r.u32()?,
            flag: r.u8()?,
        })
    }
}

/// S→C `0x01ec`: a buff is removed (reader `0x141f6c1a0`): header, `u16 buff index` (the
/// index given in [`BuffAdded`]). The sprint buffs 4 and 5 (by index within the entity) are
/// removed together with the sprint `StatsUpdate`s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuffRemoved {
    pub header: SimulationHeader,
    pub index: u16,
}

impl Message for BuffRemoved {
    const NAME: &'static str = "BuffRemoved";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        w.u16(self.index);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: SimulationHeader::decode(r)?,
            index: r.u16()?,
        })
    }
}

/// S→C `0x01eb`: a buff is added (reader `0x141f6bbf0`, **partial**): header, `u16 index`
/// (slot id chosen by the server, grows per buff), `u16 buff row` (BuffDefinitions row; the
/// sprint buffs are 2758 = `0xac6`, 2737 = `0xab1`, 2853 = `0xb25`), `u32`, `u16`, `u16
/// flags`, then optional fields by flag: `0x01` varuint + u32 + u8 + u16; `0x02` u32 (f32
/// duration; the sprint update re-sends buff `0xac6` with `0x3dcccccd` = 0.1); `0x04` u16;
/// `0x08` u16 + u32 + u8; `0x20` u8; `0x80` u32 (f32); a final `u32 n` and `n` x `u32`.
/// Flag `0x10` and fields that depend on the buff definition (a `u16` when the definition has
/// a flag at +0xfe, `n` extra `u32`s when it has one at +0x7c) are not handled: the decoder
/// rejects flag `0x10` and assumes the definition flags are clear, which holds for the three
/// sprint buffs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuffAdded {
    pub header: SimulationHeader,
    pub index: u16,
    pub buff: u16,
    pub unk_64: u32,
    pub unk_60: u16,
    pub flags: u16,
    pub flag_01: Option<(u32, u32, u8, u16)>,
    pub flag_02: Option<u32>,
    pub flag_04: Option<u16>,
    pub flag_08: Option<(u16, u32, u8)>,
    pub flag_20: Option<u8>,
    pub flag_80: Option<u32>,
    pub extra: Vec<u32>,
}

impl BuffAdded {
    /// Sets `flags` from the optional fields; call before [`Message::encode`] when building one.
    pub fn with_flags(mut self) -> Self {
        let mut f = 0;
        f |= if self.flag_01.is_some() { 0x01 } else { 0 };
        f |= if self.flag_02.is_some() { 0x02 } else { 0 };
        f |= if self.flag_04.is_some() { 0x04 } else { 0 };
        f |= if self.flag_08.is_some() { 0x08 } else { 0 };
        f |= if self.flag_20.is_some() { 0x20 } else { 0 };
        f |= if self.flag_80.is_some() { 0x80 } else { 0 };
        self.flags = f;
        self
    }
}

impl Message for BuffAdded {
    const NAME: &'static str = "BuffAdded";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        self.header.encode(w);
        w.u16(self.index)
            .u16(self.buff)
            .u32(self.unk_64)
            .u16(self.unk_60)
            .u16(self.flags);
        if let Some((a, b, c, d)) = self.flag_01 {
            w.varuint(a).u32(b).u8(c).u16(d);
        }
        if let Some(v) = self.flag_02 {
            w.u32(v);
        }
        if let Some(v) = self.flag_80 {
            w.u32(v);
        }
        if let Some(v) = self.flag_04 {
            w.u16(v);
        }
        if let Some((a, b, c)) = self.flag_08 {
            w.u16(a).u32(b).u8(c);
        }
        if let Some(v) = self.flag_20 {
            w.u8(v);
        }
        w.u32(self.extra.len() as u32);
        for &v in &self.extra {
            w.u32(v);
        }
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let header = SimulationHeader::decode(r)?;
        let mut m = Self {
            header,
            index: r.u16()?,
            buff: r.u16()?,
            unk_64: r.u32()?,
            unk_60: r.u16()?,
            flags: r.u16()?,
            ..Default::default()
        };
        if m.flags & 0x10 != 0 {
            return Err(DecodeError::Invalid {
                offset: r.position(),
                what: "BuffAdded flag 0x10 (not decoded)",
            });
        }
        if m.flags & 0x01 != 0 {
            m.flag_01 = Some((r.varuint()?, r.u32()?, r.u8()?, r.u16()?));
        }
        if m.flags & 0x02 != 0 {
            m.flag_02 = Some(r.u32()?);
        }
        if m.flags & 0x80 != 0 {
            m.flag_80 = Some(r.u32()?);
        }
        if m.flags & 0x04 != 0 {
            m.flag_04 = Some(r.u16()?);
        }
        if m.flags & 0x08 != 0 {
            m.flag_08 = Some((r.u16()?, r.u32()?, r.u8()?));
        }
        if m.flags & 0x20 != 0 {
            m.flag_20 = Some(r.u8()?);
        }
        let n = r.u32()?;
        for _ in 0..n.min(1024) {
            m.extra.push(r.u32()?);
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Official bodies (after the 2-byte opcode), captured 2026-10-14/17 on real servers.
    fn body(hex: &str) -> Vec<u8> {
        let b: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        b[2..].to_vec()
    }

    /// Decodes `hex` as `M`, checks nothing is left over and that re-encoding gives the same
    /// bytes.
    fn round_trip<M: Message + std::fmt::Debug>(hex: &str) -> M {
        let bytes = body(hex);
        let mut r = Reader::new(&bytes);
        let m = M::decode(&mut r).unwrap();
        assert!(r.is_empty(), "trailing bytes in {hex}");
        let mut w = Writer::new();
        m.encode(&mut w);
        assert_eq!(w.as_slice(), &bytes[..], "re-encode of {m:?}");
        m
    }

    #[test]
    fn use_skill_ack_roll() {
        // Roll at (371,1052) with relative cursor (15,-35): the ack has the absolute cell.
        let m: UseSkillAck =
            round_trip("01ae000000660000000000000a800000400400ff0400000182000003f9");
        assert_eq!(m.header.entity_id, 0x66);
        assert_eq!(m.header.unk_1a, 10);
        assert_eq!(m.params.skill, 0x8000_0040);
        assert_eq!((m.params.flags, m.params.unk_13), (0x0400, 0xff));
        assert_eq!(m.params.target_flags, 0x04);
        assert_eq!((m.params.x, m.params.y), (386, 1017));
    }

    #[test]
    fn use_skill_ack_entity_target() {
        // Client sent target_flags 0x08 + entity 0x37; the ack adds 0x04.
        let m: UseSkillAck =
            round_trip("01ae0000006300000000000007804000400400ff0c000000370000021d000003b7");
        assert_eq!(m.params.target_flags, 0x0c);
        assert_eq!(m.params.target_entity, Some(0x37));
        assert_eq!((m.params.x, m.params.y), (0x21d, 0x3b7));
        // Held repeat: client 0x13, ack 0x17.
        let held: UseSkillAck =
            round_trip("01ae0000006300000000000000804000400000ff17000002710000039e");
        assert_eq!(held.params.target_flags, 0x17);
        assert_eq!(held.params.target_entity, None);
    }

    #[test]
    fn use_skill_ack_move_click() {
        // The intro's Move click: the ack has x = y = 0 (the click's StartAction carries the
        // target instead).
        let m: UseSkillAck =
            round_trip("01ae0000006600000000000006200029090066ff040000000000000000");
        assert_eq!(m.params.skill, 0x2000_2909);
        assert_eq!(m.params.flags, 0x66);
        assert_eq!((m.params.x, m.params.y), (0, 0));
        assert_eq!(m.header.unk_1a, 6);
    }

    #[test]
    fn update_skill_target_ack() {
        let m: UpdateSkillTargetAck =
            round_trip("01af00000066000000000000000000017c000003f980000040");
        assert_eq!((m.x, m.y, m.skill), (380, 1017, 0x8000_0040));
        // Entity variant.
        let e: UpdateSkillTargetEntityAck =
            round_trip("01b0000000630000000000000000000053000003500000036c80400040");
        assert_eq!(
            (e.entity, e.x, e.y, e.skill),
            (0x53, 0x350, 0x36c, 0x8040_0040)
        );
    }

    #[test]
    fn skill_input_end_pair() {
        let c: SkillInputEnd = round_trip("01ac01");
        assert_eq!(c.player, 1);
        let a: SkillInputEndAck = round_trip("01b10000006600000000000000");
        assert_eq!(a.header.entity_id, 0x66);
        let a4: SkillInputEndAck = round_trip("01b10000006600000000000004");
        assert_eq!(a4.header.unk_1a, 4);
    }

    #[test]
    fn skill_released_ack() {
        let m: SkillReleasedAck = round_trip("01b200000066000000000000062000290900");
        assert_eq!((m.skill, m.flag, m.header.unk_1a), (0x2000_2909, 0, 6));
    }

    #[test]
    fn buffs() {
        let removed: BuffRemoved = round_trip("01ec00000063000000630900070004");
        assert_eq!((removed.header.unk_18, removed.index), (9, 4));
        // The three sprint buffs at is_sprinting=1, and the f32 update 30 frames later.
        let a: BuffAdded = round_trip("01eb000000630000006305000200030ac6000000000000000000000000");
        assert_eq!((a.index, a.buff, a.flags), (3, 0x0ac6, 0));
        assert_eq!(a.header.unk_18, 5);
        let upd: BuffAdded =
            round_trip("01eb000000630000006305000200030ac600000000000000023dcccccd00000000");
        assert_eq!(upd.flag_02, Some(0x3dcc_cccd));
        let built = BuffAdded {
            header: upd.header,
            index: 3,
            buff: 0x0ac6,
            flag_02: Some(0x3dcc_cccd),
            ..Default::default()
        }
        .with_flags();
        assert_eq!(built, upd);
    }

    #[test]
    fn param_block_flag_roundtrip() {
        let p = SkillUseParams {
            skill: 1,
            flags: 0x400,
            unk_13: 0xff,
            target_flags: 0x10,
            target_entity: Some(9),
            x: -5,
            y: 6,
        };
        let mut w = Writer::new();
        p.encode(&mut w);
        let q = SkillUseParams::decode(&mut Reader::new(w.as_slice())).unwrap();
        assert_eq!(q.target_flags, 0x18);
        assert_eq!(q.target_entity, Some(9));
    }
}
