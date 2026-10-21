# Poe2Protocol

The wire protocol of **Path of Exile 2 client 0.5.5e**, shared by the private server and the client DLL of
[Poe2ModdingProject](https://github.com/Distorbed678/Poe2ModdingProject) (this repo is
consumed there as the submodule `modules/protocol`, and by [Poe2ClientHook](https://github.com/Distorbed678/Poe2ClientHook)
as a nested submodule).

It is its own Cargo workspace and builds standalone.

## Layout

| Path | Contents |
|---|---|
| `poe-net/` | Byte level: big-endian `Reader`/`Writer` with PoE string and varint encodings (`codec`), the `StreamCipher` trait with `Plain` and `Salsa20Stream` (`cipher`), `hexdump`. Behind the **`tokio`** cargo feature (off by default): the buffered TCP `Connection` (`conn`). |
| `poe-proto/` | Message layouts (`login`, `instance`, `entity`, `skill`, `remote`, `player_blob`, `message`), the opcode table loader (`opcodes`), and `describe` (decode a captured message to a readable form). |
| `opcodes/<version>/` | Per-version opcode tables: `login.toml`, `instance.toml`, `hnc.toml`, and `inventories.tsv`. Opcode numbers are never written in code. |

### Opcode tables as constants

`poe_proto::opcodes::v0_5_5e::{LOGIN_TOML, INSTANCE_TOML, HNC_TOML, INVENTORIES_TSV}` are the files compiled in with
`include_str!`, so the client DLL needs no path into this repo. The servers load the directory at run time with
`Protocol::load("modules/protocol/opcodes/0.5.5e")` (`protocol_dir` in `config/server.toml`).

### The `tokio` feature

`poe-net` without features has no async dependencies (the DLL uses it that way). The servers enable it:
`poe-net = { path = "modules/protocol/poe-net", features = ["tokio"] }`.

## Build and test

```
cargo test --workspace
cargo test --workspace --features poe-net/tokio
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

Any recent stable toolchain works (edition 2024). Inside Poe2ModdingProject the pinned nightly of that repo is used.

Check a capture against the layouts (a `packets.jsonl` written by the client DLL):

```
cargo run -p poe-proto --example check_capture -- opcodes/0.5.5e <packets.jsonl> [--show N]
```

## Adding a client version

Create `opcodes/<version>/*.toml`, add a constants module next to `v0_5_5e` in `poe-proto/src/opcodes.rs`, and point the
server's `protocol_dir` at it. Layouts that differ between versions get new types or version switches.

The format documentation (framing, login, instance, entities, movement) is in the main repository under `docs/protocol/`.

## Disclaimer

This is an unofficial, fan-made project for personal and educational use. It is **not affiliated with, endorsed by or connected to Grinding Gear Games**; "Path of Exile" is their trademark. **No game files are distributed** in this repository (no assets, bundles, tables or executables): you need your own legal copy of the game. Use is at your own risk. The message layouts and opcode numbers here are reverse-engineering notes about the 0.5.5e client's network protocol (research, interoperability); they contain no game code or assets. Using this code against official servers is not supported.

Security issues: [SECURITY.md](SECURITY.md). Contributing: [CONTRIBUTING.md](https://github.com/Distorbed678/Poe2ModdingProject/blob/main/CONTRIBUTING.md) in the main repository.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE). Copyright (c) 2026 Distorbed678 and contributors.
