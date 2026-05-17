<!-- SPECKIT START -->
Active feature: **001-hdl-compiler-cli** — Rust CLI compiling a Verilog-subset
HDL into Minecraft Litematica `.litematic` schematics (MC Java Edition 26.1).

Authoritative documents (read these before editing code):

- Constitution: `.specify/memory/constitution.md`
- Spec: `specs/001-hdl-compiler-cli/spec.md`
- **Plan: `specs/001-hdl-compiler-cli/plan.md`** ← start here for technical context
- Phase 0 research: `specs/001-hdl-compiler-cli/research.md`
- Phase 1 data model: `specs/001-hdl-compiler-cli/data-model.md`
- Phase 1 contracts: `specs/001-hdl-compiler-cli/contracts/`
- Quickstart: `specs/001-hdl-compiler-cli/quickstart.md`

Stack at a glance: Rust 2021, `clap` (derive), `pest` (PEG), `petgraph`,
`fastnbt` + `flate2`, `thiserror` + `miette`. Workspace crates:
`rb-core`, `rb-parser`, `rb-synthesis`, `rb-nbt`, `redstonebuilder` (bin).
<!-- SPECKIT END -->
