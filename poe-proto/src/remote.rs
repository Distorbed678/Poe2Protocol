//! Messages around other players: removing an entity, social/party/trade traffic.
//!
//! All layouts were decoded from the 2026-10-20 official town capture (other players present;
//! see `docs/protocol/entities.md`). Field names with a number are the offsets of the client
//! message class; meanings marked "unk" are not known.

use poe_net::{DecodeError, Reader, Writer};

use crate::instance::SimulationHeader;
use crate::{Direction, Message};

macro_rules! header_only {
    ($(#[$meta:meta])* $name:ident, $label:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name {
            pub header: SimulationHeader,
        }

        impl Message for $name {
            const NAME: &'static str = $label;
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
    };
}

header_only!(
    /// S→C simulation message `0x01c5`: destroys the entity and its children (applied by
    /// `0x142134240`, `0x141e71620(entity, 0)`: sets entity flag `+0x8c |= 1` "destroyed",
    /// recurses into the children, queues the entity in the world's delete list). Official
    /// header `(entity, 0, 0, 0)`.
    DestroyEntity,
    "DestroyEntity"
);

header_only!(
    /// S→C simulation message `0x01c6`: the entity enters the wake zone (`entity +0x8d |= 4`,
    /// `0x141e73bf0`): it starts to be updated and drawn. The header's second field is the
    /// **entity of the player that woke it** (`(object, player, 0, 0)`).
    EntityWake,
    "EntityWake"
);

header_only!(
    /// S→C simulation message `0x01c7`: the entity leaves the wake zone (`+0x8d &= ~4`,
    /// `0x141e73f20`). Official header `(entity, 0xffffffff, 0, 0)` for objects, `(entity,
    /// 0, 0, 0)` right before a player's removal.
    EntitySleep,
    "EntitySleep"
);

header_only!(
    /// S→C simulation message `0x02fa`: removes the entity from the world (the final step of
    /// every removal; case `0x142136099` calls `0x141e71620(entity, 1)` unless it is already
    /// flagged destroyed). Official header `(entity, 0xffffffff, 0, 0)`: after all entity
    /// updates of the frame.
    RemoveObject,
    "RemoveObject"
);

/// S→C `0x0271`: arrives once at area entry and **right before the removal of each player
/// that leaves** (frame of its `0x01c5`/`0x02fa`). Always `01 01 01 00` so far; the handler
/// is not identified (deserializer `0x141f45090`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresenceChanged {
    pub unk: [u8; 4],
}

impl Message for PresenceChanged {
    const NAME: &'static str = "PresenceChanged";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.bytes(&self.unk);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self { unk: r.array()? })
    }
}

/// S→C `0x00c4` (class `0x1430a2fa0`, handler case `0x1420ee915`): `u8 local player, u32
/// code`. The client logs "InstanceClientSetSelfPartyInvitationSecurityCode = N" and stores it
/// in the session (`+0x3ec0`). Official servers send 0 in the entry batch of every area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetPartyInvitationCode {
    pub local_index: u8,
    pub code: u32,
}

impl Message for SetPartyInvitationCode {
    const NAME: &'static str = "SetPartyInvitationCode";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.code);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            code: r.u32()?,
        })
    }
}

/// C→S `0x00b6`: invite a character to the party (`/invite`, the social panel, the player
/// context menu; chat command id `0x16`, sender `0x142110810`): `u8 local player, wstring
/// character name`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartyInvite {
    pub local_index: u8,
    pub name: String,
}

impl Message for PartyInvite {
    const NAME: &'static str = "PartyInvite";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).wstring(&self.name);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            name: r.wstring()?,
        })
    }
}

/// C→S `0x00b7`: cancel the invitation of a character (chat command id `0x1a`, sender in
/// `0x14210d630`): same body as [`PartyInvite`]. Answered by [`PartyUpdate`] (state 1),
/// [`SocialRemove`] and [`PartyEnded`] (reason 4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartyInviteCancel {
    pub local_index: u8,
    pub name: String,
}

impl Message for PartyInviteCancel {
    const NAME: &'static str = "PartyInviteCancel";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).wstring(&self.name);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            name: r.wstring()?,
        })
    }
}

/// C→S `0x00b8`: accept a party invitation: `u8 local player, u32 party id`. **UNVERIFIED**:
/// read from the invite popup's accept callback (`0x140c9ef70`, generic `u8 + u32` class
/// `0x1430a2fa0`), never captured (gate G-PARTY).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartyAccept {
    pub local_index: u8,
    pub party: u32,
}

