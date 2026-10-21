use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

/// The opcode tables of each supported client version, compiled in so consumers (the client
/// DLL, which has no `opcodes/` directory next to it) don't need `include_str!` paths into
/// this repository. The servers load the same files from disk with [`Protocol::load`].
///
/// [`EMBEDDED`] lists them by client version (a consumer that knows its build picks one with
/// [`embedded`]); `v0_5_5e` is the same data under a fixed name, kept for the DLL until it
/// selects by its resolved build.
pub mod v0_5_5e {
    /// Client 0.5.5e: login channel.
    pub const LOGIN_TOML: &str = include_str!("../../opcodes/0.5.5e/login.toml");
    /// Client 0.5.5e: instance channel.
    pub const INSTANCE_TOML: &str = include_str!("../../opcodes/0.5.5e/instance.toml");
    /// Client 0.5.5e: HNC key derivation table.
    pub const HNC_TOML: &str = include_str!("../../opcodes/0.5.5e/hnc.toml");
    /// Client 0.5.5e: inventories an official server creates at area entry.
    pub const INVENTORIES_TSV: &str = include_str!("../../opcodes/0.5.5e/inventories.tsv");
}

/// The files of one `opcodes/<version>/` folder, compiled in.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedOpcodes {
    pub version: &'static str,
    pub login_toml: &'static str,
    pub instance_toml: &'static str,
    pub hnc_toml: &'static str,
    pub inventories_tsv: &'static str,
}

/// Every supported version's tables, oldest first. A new `opcodes/<version>/` folder is added
/// here (a test fails when one is missing) and to the main repository's `versions/`.
pub const EMBEDDED: &[EmbeddedOpcodes] = &[EmbeddedOpcodes {
    version: "0.5.5e",
    login_toml: include_str!("../../opcodes/0.5.5e/login.toml"),
    instance_toml: include_str!("../../opcodes/0.5.5e/instance.toml"),
    hnc_toml: include_str!("../../opcodes/0.5.5e/hnc.toml"),
    inventories_tsv: include_str!("../../opcodes/0.5.5e/inventories.tsv"),
}];

/// The compiled-in tables of `version`.
pub fn embedded(version: &str) -> Option<&'static EmbeddedOpcodes> {
    EMBEDDED.iter().find(|e| e.version == version)
}

/// The newest compiled-in version.
pub fn latest() -> &'static EmbeddedOpcodes {
    EMBEDDED.last().expect("at least one version is embedded")
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{channel}: duplicate {direction:?} opcode {id:#06x} ({a} and {b})")]
    Duplicate {
        channel: String,
        direction: Direction,
        id: u16,
        a: String,
        b: String,
    },
    #[error("{channel}: no {direction:?} message named {name}")]
    UnknownName {
        channel: String,
        direction: Direction,
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Sent by the client.
    Client,
    /// Sent by the server.
    Server,
}

#[derive(Debug, Default, Deserialize)]
struct RawTable {
    #[serde(default)]
    client: HashMap<String, u16>,
    #[serde(default)]
    server: HashMap<String, u16>,
    /// Body sizes (bytes after the opcode) of fixed-size client messages that are not
    /// decoded yet, by name, so a stream of messages can be split without decoding them.
    #[serde(default)]
    client_body_sizes: HashMap<String, usize>,
}

#[derive(Debug, Default)]
struct Map {
    by_name: HashMap<String, u16>,
    by_id: HashMap<u16, String>,
}

/// Opcodes of one channel (login or instance) in both directions.
#[derive(Debug)]
pub struct OpcodeTable {
    channel: String,
    client: Map,
    server: Map,
    client_body_sizes: HashMap<u16, usize>,
}

impl OpcodeTable {
    pub fn parse(channel: &str, text: &str) -> Result<Self, toml::de::Error> {
        let raw: RawTable = toml::from_str(text)?;
        let build = |entries: HashMap<String, u16>| {
            let mut map = Map::default();
            for (name, id) in entries {
                map.by_id.insert(id, name.clone());
                map.by_name.insert(name, id);
            }
            map
        };
        let client = build(raw.client);
        let client_body_sizes = raw
            .client_body_sizes
            .iter()
            .filter_map(|(name, size)| Some((*client.by_name.get(name)?, *size)))
            .collect();
        Ok(Self {
            channel: channel.to_owned(),
            client,
            server: build(raw.server),
            client_body_sizes,
        })
    }

    /// Body size of a fixed-size client message listed in `[client_body_sizes]`.
    pub fn client_body_size(&self, id: u16) -> Option<usize> {
        self.client_body_sizes.get(&id).copied()
    }

    fn map(&self, direction: Direction) -> &Map {
        match direction {
            Direction::Client => &self.client,
            Direction::Server => &self.server,
        }
    }

    /// Opcode of a named message.
    pub fn id(&self, direction: Direction, name: &str) -> Result<u16, ProtocolError> {
        self.map(direction)
            .by_name
            .get(name)
            .copied()
            .ok_or_else(|| ProtocolError::UnknownName {
                channel: self.channel.clone(),
                direction,
                name: name.to_owned(),
            })
    }

    /// Name of an opcode, if known.
    pub fn name(&self, direction: Direction, id: u16) -> Option<&str> {
        self.map(direction).by_id.get(&id).map(String::as_str)
    }

    fn check_duplicates(&self, raw_text: &str) -> Result<(), ProtocolError> {
        // HashMap construction silently keeps one name per id; detect collisions.
        let raw: RawTable = toml::from_str(raw_text).unwrap_or_default();
        for (direction, entries) in [
            (Direction::Client, raw.client),
            (Direction::Server, raw.server),
        ] {
            let mut seen: HashMap<u16, String> = HashMap::new();
            for (name, id) in entries {
                if let Some(other) = seen.insert(id, name.clone()) {
                    return Err(ProtocolError::Duplicate {
                        channel: self.channel.clone(),
                        direction,
                        id,
                        a: other,
                        b: name,
                    });
                }
            }
        }
        Ok(())
    }
}

/// All opcode tables of one client version.
#[derive(Debug)]
pub struct Protocol {
    pub login: OpcodeTable,
    pub instance: OpcodeTable,
    /// HNC side-channel key derivation (`hnc.toml`).
    pub hnc: HncKeys,
}

/// How the client derives the HNC side channel's key material from the session's.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HncKeys {
    /// 64 bytes, hex.
    pub key_xor: String,
    /// Byte pairs swapped after the XOR, in order.
    pub swaps: Vec<[usize; 2]>,
}

