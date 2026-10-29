//! Decoder and encoder for the `AddObject` blob of a **player character**, local or remote
//! (official servers send both in the same layout; see `docs/protocol/entities.md`).
//!
//! The layout is the one of [`crate::entity`] (little-endian integers, packet varints for
//! stat lists) but with every optional field the official servers fill for other players:
//! entity link records, stat sets per weapon set, buffs, equipment visuals and the Actor's
//! animation sets. Parts that depend on game tables or that are not understood are kept raw
//! (`Actor::tail`), so `decode` followed by `encode` returns the input bytes.
//!
//! Limits (decode returns an error rather than guessing): Positioned optional sections, an
//! Animated flags byte other than `0x00`, `0x80`, `0x01`, `0x81` (path and/or the five floats),
//! buffs whose definition adds the `+0x7c`
//! u32 list or the `+0xfe` u16 (unknown without `Buffs.dat`), link record kinds other than
//! 1 and 0x0b, Player entries (`u8 n > 0`), and Life with other than the character layout.

use poe_net::{DecodeError, Reader, Writer};

use crate::entity::{Life, Pool};

/// A link record of the blob header (`0x141e715a0`): `u32 key, u8 tag, u8 count, [u8 kind,
/// count x value]`. The key is a hash of the link's name (not resolved yet). Kind 1 values are
/// **entity ids** (resolved, or queued until that entity exists); kind `0x0b` values are plain
/// integers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRecord {
    pub key: u32,
    pub tag: u8,
    pub kind: u8,
    pub values: Vec<u32>,
}

/// Link key every official player carries for another player in town (observed values: a
/// nearby NPC or the stash entity id). The local player has no record.
pub const LINK_KEY_PLAYER_NEARBY: u32 = 0xb350_ac3a;
/// Link key of a pet to its owner on the owner's side (observed: the Phoenix pet's entity id
/// in its owner's blob, kind `0x0b`, tag 0).
pub const LINK_KEY_PET: u32 = 0x951d_d509;

/// One buff of the Buffs component (`0x141db5080` + body `0x141d72a90`), assuming the buff
/// definition adds no conditional fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuffEntry {
    pub id: u16,
    pub flag: u8,
    /// Buff row (`0x4a4` in a town player: a hidden permanent buff).
    pub row: u16,
    pub unk_2: u16,
    /// +0x28: f32 duration bits (`0x7f800000` = infinite).
    pub duration: u32,
    /// +0x2c: f32 bits (1.0).
    pub unk_2c: u32,
    pub unk_38: u16,
    /// +0x34: the owner entity id (the player itself).
    pub owner: u32,
    pub unk_3a: u16,
    pub unk_48: u16,
    pub unk_44: u32,
    pub unk_4c: u32,
    pub unk_3c: u32,
    pub unk_b8: u8,
    /// The counted `u32` list at the end of the entry.
    pub list: Vec<u32>,
}

/// A stat set of the Stats component's player section: `u8 id`, then a stat list. Ids seen:
/// 0x40/0x41 (the two weapon sets), 0x80/0x81 (main/off hand weapon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatSet {
    pub id: u8,
    pub stats: Vec<(u32, i32)>,
}

/// Stats component (`0x142181160`) of a player in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStats {
    /// The entity's own stats (network key = `Stats.dat` row + 1; key 1 is `level`).
    pub base: Vec<(u32, i32)>,
    pub unk_168: u8,
    /// `ShapeshiftForms` row + 1 (0 = none).
    pub form: u8,
    /// Three weapon-set shapeshift references (rows + 1; 2 for every character observed).
    pub slots: [u8; 3],
    pub sets: Vec<StatSet>,
    pub unk_1b0: u32,
    pub unk_1b4: u8,
    pub unk_1b5: u8,
}

/// The visual of one equipment slot (reader `0x1426c41a0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotVisual {
    /// First byte of the record (the visual's id; the Actor's animation set refers to the ids
    /// of slots 0 and 1).
    pub id: u8,
    pub a: (u16, u32, u8),
    pub b: (u16, u32, u8),
    pub c: u8,
    pub d: u16,
    pub e: [u8; 3],
}

/// An entry of the Inventories component: an equipment slot (`Inventories` slot index) and its
/// visual, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipSlot {
    pub slot: u8,
    pub visual: Option<SlotVisual>,
}