impl Message for PartyAccept {
    const NAME: &'static str = "PartyAccept";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.party);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            party: r.u32()?,
        })
    }
}

/// C→S `0x00b9`: decline a party invitation: `u8 local player, u32 party id`. **UNVERIFIED**:
/// the decline callback `0x140c9eee0` calls `0x142110ab0`, which builds this message (and skips
/// it when the id is the party the player is already in), never captured (gate G-PARTY).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartyDecline {
    pub local_index: u8,
    pub party: u32,
}

impl Message for PartyDecline {
    const NAME: &'static str = "PartyDecline";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.party);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            party: r.u32()?,
        })
    }
}

/// S→C `0x00c6`: a social list entry (the client keeps them in the session at `+0x27d8`, `0x210`
/// bytes each; reader `0x141f35de0`, handler case `0x142123f1a`): sent for yourself and for every
/// character that joins your party or friend list, and again when its area changes (a player
/// that left the town through a waypoint got the new area hash).
///
/// Layout: `wstring account, wstring (empty), u64, u8 flags` (bit 0: two more `u16`s at the end,
/// bit 1: two more, bit 2: character data follows; 5 for characters in the same area, 4 for
/// yourself inside an instance), then with bit 2: `wstring character, u8 level (+0x201), u8
/// class (+0x200), u16 area, u16 area2, wstring league, u8 (+0x1fc)`, `[u16, u16]` for bit 0,
/// `[u16, u16]` for bit 1, and a final `u8` (`0x141f4caa0`). Entries without bit 2 are not
/// supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialEntry {
    pub unk_0: u8,
    /// `Name#1234`.
    pub account: String,
    /// Always empty so far.
    pub unk_str: String,
    pub unk_u64: u64,
    pub flags: u8,
    pub character: String,
    pub level: u8,
    pub class: u8,
    /// `WorldAreas.HASH16` of the character's area (0 inside an instance that is not a town).
    pub area: u16,
    pub area2: u16,
    pub league: String,
    pub unk_1fc: u8,
    /// Present with flags bit 0 (`+0x1f2`, `+0x1f6`; zero in the capture).
    pub extra_a: Option<(u16, u16)>,
    /// Present with flags bit 1 (`+0x1f8`, `+0x1fa`).
    pub extra_b: Option<(u16, u16)>,
    pub tail: u8,
}

impl Message for SocialEntry {
    const NAME: &'static str = "SocialEntry";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.unk_0)
            .wstring(&self.account)
            .wstring(&self.unk_str)
            .u64(self.unk_u64)
            .u8(self.flags)
            .wstring(&self.character)
            .u8(self.level)
            .u8(self.class)
            .u16(self.area)
            .u16(self.area2)
            .wstring(&self.league)
            .u8(self.unk_1fc);
        if self.flags & 1 != 0 {
            let (a, b) = self.extra_a.unwrap_or_default();
            w.u16(a).u16(b);
        }
        if self.flags & 2 != 0 {
            let (a, b) = self.extra_b.unwrap_or_default();
            w.u16(a).u16(b);
        }
        w.u8(self.tail);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let unk_0 = r.u8()?;
        let account = r.wstring()?;
        let unk_str = r.wstring()?;
        let unk_u64 = r.u64()?;
        let flags = r.u8()?;
        if flags & 4 == 0 {
            return Err(DecodeError::Invalid {
                offset: r.position(),
                what: "social entry without character data",
            });
        }
        let character = r.wstring()?;
        let level = r.u8()?;
        let class = r.u8()?;
        let area = r.u16()?;
        let area2 = r.u16()?;
        let league = r.wstring()?;
        let unk_1fc = r.u8()?;
        let extra_a = if flags & 1 != 0 {
            Some((r.u16()?, r.u16()?))
        } else {
            None
        };
        let extra_b = if flags & 2 != 0 {
            Some((r.u16()?, r.u16()?))
        } else {
            None
        };
        Ok(Self {
            unk_0,
            account,
            unk_str,
            unk_u64,
            flags,
            character,
            level,
            class,
            area,
            area2,
            league,
            unk_1fc,
            extra_a,
            extra_b,
            tail: r.u8()?,
        })
    }
}

/// S→C `0x00c7`: a social entry goes away (the invited character's account after the
/// invitation was cancelled): `u8, wstring account` (class `0x1434e51f0`, `u8` + string).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocialRemove {
    pub unk_0: u8,
    pub account: String,
}