impl HncKeys {
    /// The HNC key material for a session's key material.
    pub fn derive(&self, session: &[u8; 64]) -> [u8; 64] {
        let mut out = *session;
        let xor = self.key_xor.as_bytes();
        for (i, b) in out.iter_mut().enumerate() {
            let hex = std::str::from_utf8(&xor[i * 2..i * 2 + 2]).unwrap_or("00");
            *b ^= u8::from_str_radix(hex, 16).unwrap_or(0);
        }
        for [a, b] in &self.swaps {
            out.swap(*a, *b);
        }
        out
    }
}

impl Protocol {
    /// Loads `<dir>/login.toml`, `<dir>/instance.toml` and `<dir>/hnc.toml`.
    pub fn load(dir: &Path) -> Result<Self, ProtocolError> {
        let path = dir.join("hnc.toml");
        let text = std::fs::read_to_string(&path).map_err(|source| ProtocolError::Io {
            path: path.clone(),
            source,
        })?;
        let hnc: HncKeys =
            toml::from_str(&text).map_err(|source| ProtocolError::Parse { path, source })?;
        Ok(Self {
            login: load_table(dir, "login")?,
            instance: load_table(dir, "instance")?,
            hnc,
        })
    }
}

fn load_table(dir: &Path, channel: &str) -> Result<OpcodeTable, ProtocolError> {
    let path = dir.join(format!("{channel}.toml"));
    let text = std::fs::read_to_string(&path).map_err(|source| ProtocolError::Io {
        path: path.clone(),
        source,
    })?;
    let table = OpcodeTable::parse(channel, &text).map_err(|source| ProtocolError::Parse {
        path: path.clone(),
        source,
    })?;
    table.check_duplicates(&text)?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_protocol_loads() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../opcodes")
            .join(latest().version);
        let p = Protocol::load(&dir).unwrap();
        let id = p.login.id(Direction::Client, "KeyExchange").unwrap();
        assert_eq!(p.login.name(Direction::Client, id), Some("KeyExchange"));
        assert_eq!(p.hnc.key_xor.len(), 128);
        // XOR with zeros gives the table, then the swaps apply.
        let k = p.hnc.derive(&[0; 64]);
        assert_eq!(k[0x15], 0xe5);
    }

    #[test]
    fn builtin_constants_match_the_files() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../opcodes/0.5.5e"); // the v0_5_5e module's folder
        for (name, text) in [
            ("login.toml", v0_5_5e::LOGIN_TOML),
            ("instance.toml", v0_5_5e::INSTANCE_TOML),
            ("hnc.toml", v0_5_5e::HNC_TOML),
            ("inventories.tsv", v0_5_5e::INVENTORIES_TSV),
        ] {
            assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), text);
        }
        let login = OpcodeTable::parse("login", v0_5_5e::LOGIN_TOML).unwrap();
        login.check_duplicates(v0_5_5e::LOGIN_TOML).unwrap();
        assert!(login.id(Direction::Client, "KeyExchange").is_ok());
        OpcodeTable::parse("instance", v0_5_5e::INSTANCE_TOML).unwrap();
    }

    #[test]
    fn every_opcode_folder_is_embedded_and_equal_to_its_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../opcodes");
        let mut folders = 0;
        for entry in std::fs::read_dir(&root).unwrap().flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            folders += 1;
            let version = entry.file_name().to_string_lossy().into_owned();
            let e = embedded(&version)
                .unwrap_or_else(|| panic!("opcodes/{version} is not in EMBEDDED"));
            for (name, text) in [
                ("login.toml", e.login_toml),
                ("instance.toml", e.instance_toml),
                ("hnc.toml", e.hnc_toml),
                ("inventories.tsv", e.inventories_tsv),
            ] {
                assert_eq!(
                    std::fs::read_to_string(entry.path().join(name)).unwrap(),
                    text,
                    "{version}/{name}"
                );
            }
        }
        assert_eq!(folders, EMBEDDED.len());
        assert_eq!(latest().version, EMBEDDED.last().unwrap().version);
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let text = "[client]\nA = 1\nB = 1\n";
        let t = OpcodeTable::parse("x", text).unwrap();
        assert!(t.check_duplicates(text).is_err());
    }
}