/// Player component (`0x141dcf4a0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSection {
    pub name: String,
    /// `Characters.IntegerId` (2 = Ranger line, 7 = Witch line, 11 observed).
    pub class: u8,
    /// Experience (`0x141dd1360`; the same `u32` a [`crate::entity::PlayerDelta`] updates).
    pub experience: u32,
    pub unk_1d0: u32,
    pub unk_1d4: u32,
    pub unk_200: u16,
    pub unk_202: u16,
    /// Two lists of (key, `u64` bits): progress flag words (the first 15-37 entries).
    pub flags_a: Vec<(u8, u64)>,
    pub flags_b: Vec<(u8, u64)>,
    pub bytes5: [u8; 5],
    pub hideout_a: u16,
    pub hideout_b: u16,
    /// Bit 0 washed-up intro, bit 2 combat stance (`+0x20c`), bit 3 (remote players: set).
    pub flags: u8,
    pub unk_205: u8,
    /// `+0x3e7` (local 4, remote players 2).
    pub unk_3e7: u8,
    pub unk_list: Vec<u32>,
}

/// An animation set string of the Actor component with its two bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationSet {
    pub a: u8,
    pub b: u8,
    pub name: String,
}

/// Actor component (`0x142147300`): three animation sets (weapon pose names such as
/// `BowTown`, `UnarmedTown`, `default`; the two bytes of the first are the ids of equipment
/// slots 0 and 1), then the skill container and the rest, kept raw (including the blob's
/// final `u8` keyed-entry count). Remote players list two skills (`0x80000040` dodge roll,
/// `0x80000041`), the local player all of its skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorSection {
    pub sets: [AnimationSet; 3],
    pub tail: Vec<u8>,
}

/// A player blob.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBlob {
    /// Entity flags (bit 1: in the wake zone; always 2 for players).
    pub flags: u8,
    pub links: Vec<LinkRecord>,
    pub grid_x: i32,
    pub grid_y: i32,
    /// Facing as f32 bits (+0x44c).
    pub facing: u32,
    pub team: u16,
    pub stats: PlayerStats,
    pub buffs: Vec<BuffEntry>,
    pub life: Life,
    /// Animated flags byte (bit 7: five f32 follow, bit 0: the animated object's path follows
    /// the flags; `0x80` in every official blob, local and remote). `encode` derives bit 0 from
    /// [`PlayerBlob::animated_path`].
    pub animated_flags: u8,
    /// The animated object (`.ao`) path of flags bit 0 (official blobs send none: the model
    /// comes from the object type; our own player's blob sends the class's).
    pub animated_path: Option<String>,
    /// +0x25c..+0x278 as f32 bits: (turn speed, ?, ?, facing, facing) in the observed blobs.
    pub animated: [u32; 5],
    pub player: PlayerSection,
    pub equipment: Vec<EquipSlot>,
    pub inventory_refs: Vec<u32>,
    pub actor: ActorSection,
}

fn short(r: &Reader<'_>, needed: usize) -> DecodeError {
    DecodeError::Incomplete {
        offset: r.position(),
        needed,
    }
}

fn invalid(r: &Reader<'_>, what: &'static str) -> DecodeError {
    DecodeError::Invalid {
        offset: r.position(),
        what,
    }
}

fn u16_le(r: &mut Reader<'_>) -> Result<u16, DecodeError> {
    Ok(u16::from_le_bytes(r.array()?))
}
fn u32_le(r: &mut Reader<'_>) -> Result<u32, DecodeError> {
    Ok(u32::from_le_bytes(r.array()?))
}
fn i32_le(r: &mut Reader<'_>) -> Result<i32, DecodeError> {
    Ok(i32::from_le_bytes(r.array()?))
}
fn i16_le(r: &mut Reader<'_>) -> Result<i16, DecodeError> {
    Ok(i16::from_le_bytes(r.array()?))
}

fn utf16(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    let n = u32_le(r)? as usize;
    if r.remaining() < n * 2 {
        return Err(short(r, n * 2));
    }
    let units: Vec<u16> = r
        .bytes(n * 2)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| invalid(r, "utf-16 string"))
}

fn stat_list(r: &mut Reader<'_>) -> Result<Vec<(u32, i32)>, DecodeError> {
    let n = r.varuint()?;
    let mut v = Vec::new();
    for _ in 0..n {
        v.push((r.varuint()?, r.varint()?));
    }
    Ok(v)
}

fn put_stat_list(w: &mut Writer, list: &[(u32, i32)]) {
    w.varuint(list.len() as u32);
    for &(k, v) in list {
        w.varuint(k).varint(v);
    }
}