impl Message for SocialRemove {
    const NAME: &'static str = "SocialRemove";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.unk_0).wstring(&self.account);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            unk_0: r.u8()?,
            account: r.wstring()?,
        })
    }
}

/// A member of a [`PartyUpdate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyMember {
    /// 2 = the leader, 1 = a member, 0 = invited, not joined (the client maps them to
    /// leader / member / invited with `0x1421063f0`).
    pub role: u8,
    pub character: String,
    pub account: String,
    /// 0xff in every capture.
    pub tail: u8,
}

/// S→C `0x00c8` (reader `0x142156320`): the state of the party you are in or invite from. The
/// server sends it three times when an invitation starts (one member, one member, then with the
/// invitee as a second member, role 0) and once more with `state` 1 when the invitation is
/// cancelled. The party id is the same `u32` twice and changes with a new party (`0xcfc43`,
/// then `0xcfc55`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyUpdate {
    pub local_index: u8,
    pub party: u32,
    pub unk_a: u16,
    pub unk_b: u16,
    pub party2: u32,
    /// Index of the local player in `members` (`+0x76`; `0xff` = not in the list). The client
    /// takes its own state from that member's role: leader, member, or invited (which makes it
    /// show the invitation popup). 0 for the inviter in the capture; the invitee's is 1
    /// (UNVERIFIED, from the handler `0x142106a30` case `0xc8`).
    pub unk_0: u8,
    /// 1 in every capture (`+0x74`).
    pub unk_1: u8,
    /// 0 in every capture (`+0x75`).
    pub unk_2: u8,
    /// 0, or 1 after a cancel (`+0x77`).
    pub state: u8,
    pub members: Vec<PartyMember>,
}

impl Message for PartyUpdate {
    const NAME: &'static str = "PartyUpdate";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index)
            .u32(self.party)
            .u16(self.unk_a)
            .u16(self.unk_b)
            .u32(self.party2)
            .u8(self.unk_0)
            .u8(self.unk_1)
            .u8(self.unk_2)
            .u8(self.state)
            .u8(self.members.len() as u8);
        for m in &self.members {
            w.u8(m.role)
                .wstring(&m.character)
                .wstring(&m.account)
                .u8(m.tail);
        }
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let local_index = r.u8()?;
        let party = r.u32()?;
        let unk_a = r.u16()?;
        let unk_b = r.u16()?;
        let party2 = r.u32()?;
        let unk_0 = r.u8()?;
        let unk_1 = r.u8()?;
        let unk_2 = r.u8()?;
        let state = r.u8()?;
        let count = r.u8()?;
        let mut members = Vec::new();
        for _ in 0..count {
            members.push(PartyMember {
                role: r.u8()?,
                character: r.wstring()?,
                account: r.wstring()?,
                tail: r.u8()?,
            });
        }
        Ok(Self {
            local_index,
            party,
            unk_a,
            unk_b,
            party2,
            unk_0,
            unk_1,
            unk_2,
            state,
            members,
        })
    }
}

/// S→C `0x00c9` (`0x141f43e80`): a party ended: `u8, u32 party id, u8 reason` (4 after
/// [`PartyInviteCancel`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartyEnded {
    pub local_index: u8,
    pub party: u32,
    pub reason: u8,
}

impl Message for PartyEnded {
    const NAME: &'static str = "PartyEnded";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.party).u8(self.reason);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            party: r.u32()?,
            reason: r.u8()?,
        })
    }
}

/// C→S `0x00db`: ask the player behind an entity to trade (the player context menu; class
/// `0x14086d280`, `u8 + u32`): `u8 local player, u32 target entity id`. The target is the
/// entity id of the **other player's character** (the capture: `12002`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TradeRequest {
    pub local_index: u8,
    pub target: u32,
}

impl Message for TradeRequest {
    const NAME: &'static str = "TradeRequest";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.target);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            target: r.u32()?,
        })
    }
}

/// S→C `0x00d9` (`0x1421574b0`): the trade request is pending: `u8 local, u32 trade id, u32,
/// u32, u8, wstring other character, u32 (0xffffffff), u16`. Sent in the frame after
/// [`TradeRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeState {
    pub local_index: u8,
    /// 1 for the first request.
    pub trade: u32,
    /// 2 in the capture.
    pub unk_a: u32,
    /// 1 in the capture.
    pub unk_b: u32,
    /// `0x31` for the player who asked (capture). The client shows the "trade invite from"
    /// popup (`0x1402c3cb0`) only when bit 1 is set and bit 0 clear, so the target's is
    /// presumably `0x32` (UNVERIFIED, never captured from the target's side).
    pub unk_c: u8,
    pub other: String,
    pub unk_d: u32,
    pub unk_e: u16,
}

