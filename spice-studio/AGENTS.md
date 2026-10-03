# AGENTS.md

This file gives future coding agents repository-specific guidance for working safely and effectively in `spice-studio`.

## Initial prompt context

The initial request for work in this repository was, in substance:

- commit the project as `initial commit`
- create a `.zip` archive of the current project
- add remote `git@github.com:beastr45/Vibe-Coded-SPICE.git` and push to GitHub
- set up SSH in the environment if needed to make the push work
- scan the codebase for readability, pragmatism, maintainability, selective documentation, and performance issues
- fix significant mdBook issues
- test the project to make sure it works
- add a detailed `AGENTS.md` for future agents

The standing style preference from the user was that the code should mostly document itself, with comments added only when they clarify what a function does or explain non-obvious behavior.

## Mission and standards

This repository is a Rust-first SPICE simulation workspace with a Tauri GUI, a standalone CLI, and an mdBook. Favor correctness, clarity, and incremental improvements over broad rewrites. The project is intended to stay understandable to humans. Prefer code that is explicit, pragmatic, and easy to modify later.

When making changes:

- preserve simulator correctness first
- keep interfaces stable unless a change is clearly justified
- avoid speculative abstraction
- prefer self-documenting code over comment-heavy code
- add doc comments only when they explain purpose or non-obvious behavior
- do not hand-edit generated or build-output files unless the task explicitly requires it

## Repository structure

Top-level components:

- `crates/spice-core`: parser, circuit model, simulation engine, schematic helpers
- `crates/spice-cli`: headless CLI wrapper around `spice-core`
- `apps/spice-studio`: React + TypeScript frontend
- `apps/spice-studio/src-tauri`: Tauri Rust backend for the desktop app
- `book`: mdBook documentation
- `examples`: example netlists used for manual checks and demos

High-value source files:

- `crates/spice-core/src/netlist.rs`: SPICE/Xyce-style parsing, source specs, `.subckt` expansion
- `crates/spice-core/src/simulator.rs`: operating point, DC, AC, transient, controlled sources, nonlinear stamping
- `crates/spice-core/src/schematic.rs`: lightweight SVG-backed schematic serialization helpers
- `crates/spice-cli/src/main.rs`: CLI entrypoint and output formatting
- `apps/spice-studio/src/App.tsx`: main UI component, net inference, waveform rendering
- `apps/spice-studio/src/types.ts`: shared frontend types
- `apps/spice-studio/src-tauri/src/main.rs`: Tauri commands bridging frontend to simulator/export logic
- `book/src/*.md`: documentation chapters

## Files and directories agents should usually avoid editing

These are generated, installed, or build artifacts unless the user explicitly asks otherwise:

- `target/`
- `apps/spice-studio/node_modules/`
- `apps/spice-studio/dist/`
- `book/book/`
- `apps/spice-studio/src-tauri/gen/`

If build or validation commands regenerate these, do not commit them unless the repository intentionally tracks them and the user asked for that.

## Development environment notes

Rust may not be on `PATH` in a fresh shell. If needed, load it with:

```bash
source "$HOME/.cargo/env"
```

Useful commands:

### Core / CLI

```bash
source "$HOME/.cargo/env"
cargo test -p spice-core
cargo test -p spice-cli
cargo check -p spice-core -p spice-cli
cargo run -p spice-cli -- examples/rc_lowpass.cir
```

### Frontend

```bash
cd apps/spice-studio
npm install
npm run build
```

### Tauri backend

```bash
source "$HOME/.cargo/env"
cargo check -p spice-studio-tauri
```

### mdBook

```bash
source "$HOME/.cargo/env"
mdbook build book
```

## Important validation constraints

Use the smallest validation surface that meaningfully proves the change:

- parser or solver changes: run `cargo test -p spice-core`
- CLI changes: run `cargo test -p spice-cli` and optionally a manual `cargo run -p spice-cli -- ...`
- frontend changes: run `npm run build` in `apps/spice-studio`
- mdBook changes: run `mdbook build book`
- Tauri Rust command changes: run `cargo check -p spice-studio-tauri`

Do not assume full workspace builds will succeed in every environment. GUI-linked Tauri builds may depend on system libraries such as GTK/WebKit packages that are not always installed. If those OS libraries are missing, clearly distinguish:

- repository code failures
- environment/dependency failures

## Guidance for changing `spice-core`

`spice-core` is the most sensitive part of the repository.

When touching parser or solver code:

- preserve existing analysis behavior unless intentionally expanding it
- avoid introducing hidden allocations inside tight loops unless the tradeoff is clear
- prefer localized helpers over new abstraction layers
- keep numeric constants and fallback behavior explicit
- check for unnecessary cloning, repeated map lookups, or rebuilding derived data inside loops
- maintain informative error messages with context

If you change anything in:

- `netlist.rs`: verify parsing of engineering suffixes, models, sources, and `.subckt`
- `simulator.rs`: verify at least operating point and one dynamic analysis path

The existing tests in `crates/spice-core/src/lib.rs` are the first line of defense. Add tests when behavior changes or bug fixes are non-trivial.

## Guidance for changing the frontend

`apps/spice-studio/src/App.tsx` is currently a large single component. Prefer incremental cleanup rather than aggressive decomposition unless the task specifically asks for a refactor.

Good patterns here:

- extract small pure helpers for repeated logic
- reduce repeated derived computations with `useMemo` or local precomputation when justified
- keep UI behavior easy to trace from event -> state update -> rendered output

Be careful with:

- ID generation
- inferred net naming behavior
- waveform cursor behavior
- keeping TypeScript types aligned with Tauri/Rust payloads

Always run `npm run build` after frontend changes.

## Guidance for mdBook changes

The book is part of the product, not just auxiliary docs.

When editing the book:

- keep chapters aligned with the actual codebase
- prefer concrete explanation tied to repository files
- ensure `book/book.toml` remains compatible with the installed `mdbook` version
- validate with `mdbook build book`

Do not leave broken chapter references or outdated configuration keys.

## Guidance for Tauri/backend changes

The Tauri backend should remain thin. Business logic belongs in `spice-core`, not in Tauri commands.

When editing `apps/spice-studio/src-tauri/src/main.rs`:

- keep commands focused on orchestration, I/O, and serialization
- avoid hardcoding paths when a simple helper or env override can make behavior safer
- return readable error strings to the frontend

Use `cargo check -p spice-studio-tauri` for validation when possible.

## Comments and documentation style

Comment policy for this repository:

- prefer expressive names and small helpers first
- use comments to explain purpose, invariants, or non-obvious tradeoffs
- avoid narrating trivial control flow
- use `///` doc comments on public APIs or small internal helpers only when it adds real orientation

## Performance review checklist

Before finalizing a non-trivial change, quickly scan for:

- repeated cloning of large vectors or maps
- repeated `Object.keys(...)` / `collect::<Vec<_>>()` inside hot paths
- repeated string normalization or lookup work that can be cached
- dense matrix rebuilds or expensive recomputation performed more often than necessary
- unnecessary serialization / formatting in UI render paths

Do not micro-optimize blindly. Favor changes with clear readability and measurable reduction in repeated work.

## Git hygiene

Before committing:

- inspect `git status`
- exclude generated artifacts unless intentionally tracked
- avoid committing `target/`, `book/book/`, transient `dist/` output, or local tool caches

Commit messages should be short and purpose-oriented.

## If you are unsure

If a task risks changing simulation semantics, widening scope, or requiring missing system packages, pause and state clearly:

- what is confirmed
- what is an environment issue
- what minimal next step is safest

The best future agent behavior in this repository is careful, transparent, and incremental.
