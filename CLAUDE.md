# bevy-deploy-helper

## Product Vision

Coaching and analysis platform for Warhammer 40K — not a live-play simulator. Primary use: post-game review (coach reviews student's game) and solo prep/theorycrafting. Features a chess-style analysis tree (like Lichess) with branching timelines, text annotations, and board drawings per node. Includes a chroma-key overlay mode for OBS casting (overlay on overhead board camera showing wounds, annotations). JSON replay export/import for sharing annotated games. Web app (WASM) is the primary distribution target.

## Core Design Philosophy

- **Annotation-first**: tools record what happened, never enforce what's allowed. The player is the rules arbiter; the app is a shared annotation layer.
- **Phase is context, not a gate**: phase suggests the default tool and records timeline snapshots, but no tool is ever phase-locked. You can Kill in Movement, Move in Shooting — the player declares, the app annotates.
- **Player declares move type**: the app doesn't infer Normal vs Advance from distance. The player explicitly picks the tool; the app records correctly.
- **Soft guidance only**: passive visual indicators (colored tints, outlines) for "egregiously wrong" situations. No popups, no blocking dialogs, no enforcement.
- **Don't encode rules**: no engagement range enforcement, no coherency checking, no stratagem availability. The app does LOS analysis, range measurement, timeline snapshots, movement annotation, and deployment analysis.

## Bevy ECS Conventions

### Systems
- Max ~100 lines per system. If larger, decompose by responsibility.
- Use `run_if` conditions for ALL prerequisites. No early returns to check state — if a condition can be expressed as `run_if`, it must be.
- Naming: `on_` (event handlers), `sync_` (reactive to resource/component changes), `update_` (per-frame), `draw_` (UI/gizmos).
- Max 8 system parameters. Use `SystemParam` bundles beyond that.
- One responsibility per system.

### Components
- Prefer marker components over bool fields (`IsAdvanced` component, not `has_advanced: bool`).
- One responsibility per component.
- Child entities for visual elements (labels, rings, badges).
- Entity relationships via marker components, not `HashMap<Entity, Entity>` in resources.

### Resources
- Max 5 fields. Split larger resources by domain.
- Per-tool state resources are the correct pattern — continue using them.
- No `Option<Entity>` fields — use marker components and queries instead.

### Events
- Fire-and-forget triggers. Name is a verb: `SpawnUnit`, `ConfirmKill`, `AdvancePhase`.
- No circular event flows (system must not send an event it also consumes).
- Events defined in `events.rs`, not scattered across type modules.

### States
- `ActiveTool` as Bevy `States`: correct, keep it.
- `GamePhase` should become a Bevy `State` (currently manual in `PhaseState` resource) to get `OnEnter`/`OnExit` scheduling.
- `OnExit` cleanup is mandatory for every state.
- State transitions via `NextState` only.

### Run Conditions
- `in_state(ActiveTool::X)` for tool-specific systems.
- `resource_changed()` for sync systems.
- Custom closures for composite conditions.
- NEVER check prerequisites inside the system body when `run_if` can express them.

## Architecture

- `src/types/` — data structs (terrain, units, deployment, visibility, timeline, phase)
- `src/los/` — geometric LOS: shapes, occupancy, visibility polygons
- `src/army_list/` — Listforge parser + base_lookup (Datasheets.json)
- `src/plugins/` — Bevy plugins: board, terrain, deployment, units, visibility, timeline, ui
- `src/resources.rs` — shared resources (BoardConfig, per-tool state, panel widths, PhaseState)
- `src/events.rs` — all events
- `src/main.rs` — app setup, plugin registration, static data loading

**Coordinates**: JSON data is y-down; Bevy is y-up. Flip at data boundary: `world_position()` returns `(x, BOARD_HEIGHT - json_y)`. BOARD_HEIGHT = 44.0. Rotation sign negated. 1 inch = 1 Bevy world unit. Board = 60x44.

## Known Architectural Debt

Priority order:

1. **handle_drag is a God system** (185 lines, 11 responsibilities) — must be decomposed into per-tool drag handlers
2. **GameTimeline is a God resource** (10 fields) — must be split into game state / entity mappings / live movement tracking
3. **trigger_analysis is a God system** (197 lines, 13 params) — must be split by responsibility
4. **GamePhase should be a Bevy State**, not a manual field in PhaseState
5. **Early returns instead of run_if** conditions throughout the codebase
6. **Inline `.observe()` closures** with state mutation in visibility.rs
7. **No guards** on unit removal after deployment lock
8. **Magic numbers** (ring sizes, charge range, advance bonus) — extract to named constants

## Feature Roadmap

- **Timeline tree**: branching snapshots with sidebar tree UI (like Lichess analysis)
- **Node annotations**: text notes + board drawings per tree node
- **Chroma-key mode**: toggle green/transparent background for OBS overlay
- **Wound tracking**: visible wound counters on units for casting
- **JSON replay**: export/import full game tree
- **External datasheets**: API or file-based datasheet sourcing (replace embedded JSON)
- **Floating hotbar**: move tool palette from right panel to bottom-center hotbar
- **Unit datacard**: StarCraft-style card in bottom-right showing selected unit stats

## UI Conventions

- Tool palette: floating hotbar at bottom center (currently in right panel — to be moved)
- Unit datacard: bottom-right, StarCraft-style stat card for selected unit
- Analysis tree: left sidebar when reviewing
- Right panel: phase info, snapshot list, tool-specific controls
- Passive indicators: colored tints/outlines on units, never popups or blocking dialogs

## Testing

- `cargo test` must pass before committing.
- `cargo build` must be clean. Warnings acceptable only for pre-existing unused items.
- New systems should have unit tests where feasible.