fn pool(r: &mut Reader<'_>, long: bool) -> Result<Pool, DecodeError> {
    Ok(Pool {
        maximum: u32_le(r)?,
        unk_18: u32_le(r)?,
        unk_1c: i16_le(r)?,
        current: u32_le(r)?,
        unk_48: if long { u32_le(r)? } else { 0 },
    })
}

fn put_pool(w: &mut Writer, p: &Pool, long: bool) {
    w.u32_le(p.maximum)
        .u32_le(p.unk_18)
        .i16_le(p.unk_1c)
        .u32_le(p.current);
    if long {
        w.u32_le(p.unk_48);
    }
}

fn flag_list(r: &mut Reader<'_>) -> Result<Vec<(u8, u64)>, DecodeError> {
    let n = u16_le(r)?;
    let mut v = Vec::new();
    for _ in 0..n {
        let key = r.u8()?;
        v.push((key, u64::from_le_bytes(r.array()?)));
    }
    Ok(v)
}

fn put_flag_list(w: &mut Writer, list: &[(u8, u64)]) {
    w.u16_le(list.len() as u16);
    for &(k, bits) in list {
        w.u8(k).bytes(&bits.to_le_bytes());
    }
}

impl PlayerBlob {
    /// Decodes a blob (the bytes after `AddObject`'s varuint length) of a player entity.
    pub fn decode(blob: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(blob);
        let flags = r.u8()?;
        // Link records.
        let mut links = Vec::new();
        for _ in 0..r.u8()? {
            let key = u32_le(&mut r)?;
            let tag = r.u8()?;
            let count = r.u8()?;
            let (kind, values) = if count == 0 {
                (0, Vec::new())
            } else {
                let kind = r.u8()?;
                if kind != 1 && kind != 0x0b {
                    return Err(invalid(&r, "link record kind"));
                }
                let mut v = Vec::new();
                for _ in 0..count {
                    v.push(u32_le(&mut r)?);
                }
                (kind, v)
            };
            links.push(LinkRecord {
                key,
                tag,
                kind,
                values,
            });
        }
        // Positioned (no optional sections).
        let grid_x = i32_le(&mut r)?;
        let grid_y = i32_le(&mut r)?;
        let facing = u32_le(&mut r)?;
        let team = u16_le(&mut r)?;
        if r.array::<3>()? != [0; 3] {
            return Err(invalid(&r, "Positioned optional sections"));
        }
        // Stats.
        let base = stat_list(&mut r)?;
        let unk_168 = r.u8()?;
        let form = r.u8()?;
        let slots = r.array::<3>()?;
        let mut sets = Vec::new();
        for _ in 0..r.u8()? {
            let id = r.u8()?;
            sets.push(StatSet {
                id,
                stats: stat_list(&mut r)?,
            });
        }
        let stats = PlayerStats {
            base,
            unk_168,
            form,
            slots,
            sets,
            unk_1b0: u32_le(&mut r)?,
            unk_1b4: r.u8()?,
            unk_1b5: r.u8()?,
        };
        // Pathfinding: nothing. Buffs.
        let mut buffs = Vec::new();
        for _ in 0..u16_le(&mut r)? {
            let id = u16_le(&mut r)?;
            let flag = r.u8()?;
            let row = u16_le(&mut r)?;
            let unk_2 = u16_le(&mut r)?;
            let duration = u32_le(&mut r)?;
            let unk_2c = u32_le(&mut r)?;
            let unk_38 = u16_le(&mut r)?;
            let owner = u32_le(&mut r)?;
            let unk_3a = u16_le(&mut r)?;
            let unk_48 = u16_le(&mut r)?;
            let unk_44 = u32_le(&mut r)?;
            let unk_4c = u32_le(&mut r)?;
            let unk_3c = r.varuint()?;
            let unk_b8 = r.u8()?;
            let n = i32_le(&mut r)?;
            if !(0..=64).contains(&n) {
                return Err(invalid(&r, "buff list length"));
            }
            let mut list = Vec::new();
            for _ in 0..n {
                list.push(u32_le(&mut r)?);
            }
            buffs.push(BuffEntry {
                id,
                flag,
                row,
                unk_2,
                duration,
                unk_2c,
                unk_38,
                owner,
                unk_3a,
                unk_48,
                unk_44,
                unk_4c,
                unk_3c,
                unk_b8,
                list,
            });
        }
        // Life.
        let life_pool = pool(&mut r, true)?;
        let mana = pool(&mut r, false)?;
        let unk_240 = pool(&mut r, true)?;
        let unk_298 = pool(&mut r, false)?;
        let energy_shield = pool(&mut r, true)?;
        let unk_330 = pool(&mut r, true)?;
        let mut unk_380 = Vec::new();
        for _ in 0..3 {
            unk_380.push((u32_le(&mut r)?, i16_le(&mut r)?));
        }
        let unk_398 = (u32_le(&mut r)?, i16_le(&mut r)?);
        let life = Life {
            life: life_pool,
            mana,
            unk_240,
            unk_298,
            energy_shield,
            unk_330,
            unk_380,
            unk_398,
            flags: r.u8()?,
        };
        // Animated.
        let animated_flags = r.u8()?;
        let mut animated = [0u32; 5];
        if animated_flags & !0x81 != 0 {
            return Err(invalid(&r, "Animated flags"));
        }
        let animated_path = if animated_flags & 1 != 0 {
            Some(utf16(&mut r)?)
        } else {
            None
        };
        if animated_flags & 0x80 != 0 {
            for a in &mut animated {
                *a = u32_le(&mut r)?;
            }
        }
        // Player.
        let name = utf16(&mut r)?;
        let class = r.u8()?;
        let experience = u32_le(&mut r)?;
        let unk_1d0 = u32_le(&mut r)?;
        let unk_1d4 = u32_le(&mut r)?;
        let unk_200 = u16_le(&mut r)?;
        let unk_202 = u16_le(&mut r)?;
        let flags_a = flag_list(&mut r)?;
        let flags_b = flag_list(&mut r)?;
        let bytes5 = r.array::<5>()?;
        let hideout_a = u16_le(&mut r)?;
        let hideout_b = u16_le(&mut r)?;
        let pflags = r.u8()?;
        let unk_205 = r.u8()?;
        let unk_3e7 = r.u8()?;
        let mut unk_list = Vec::new();
        for _ in 0..r.varuint()? {
            unk_list.push(u32_le(&mut r)?);
        }
        if r.u8()? != 0 {
            return Err(invalid(&r, "Player entries"));
        }
        let player = PlayerSection {
            name,
            class,
            experience,
            unk_1d0,
            unk_1d4,
            unk_200,
            unk_202,
            flags_a,
            flags_b,
            bytes5,
            hideout_a,
            hideout_b,
            flags: pflags,
            unk_205,
            unk_3e7,
            unk_list,
        };
        // PlayerClass: nothing. Inventories.
        let mut equipment = Vec::new();
        for _ in 0..r.u8()? {
            let slot = r.u8()?;
            let id = r.u8()?;
            let visual = if id == 0 {
                None
            } else {
                Some(SlotVisual {
                    id,
                    a: (u16_le(&mut r)?, u32_le(&mut r)?, r.u8()?),
                    b: (u16_le(&mut r)?, u32_le(&mut r)?, r.u8()?),
                    c: r.u8()?,
                    d: u16_le(&mut r)?,
                    e: r.array::<3>()?,
                })
            };
            equipment.push(EquipSlot { slot, visual });
        }
        let mut inventory_refs = Vec::new();
        for _ in 0..r.u8()? {
            inventory_refs.push(u32_le(&mut r)?);
        }
        // Actor: three animation sets, then the raw rest.
        let set = |r: &mut Reader<'_>| -> Result<AnimationSet, DecodeError> {
            Ok(AnimationSet {
                a: r.u8()?,
                b: r.u8()?,
                name: utf16(r)?,
            })
        };
        let sets = [set(&mut r)?, set(&mut r)?, set(&mut r)?];
        let tail = r.rest().to_vec();
        Ok(Self {
            flags,
            links,
            grid_x,
            grid_y,
            facing,
            team,
            stats,
            buffs,
            life,
            animated_flags,
            animated_path,
            animated,
            player,
            equipment,
            inventory_refs,
            actor: ActorSection { sets, tail },
        })
    }

    /// Encodes the blob (inverse of [`Self::decode`]).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.flags).u8(self.links.len() as u8);
        for l in &self.links {
            w.u32_le(l.key).u8(l.tag).u8(l.values.len() as u8);
            if !l.values.is_empty() {
                w.u8(l.kind);
                for &v in &l.values {
                    w.u32_le(v);
                }
            }
        }
        w.i32_le(self.grid_x)
            .i32_le(self.grid_y)
            .u32_le(self.facing)
            .u16_le(self.team)
            .bytes(&[0; 3]);
        let s = &self.stats;
        put_stat_list(&mut w, &s.base);
        w.u8(s.unk_168)
            .u8(s.form)
            .bytes(&s.slots)
            .u8(s.sets.len() as u8);
        for set in &s.sets {
            w.u8(set.id);
            put_stat_list(&mut w, &set.stats);
        }
        w.u32_le(s.unk_1b0).u8(s.unk_1b4).u8(s.unk_1b5);
        w.u16_le(self.buffs.len() as u16);
        for b in &self.buffs {
            w.u16_le(b.id)
                .u8(b.flag)
                .u16_le(b.row)
                .u16_le(b.unk_2)
                .u32_le(b.duration)
                .u32_le(b.unk_2c)
                .u16_le(b.unk_38)
                .u32_le(b.owner)
                .u16_le(b.unk_3a)
                .u16_le(b.unk_48)
                .u32_le(b.unk_44)
                .u32_le(b.unk_4c)
                .varuint(b.unk_3c)
                .u8(b.unk_b8)
                .i32_le(b.list.len() as i32);
            for &v in &b.list {
                w.u32_le(v);
            }
        }
        let l = &self.life;
        put_pool(&mut w, &l.life, true);
        put_pool(&mut w, &l.mana, false);
        put_pool(&mut w, &l.unk_240, true);
        put_pool(&mut w, &l.unk_298, false);
        put_pool(&mut w, &l.energy_shield, true);
        put_pool(&mut w, &l.unk_330, true);
        for &(a, b) in l.unk_380.iter().chain([&l.unk_398]) {
            w.u32_le(a).i16_le(b);
        }
        w.u8(l.flags);
        w.u8((self.animated_flags & 0x80) | u8::from(self.animated_path.is_some()));
        if let Some(path) = &self.animated_path {
            w.utf16_le(path);
        }
        if self.animated_flags & 0x80 != 0 {
            for &a in &self.animated {
                w.u32_le(a);
            }
        }
        let p = &self.player;
        w.utf16_le(&p.name)
            .u8(p.class)
            .u32_le(p.experience)
            .u32_le(p.unk_1d0)
            .u32_le(p.unk_1d4)
            .u16_le(p.unk_200)
            .u16_le(p.unk_202);
        put_flag_list(&mut w, &p.flags_a);
        put_flag_list(&mut w, &p.flags_b);
        w.bytes(&p.bytes5)
            .u16_le(p.hideout_a)
            .u16_le(p.hideout_b)
            .u8(p.flags)
            .u8(p.unk_205)
            .u8(p.unk_3e7)
            .varuint(p.unk_list.len() as u32);
        for &v in &p.unk_list {
            w.u32_le(v);
        }
        w.u8(0);
        w.u8(self.equipment.len() as u8);
        for e in &self.equipment {
            w.u8(e.slot);
            match &e.visual {
                None => {
                    w.u8(0);
                }
                Some(v) => {
                    w.u8(v.id)
                        .u16_le(v.a.0)
                        .u32_le(v.a.1)
                        .u8(v.a.2)
                        .u16_le(v.b.0)
                        .u32_le(v.b.1)
                        .u8(v.b.2)
                        .u8(v.c)
                        .u16_le(v.d)
                        .bytes(&v.e);
                }
            }
        }
        w.u8(self.inventory_refs.len() as u8);
        for &v in &self.inventory_refs {
            w.u32_le(v);
        }
        for s in &self.actor.sets {
            w.u8(s.a).u8(s.b).utf16_le(&s.name);
        }
        w.bytes(&self.actor.tail);
        w.into_vec()
    }

    /// The `level` stat (network key 1), if present.
    pub fn level(&self) -> Option<i32> {
        self.stats
            .base
            .iter()
            .find(|&&(k, _)| k == 1)
            .map(|&(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anonymised blobs of the 2026-10-20 town capture: the AddObject payloads (after the
    /// varuint length) of three other players and of the local player, with the character
    /// names replaced by placeholders and the two id words after the experience set to fixed
    /// values.
    fn fixture(text: &str) -> Vec<u8> {
        text.trim()
            .as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect()
    }

    const REMOTE_DEX: &str = include_str!("../tests/data/remote_dex_blob.hex");
    const REMOTE_INT: &str = include_str!("../tests/data/remote_int_blob.hex");
    const REMOTE_STRINT: &str = include_str!("../tests/data/remote_strint_blob.hex");
    const LOCAL_DEX: &str = include_str!("../tests/data/local_dex_blob.hex");

    #[test]
    fn blobs_round_trip() {
        for text in [REMOTE_DEX, REMOTE_INT, REMOTE_STRINT, LOCAL_DEX] {
            let bytes = fixture(text);
            let blob = PlayerBlob::decode(&bytes).unwrap();
            assert_eq!(blob.encode(), bytes, "{}", blob.player.name);
        }
    }

    #[test]
    fn remote_player_fields() {
        let dex = PlayerBlob::decode(&fixture(REMOTE_DEX)).unwrap();
        assert_eq!(dex.flags, 2);
        assert_eq!(dex.player.name, "player1");
        assert_eq!((dex.player.class, dex.level()), (2, Some(10)));
        assert_eq!((dex.grid_x, dex.grid_y, dex.team), (314, 246, 1));
        // A link to a nearby object (entity 35) and one buff owned by the player (12000).
        assert_eq!(
            dex.links,
            [LinkRecord {
                key: LINK_KEY_PLAYER_NEARBY,
                tag: 1,
                kind: 1,
                values: vec![35]
            }]
        );
        assert_eq!(dex.buffs.len(), 1);
        assert_eq!((dex.buffs[0].row, dex.buffs[0].owner), (0x4a4, 12000));
        assert_eq!(dex.buffs[0].duration, 0x7f80_0000);
        // Stat sets: weapon sets 0x40/0x41 and the main hand 0x80/0x81.
        let ids: Vec<u8> = dex.stats.sets.iter().map(|s| s.id).collect();
        assert_eq!(ids, [0x40, 0x41, 0x80, 0x81]);
        assert_eq!(dex.stats.slots, [2, 2, 2]);
        // Remote players are in combat stance (+0x20c, flags bit 2) with bit 3 set.
        assert_eq!(dex.player.flags, 0x0c);
        assert_eq!(dex.player.unk_3e7, 2);
        assert_eq!(dex.actor.sets[0].name, "BowTown");
        assert_eq!(dex.equipment.len(), 10);

        let strint = PlayerBlob::decode(&fixture(REMOTE_STRINT)).unwrap();
        // The pet link (kind 0x0b, entity 12010) comes first; buff with an extra u32 list.
        assert_eq!(strint.links.len(), 2);
        assert_eq!(strint.links[0].key, LINK_KEY_PET);
        assert_eq!(
            (strint.links[0].kind, &strint.links[0].values[..]),
            (0x0b, &[12010][..])
        );
        assert_eq!(strint.player.class, 11);
    }

    #[test]
    fn the_animated_path_flag_round_trips() {
        // Official blobs: flags 0x80, five floats, no path.
        let mut blob = PlayerBlob::decode(&fixture(REMOTE_DEX)).unwrap();
        assert_eq!(
            (blob.animated_flags, blob.animated_path.as_deref()),
            (0x80, None)
        );
        // With a path (bit 0) the path comes between the flags and the floats.
        blob.animated_path = Some("Metadata/Characters/Dex/DexFour.ao".into());
        let bytes = blob.encode();
        let back = PlayerBlob::decode(&bytes).unwrap();
        assert_eq!(back.animated_flags, 0x81);
        assert_eq!(back.animated_path, blob.animated_path);
        assert_eq!(back.animated, blob.animated);
        assert_eq!(back.player, blob.player);
        assert_eq!(back.encode(), bytes);
        // Neither floats nor path.
        blob.animated_path = None;
        blob.animated_flags = 0;
        let back = PlayerBlob::decode(&blob.encode()).unwrap();
        assert_eq!((back.animated_flags, back.animated), (0, [0; 5]));
    }

    #[test]
    fn local_player_differs_from_remote() {
        let local = PlayerBlob::decode(&fixture(LOCAL_DEX)).unwrap();
        // No links, not in combat stance at entry, ordinary +0x3e7, many more skills.
        assert!(local.links.is_empty());
        assert_eq!((local.player.flags, local.player.unk_3e7), (0, 4));
        assert_eq!(local.level(), Some(2));
        assert!(local.actor.tail.len() > 2000);
        let remote = PlayerBlob::decode(&fixture(REMOTE_INT)).unwrap();
        assert!(remote.actor.tail.len() < 300);
        // Both carry skill 0x80000040 (dodge roll) in the Actor tail.
        let roll = 0x8000_0040u32.to_le_bytes();
        for b in [&local, &remote] {
            assert!(b.actor.tail.windows(4).any(|w| w == roll));
        }
    }
}
