# RedstoneBuilder

Compile a Verilog-subset HDL into a Minecraft Litematica `.litematic`
schematic. Target: Minecraft Java Edition 26.1 ("Tiny Takeover").

```bash
cargo build --release
target/release/redstonebuilder examples/half_adder.hdl -o ha.litematic
```

Drop `ha.litematic` into your world's `schematics/` folder, open
Litematica in-game, and place the resulting half-adder.

See **[specs/001-hdl-compiler-cli/quickstart.md](specs/001-hdl-compiler-cli/quickstart.md)**
for the full onboarding walkthrough, the HDL grammar, the CLI flags,
and the debugging workflow.

## Repository layout

This is a Cargo workspace with five crates:

| Crate | Role |
|---|---|
| [`crates/rb-core`](crates/rb-core) | Shared types: positions, source spans, gate kinds, block-state catalogue. |
| [`crates/rb-parser`](crates/rb-parser) | PEG parser for the HDL surface syntax (pest grammar in `src/hdl.pest`). |
| [`crates/rb-synthesis`](crates/rb-synthesis) | Netlist + cycle detection + cell library + row-based placement + 3D maze router. |
| [`crates/rb-nbt`](crates/rb-nbt) | Litematica `.litematic` writer (gzip + fastnbt). |
| [`crates/redstonebuilder`](crates/redstonebuilder) | Binary entry-point: clap CLI + miette diagnostics + pipeline. |

## Specification & docs

- **Constitution**: [`.specify/memory/constitution.md`](.specify/memory/constitution.md) — non-negotiable engineering rules.
- **Spec**: [`specs/001-hdl-compiler-cli/spec.md`](specs/001-hdl-compiler-cli/spec.md) — functional requirements, success criteria, user stories.
- **Plan**: [`specs/001-hdl-compiler-cli/plan.md`](specs/001-hdl-compiler-cli/plan.md) — technical decisions, dependencies, workspace layout.
- **Contracts**: [`specs/001-hdl-compiler-cli/contracts/`](specs/001-hdl-compiler-cli/contracts/) — CLI, HDL grammar, AST, netlist IR, NBT tag tree, block-state catalogue.
- **HDL grammar reference**: [`docs/grammar.md`](docs/grammar.md) (mirror of the grammar contract).

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench --workspace --no-run
```

CI runs these on every PR. The full bench suite is manual / nightly.

## Status

v1 implementation status: see [`specs/001-hdl-compiler-cli/tasks.md`](specs/001-hdl-compiler-cli/tasks.md).

| Milestone | Coverage |
|---|---|
| US1 — combinational compilation (half-adder) | ✅ |
| US2 — sequential (D-trigger / Memory Cell) | ✅ |
| US3 — scale (`--max-footprint`, `--seed`, retry semantics) | ✅ |
| Polish — determinism gate, benches, docs | ✅ |
| In-game functional verification on MC Java 26.1 | manual (T078) |

## License

Dual-licensed under MIT OR Apache-2.0.