impl Message for TradeState {
    const NAME: &'static str = "TradeState";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index)
            .u32(self.trade)
            .u32(self.unk_a)
            .u32(self.unk_b)
            .u8(self.unk_c)
            .wstring(&self.other)
            .u32(self.unk_d)
            .u16(self.unk_e);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            trade: r.u32()?,
            unk_a: r.u32()?,
            unk_b: r.u32()?,
            unk_c: r.u8()?,
            other: r.wstring()?,
            unk_d: r.u32()?,
            unk_e: r.u16()?,
        })
    }
}

/// C→S `0x00d2` (`0x141f38ce0`): cancel the trade: `u8 local, u32 trade id, u8 (0)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TradeCancel {
    pub local_index: u8,
    pub trade: u32,
    pub unk: u8,
}

impl Message for TradeCancel {
    const NAME: &'static str = "TradeCancel";
    const DIRECTION: Direction = Direction::Client;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.trade).u8(self.unk);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            trade: r.u32()?,
            unk: r.u8()?,
        })
    }
}

/// S→C `0x00da` (`0x141f38d80`): the trade is closed (echo of [`TradeCancel`]): `u8 local,
/// u32 trade id, u8 (0)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TradeAck {
    pub local_index: u8,
    pub trade: u32,
    pub unk: u8,
}

impl Message for TradeAck {
    const NAME: &'static str = "TradeAck";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u32(self.trade).u8(self.unk);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            trade: r.u32()?,
            unk: r.u8()?,
        })
    }
}

/// S→C `0x00e0` (`0x141f3cd50`): a trade-related event: `u8 local, u8 kind, u32 value`. Two
/// arrive with the cancel (`(1, 2)` then `(1, 1)`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TradeEvent {
    pub local_index: u8,
    pub kind: u8,
    pub value: u32,
}

impl Message for TradeEvent {
    const NAME: &'static str = "TradeEvent";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.local_index).u8(self.kind).u32(self.value);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            local_index: r.u8()?,
            kind: r.u8()?,
            value: r.u32()?,
        })
    }
}

/// S→C `0x0019` (`0x142150da0`): a system notice shown in the chat (id into a client table not
/// identified; `0x010e` after a whisper that could not be delivered, `0x00e1` when the trade
/// was cancelled): `u8, u16 id, u16 (0xffff), u16 (0x0200), u32, u8, u8`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoticeMessage {
    pub unk_0: u8,
    pub id: u16,
    pub unk_a: u16,
    pub unk_b: u16,
    pub unk_c: u32,
    pub unk_d: u8,
    pub unk_e: u8,
}

