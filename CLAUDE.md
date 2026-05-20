<!-- SPECKIT START -->
Active feature: **002-v2-analog-scale** — v2.0 refactor: analog signals,
event-driven primitives (observer / repeater with delay+lock / target_block),
and scaling to 10 000+ gates. Additive on top of v1 (`001-hdl-compiler-cli`).

Authoritative v2 documents (read these before editing code):

- Constitution: `.specify/memory/constitution.md`
- Spec: `specs/002-v2-analog-scale/spec.md`
- **Plan: `specs/002-v2-analog-scale/plan.md`** ← start here for technical context
- Phase 0 research: `specs/002-v2-analog-scale/research.md`
- Phase 1 data model: `specs/002-v2-analog-scale/data-model.md`
- Phase 1 contracts: `specs/002-v2-analog-scale/contracts/` (cli, hdl-grammar, router-ir, timing, minecraft-blocks)
- Quickstart: `specs/002-v2-analog-scale/quickstart.md`

v1 docs (`specs/001-hdl-compiler-cli/`) remain authoritative for the
v1 surface — v2 spec lists explicit FR/SC deltas; v1 FRs / contracts not
mentioned in the v2 spec are carried forward unchanged.

Stack additions in v2: `memory-stats` (RAM-cap enforcement), `rayon`
(parallel routing). Existing workspace crates: `rb-core`, `rb-parser`,
`rb-synthesis`, `rb-nbt`, `redstonebuilder` (bin). No new crates in v2.
<!-- SPECKIT END -->
