# Specification Quality Checklist: v2.0 — Analog Signals, Event-Driven Primitives, and 10 000+ Gate Scale

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-17
**Last validated**: 2026-05-17 (post-/speckit-clarify on P&R architecture)
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Resolved Clarifications

### Session 2026-05-17 (initial /speckit-specify)

| # | Topic                       | Decision                                                                                            |
|---|-----------------------------|-----------------------------------------------------------------------------------------------------|
| 1 | Analog HDL surface syntax   | `analog wire <name>;` type modifier. No bit-vector busses.                                          |
| 2 | RAM-cap overflow behaviour  | Hard abort with structured diagnostic + distinct exit code.                                         |
| 3 | Observer semantic model     | Asynchronous MC-native; data input = cycle-cut.                                                     |

### Session 2026-05-17 (P&R architecture clarify)

| # | Topic                                | Decision                                                                                            |
|---|--------------------------------------|-----------------------------------------------------------------------------------------------------|
| 4 | Iterative router non-convergence     | Hard abort via new `--max-routing-iterations` flag + distinct exit code (FR-V16).                   |
| 5 | Placement ↔ routing feedback         | Strict single-pass P→R. Re-place feedback deferred to v3 (FR-V17).                                  |
| 6 | Slab / glass — user-instantiable?    | Router-only. Asymmetric vertical-cost edges in cost map; never in HDL/AST (FR-V07, FR-V18).         |

## Coverage Map

| Category                                              | Status   |
|-------------------------------------------------------|----------|
| Functional Scope & Behavior                           | Resolved |
| Domain & Data Model (signal types / prims)            | Resolved |
| Interaction & UX Flow                                 | Resolved |
| Non-Functional — Performance (10 k gates)             | Resolved (SC-V03) |
| Non-Functional — Scalability (RAM, routing iters)     | Resolved (FR-V09 / FR-V10 / FR-V16 / SC-V04 / SC-V08) |
| Non-Functional — Reliability (no OOM-kill, abort)     | Resolved (SC-V08, FR-V16) |
| Non-Functional — Observability (`--stats`)            | Resolved (FR-V14) |
| Non-Functional — Determinism (preserved)              | Resolved (Assumptions, FR-V13) |
| Integration & External Dependencies                   | Resolved (Litematica mod for MC 26.1) |
| Edge Cases & Failure Handling                         | Resolved |
| Constraints & Tradeoffs (single-pass P&R; router-only slab/glass) | Resolved (FR-V17, FR-V18) |
| Terminology & Consistency                             | Clear    |
| Completion Signals                                    | Clear    |

No Outstanding items.

## Deferred Items (intentionally out of v2 scope)

- **Iterative placement↔routing feedback** — v3 epic (per FR-V17).
- **User-instantiable slab/glass placement hints** in HDL — beyond scope; manual override path is via editing the `--dump-placement` JSON.
- **Multi-bit Verilog-style busses (`wire [3:0]`)** — explicitly rejected (Session 1, Q1). Future revisit only if user demand emerges.

## Notes

- Spec is ready for `/speckit-plan`.
- Architectural decisions land naturally: hard caps (RAM, routing
  iterations, footprint) form a coherent family with consistent UX
  (structured diagnostic + distinct exit code + user re-runs with
  larger cap).
- The slab/glass cost-map asymmetry (FR-V18) is the spec-level
  capture of the user's plan-level question about Minecraft physics
  in the cost map. The actual cost-function shape and routing-
  algorithm choice (A*-PathFinder vs negotiated-congestion vs other)
  remain plan-level — the spec mandates only the **observable
  outcome** (no silent downward leakage, FR-V07).