impl Message for NoticeMessage {
    const NAME: &'static str = "NoticeMessage";
    const DIRECTION: Direction = Direction::Server;
    fn encode(&self, w: &mut Writer) {
        w.u8(self.unk_0)
            .u16(self.id)
            .u16(self.unk_a)
            .u16(self.unk_b)
            .u32(self.unk_c)
            .u8(self.unk_d)
            .u8(self.unk_e);
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            unk_0: r.u8()?,
            id: r.u16()?,
            unk_a: r.u16()?,
            unk_b: r.u16()?,
            unk_c: r.u32()?,
            unk_d: r.u8()?,
            unk_e: r.u8()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bodies re-encoded from the 2026-10-20 capture with placeholder names (`Local1`,
    /// `Acct#0001`, `Player1`, `remote_acct`).
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    fn round_trip<M: Message + PartialEq + std::fmt::Debug>(s: &str) -> M {
        let bytes = hex(s);
        let mut r = Reader::new(&bytes);
        let m = M::decode(&mut r).unwrap();
        assert!(r.is_empty(), "trailing bytes in {}", M::NAME);
        let mut w = Writer::new();
        m.encode(&mut w);
        assert_eq!(w.as_slice(), &bytes[..], "{}", M::NAME);
        m
    }

    #[test]
    fn removal_messages() {
        // A leaving player (entity 12009): destroy and sleep at key 0, remove at 0xffffffff.
        let d: DestroyEntity = round_trip("00002ee900000000000000");
        assert_eq!((d.header.entity_id, d.header.unk_14), (12009, 0));
        let s: EntitySleep = round_trip("00002ee900000000000000");
        assert_eq!(s.header.entity_id, 12009);
        let r: RemoveObject = round_trip("00002ee9ffffffff000000");
        assert_eq!(r.header.unk_14, 0xffff_ffff);
        // An object woken by player 12013.
        let w: EntityWake = round_trip("0000001d00002eed000000");
        assert_eq!((w.header.entity_id, w.header.unk_14), (29, 12013));
        let p: PresenceChanged = round_trip("01010100");
        assert_eq!(p.unk, [1, 1, 1, 0]);
        // Sent in every entry batch.
        let c: SetPartyInvitationCode = round_trip("0100000000");
        assert_eq!((c.local_index, c.code), (1, 0));
    }

    #[test]
    fn party_messages() {
        let inv: PartyInvite = round_trip("010007506c6179657231");
        assert_eq!((inv.local_index, inv.name.as_str()), (1, "Player1"));
        let cancel = PartyInviteCancel {
            local_index: 1,
            name: "Player1".into(),
        };
        let mut w = Writer::new();
        cancel.encode(&mut w);
        assert_eq!(w.as_slice(), hex("010007506c6179657231"));

        let own: SocialEntry = round_trip(
            "000009416363742330303031000000000000000000000500064c6f63616c3102023f703f70000f466f7262696464656e2052697465739b0000000000",
        );
        assert_eq!((own.level, own.class, own.area), (2, 2, 0x3f70)); // G1_town
        assert_eq!((own.flags, own.extra_a), (5, Some((0, 0))));
        // Inside the first zone (flags 4, no extra words), area G1_1.
        let first: SocialEntry = round_trip(
            "000009416363742330303031000000000000000000000400064c6f63616c3102020f140000000f466f7262696464656e2052697465739b00",
        );
        assert_eq!((first.flags, first.area, first.extra_a), (4, 0x0f14, None));
        let remote: SocialEntry = round_trip(
            "00000b72656d6f74655f6163637400000000000000000000050007506c61796572310a07e911e911000f466f7262696464656e2052697465739b0000000000",
        );
        // Level 10, class 7; moved to area 0xe911 (G1_7).
        assert_eq!((remote.level, remote.class, remote.area), (10, 7, 0xe911));

        let one: PartyUpdate = round_trip(
            "01000cfc4300000000000cfc4300010000010200064c6f63616c310009416363742330303031ff",
        );
        assert_eq!((one.party, one.members.len(), one.state), (0xcfc43, 1, 0));
        let two: PartyUpdate = round_trip(
            "01000cfc4300000000000cfc4300010000020200064c6f63616c310009416363742330303031ff000007506c6179657231000b72656d6f74655f61636374ff",
        );
        assert_eq!(two.members[1].role, 0);
        let cancelled: PartyUpdate = round_trip(
            "01000cfc4300000000000cfc4300010001010200064c6f63616c310009416363742330303031ff",
        );
        assert_eq!(cancelled.state, 1);
        let gone: SocialRemove = round_trip("00000b72656d6f74655f61636374");
        assert_eq!(gone.account, "remote_acct");
        // Read from the client's code, not captured (G-PARTY): `u8 local, u32 party id`.
        let accept: PartyAccept = round_trip("01000cfc43");
        let decline: PartyDecline = round_trip("01000cfc43");
        assert_eq!((accept.party, decline.party), (0xcfc43, 0xcfc43));
        let ended: PartyEnded = round_trip("01000cfc4304");
        assert_eq!((ended.party, ended.reason), (0xcfc43, 4));
    }

    #[test]
    fn trade_messages() {
        let req: TradeRequest = round_trip("0100002ee2");
        assert_eq!(req.target, 12002);
        let st: TradeState =
            round_trip("01000000010000000200000001310007506c6179657231ffffffff006a");
        assert_eq!((st.trade, st.unk_a, st.other.as_str()), (1, 2, "Player1"));
        let c: TradeCancel = round_trip("010000000100");
        assert_eq!(c.trade, 1);
        let _: TradeAck = round_trip("010000000100");
        let e: TradeEvent = round_trip("010100000002");
        assert_eq!((e.kind, e.value), (1, 2));
        let n: NoticeMessage = round_trip("0000e1ffff0200000000000000");
        assert_eq!(n.id, 0xe1);
        let n: NoticeMessage = round_trip("00010effff0200000000000000");
        assert_eq!(n.id, 0x10e);
    }
}
