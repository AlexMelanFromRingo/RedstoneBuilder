<!--
Sync Impact Report
==================
Version change: TEMPLATE (uninitialized) → 1.0.0
Bump rationale: Initial ratification — all placeholders replaced with concrete
principles and governance. Treated as the foundational MAJOR.MINOR.PATCH = 1.0.0.

Modified principles: N/A (first ratification)
Added sections:
  - Core Principles I–IV
  - Technology & Performance Constraints
  - Development Workflow & Quality Gates
  - Governance
Removed sections: All `[PLACEHOLDER]` tokens from constitution-template.md

Templates status:
  - .specify/templates/plan-template.md       ✅ no changes required (generic; Constitution Check
                                                   gate references the constitution file by reference)
  - .specify/templates/spec-template.md       ✅ no changes required (domain-agnostic)
  - .specify/templates/tasks-template.md      ✅ no changes required (domain-agnostic)
  - .specify/templates/checklist-template.md  ✅ no changes required (not inspected; generic)

Follow-up TODOs: none
-->

# RedstoneBuilder Constitution

RedstoneBuilder is a compiler that translates an HDL-like source language into
Minecraft redstone schematic formats (`.schem`, `.litematic`). This constitution
defines the non-negotiable engineering rules for the project.

## Core Principles

### I. Rust-First, No Panics in the Compiler Path

The implementation language is Rust (edition 2021). Parser, AST transforms,
logic synthesizer, place-and-route, and NBT serialization code MUST NOT call
`unwrap()`, `expect()`, `panic!()`, `unreachable!()`, or `todo!()` on any code
path reachable from end-user input. All recoverable conditions MUST flow through
`Result<T, E>`. Errors MUST carry source location (file, line, column, span)
and produce human-readable diagnostics with the offending source snippet —
`thiserror`/`miette`-style reporting is the baseline.

Exceptions: `#[cfg(test)]` modules MAY use `unwrap()`/`expect()` for brevity;
truly impossible states inside private helpers MAY use `unreachable!()` ONLY
when accompanied by a comment proving the invariant.

**Rationale**: A compiler is judged on its error messages. Panics on bad input
are bugs, not edge cases.

### II. Strict Pipeline Architecture

The compiler MUST be organized as a linear pipeline with these stages, in order:
`Parser → AST → Logic Synthesis → 3D Place and Route → NBT Generation`.

Each stage MUST accept a typed input artifact and produce a typed output
artifact. Stages MUST be invocable in isolation (for testing, debugging, and
intermediate dumps). Skipping stages, back-edges between stages, or
cross-stage shortcuts are prohibited; new concerns get a new stage, not a
side channel.

**Rationale**: A clean pipeline makes the compiler debuggable, parallelizable,
and incrementally testable — and matches decades of compiler-engineering
practice.

### III. Module Isolation via Cargo Workspace

Parsing logic, graph algorithms (synthesis, placement, routing), and
file-format I/O (NBT, `.schem`, `.litematic`) MUST live in independent
workspace crates. Each crate MUST compile, test, and be benchmarked on its
own. Cross-crate dependencies MUST form a directed acyclic graph — no
cycles. A crate MUST NOT pull in another crate solely to share types;
shared types belong in a dedicated `core`/`types` crate.

**Rationale**: Isolation enables parallel development, faster incremental
builds, and reuse of components (e.g., the NBT crate by external tools).

### IV. Built for High-Throughput Compilation

The compiler MUST be designed for large workloads (e.g., compiling
processor-scale designs). Hot loops MUST avoid unnecessary allocations and
copying. Data-parallel stages (synthesis passes, routing) SHOULD use `rayon`
or equivalent. Performance-sensitive components MUST have `criterion`
benchmarks. Algorithmic complexity MUST be documented; any change that
worsens asymptotic complexity of a hot path requires explicit justification
in the PR description.

**Rationale**: Compiling a 16-bit CPU is the target workload, not a 4-bit
toy. Performance is a feature, not a polish step.

## Technology & Performance Constraints

- **Toolchain**: Rust stable, edition 2021, MSRV pinned in workspace `Cargo.toml`.
- **Output formats**: Sponge `.schem` and Litematica `.litematic`. Minecraft
  block-ID compatibility MUST be documented per release.
- **Dependencies**: Prefer well-maintained crates (`nom`/`chumsky` for parsing,
  `petgraph` for graphs, `fastnbt`/`hematite-nbt` for NBT, `rayon` for
  parallelism, `miette`/`ariadne` for diagnostics). New top-level dependencies
  require justification.
- **CLI**: A `redstonebuilder` binary crate MUST expose the full pipeline and
  per-stage entry points (e.g., `--dump-ast`, `--dump-netlist`).
- **`unsafe`**: Allowed only with a comment proving soundness and a unit test
  covering the invariant; default to safe Rust.

## Development Workflow & Quality Gates

Every PR MUST pass, locally and in CI:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo bench --workspace --no-run` (benchmarks must at least compile)

Additional requirements:

- Public APIs MUST have rustdoc with at least one runnable example.
- Changes to parser grammar, IR shape, or NBT layout MUST include
  regression tests with golden-file fixtures.
- Code review MUST explicitly verify compliance with Principles I–IV;
  violations require a Complexity Tracking entry in the corresponding
  implementation plan.

## Governance

This constitution supersedes other engineering practices and style guides
within the project. When a conflict arises, the constitution wins; the other
document is updated.

**Amendments**: any change requires (a) a PR editing this file, (b) a
rationale in the PR description, (c) a version bump per the policy below,
and (d) propagation to dependent templates under `.specify/templates/`.

**Versioning policy** (semantic):

- **MAJOR**: Backward-incompatible governance changes, principle removal, or
  redefinition that invalidates prior compliance.
- **MINOR**: New principle added, or a section materially expanded.
- **PATCH**: Clarifications, wording fixes, typo corrections, non-semantic
  refinements.

**Compliance review**: every PR is reviewed against the principles. Justified
deviations are tracked in the plan's Complexity Tracking table; unjustified
deviations block merge.

**Version**: 1.0.0 | **Ratified**: 2026-05-17 | **Last Amended**: 2026-05-17
