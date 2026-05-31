# rand-game

> **Development status:** This project is under active development. APIs, protocols, rules, storage formats, commands, and gameplay behavior may change at any time.

`rand-game` is an automation game MVP where user-provided executable bots receive a limited view of the world, output actions, and let the server validate and apply those actions to a shared 2D world.

The current codebase includes a single-process server, FlatBuffers-based bot I/O, a sample bot, a terminal client, and local development tasks.

## Current Scope

- Rust workspace crates: `server`, `client`, `common`, `binary`, `xtask`
- Deterministic 2D tile generation from `world_seed`, `map_id`, and coordinates
- Delta-like world storage: changed tiles, entities, buildings, and players are saved to `var/server/world.bin`
- Action log storage in `var/server/action-log.bin`
- HTTP API for health, world queries, entities, action log, ASCII map view, bot uploads, and bot stderr streaming
- Bot protocol using `magic + little-endian u32 length + FlatBuffer payload`
- Sample bot that reads `GameInput` from stdin and writes `GameOutput` to stdout
- Terminal client for API calls, bot uploads, a colored ASCII map view, and bot stderr streaming
- Local developer tooling for build, validation, server runs, debug runs, and E2E debug checks

Sandboxing, production multi-player flows, a web map, monster/event simulation, and external storage are not implemented yet.

## Requirements

- Rust toolchain with edition 2024 support
- Linux or a Unix-like environment is recommended

## Quick Start

```bash
cargo xtask build
cargo xtask server-debug
```

In another terminal, upload the default sample bot and view the map.

```bash
cargo xtask upload-bot
cargo xtask map-view
```

Run the interactive terminal map client:

```bash
cargo run -p client -- map-view
cargo run -p client -- bot-stderr --player-id 1
```

Use `w/a/s/d` to move the viewport and `q` or `Ctrl-C` to quit.

## Main Commands

```bash
cargo xtask build
cargo xtask validate
cargo xtask clean-state
cargo xtask server
cargo xtask server-debug
cargo xtask upload-bot
cargo xtask map-view
cargo xtask user-debug
cargo xtask e2e-debug
```

- `build`: builds the server, common protocol crate, sample bot, and client.
- `validate`: runs `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings`.
- `clean-state`: removes `var/server` and `var/bots` to reset local state.
- `server`: runs `rand-game-server` with the default configuration.
- `server-debug`: resets state and runs the server with a debug action limit.
- `upload-bot`: uploads the default sample bot or a specified executable to the server.
- `map-view`: prints the server ASCII map view.
- `user-debug`: uploads the default bot and opens the client map view.
- `e2e-debug`: starts a server, uploads a bot, and verifies that the world changes.

## Running The Server

The default server address is `127.0.0.1:3000`.

```bash
cargo run -p rand-game-server
```

Options:

```bash
cargo run -p rand-game-server -- --addr 127.0.0.1:3000
cargo run -p rand-game-server -- --env-path config/server.env.toml
cargo run -p rand-game-server -- --rules-path config/server.rules.toml
cargo run -p rand-game-server -- --debug-max-actions 1000
cargo run -p rand-game-server -- --log-bot-stderr
```

Environment variables:

- `RAND_GAME_DEBUG_MAX_ACTIONS`: overrides the max action count per bot run for debugging.
- `RAND_GAME_LOG_BOT_STDERR`: when set to a truthy value, prints bot stderr in the server log.

## HTTP API

- `GET /health`: server status, tick, player/entity counts, and action log count
- `GET /world?x=0&y=0&radius=4`: tile JSON in a Manhattan-radius region
- `GET /map-view?player_id=1&x=0&y=0&radius=8`: ASCII map view
- `GET /entities`: entity JSON
- `GET /action-log`: action log JSON
- `GET /bot-stderr`: WebSocket stream of bot stderr event JSON; optional `player_id` query filters events
- `POST /bots?player_id=1`: upload a bot executable

## Bot Protocol

A bot may be started as a fresh process on each run. The server writes a `GameInput` frame to bot stdin, and the bot must write a `GameOutput` frame to stdout.

Frame format:

```text
4 bytes magic
4 bytes payload length, little-endian u32
N bytes FlatBuffer payload
```

Magic values:

- `BWI1`: `GameInput` from server to bot
- `BWO1`: `GameOutput` from bot to server

FlatBuffers schemas live in `crates/common/schema/*.fbs`. Generated Rust code is included under `crates/common/src/flatbuffers_generated/`.

Current `GameInput` contains the protocol version, tick/map/ruleset information, visible tiles, owned entities, persistent memory, and runtime/action limits.

Current `GameOutput` contains an action list and the next persistent memory value.

Supported actions:

- `Move`: move an owned entity to an orthogonally adjacent empty tile
- `Mine`: mine an adjacent resource tile
- `Build`: build near an owned core on an adjacent empty tile
- `Lift`: move resources from the current tile into entity cargo
- `Put`: move resources from entity cargo onto the current tile
- `Craft`: craft a generated recipe by `recipe_id` from entity cargo, optionally using a compatible owned building

## World And Rules

The world is managed by `WorldState`.

- A new world starts with one default player, one core entity, one worker entity, and one core building.
- `map_id % 3` selects `Resource`, `Hazard`, or `Monster` map kind.
- Current generation is resource-cluster focused; hazard and monster event simulation are not implemented yet.
- Bot run cadence is based on the core tier's `run_interval_ticks` and `core_entity_id % interval` phase.
- The default tick interval is `1000ms`.

Configuration files:

- `config/server.env.toml`: `world_seed`, `map_id`
- `config/server.rules.toml`: tick interval, observation radius, API query radius, upload limit, and per-core-tier runtime profiles
- `config/building.yaml`, `config/recipe.yaml`: source YAML catalogs compiled into `rand-game-common` at build time. The server and sample bot use the same generated Rust catalog; recipe ids are validated by the server for `Craft` actions.

## Crate Structure

- `crates/server`: world state, rule validation, bot execution, storage, and HTTP API
- `crates/client`: simple HTTP/WebSocket client and terminal map viewer
- `crates/common`: FlatBuffers schemas/generated code and frame utilities
- `crates/binary`: sample bot using the server protocol
- `crates/xtask`: development, validation, and debug task automation

## License

This repository uses different licenses for different crates.

- `crates/xtask`: GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`)
- `crates/server`: GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`)
- `crates/client`: GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`)
- `crates/binary`: Apache License 2.0 (`Apache-2.0`)
- `crates/common`: Apache License 2.0 (`Apache-2.0`)

Full license texts are available in `LICENSES/AGPL-3.0-or-later.txt` and `LICENSES/Apache-2.0.txt`.
