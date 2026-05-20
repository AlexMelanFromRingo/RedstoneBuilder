# Contract — Static Timing Analysis (v2)

**Crate**: `rb-synthesis::timing`
**New stage** in the v2 pipeline (between cycle detection and
placement).

## Public surface

```rust
pub fn analyse_timing(
    netlist: &Netlist,
    cells:   &CellLibrary,
    cfg:     &TimingConfig,
) -> Result<TimingMap, TimingError>;

pub struct TimingConfig {
    /// Per-data-net arrival-tick mismatch tolerance. Default 0 ticks.
    pub data_race_tolerance: u32,
    /// Whether to treat data-race diagnostics as hard errors (default
    /// true) or warnings (when `--allow-timing-races` is set on the
    /// CLI).
    pub strict: bool,
}
```

## TimingMap

```rust
pub struct TimingMap {
    pub arrival:      BTreeMap<NodeIndex, Tick>,   // earliest tick the node's output is stable
    pub depart:       BTreeMap<NodeIndex, Tick>,   // tick by which all inputs of the node must be stable
    pub critical_path: Vec<NodeIndex>,             // longest-arrival path, for `--stats`
}
```

## Algorithm

```text
1. Build the combinational projection of the netlist
   (reuses `rb_synthesis::cycle::build_projection`).
2. Topologically sort the projection.
3. For each node n in topo order:
       arrival[n] = max over predecessors (arrival[pred] + cell_delay(pred → n))
       where cell_delay = cells[pred.kind].tick_delay if pred is a Gate,
                          0 if pred is a ModuleInput.
4. For every node n with multiple predecessors driving the *same* data
   port (i.e., converging into a combinational gate input):
       if max(arrival[preds]) - min(arrival[preds]) > cfg.data_race_tolerance:
           emit TimingError::DataRace
5. For module-output nodes: depart[out] = max over fanout (arrival[load]).
6. critical_path = trace_back(max(arrival)).
```

Clock-domain nets (signals named in `always @(posedge ...)` clauses
or driving `ClockIn` / `ObserverWatch` / `RepeaterLock` roles) are
**excluded** from race detection — clock skew is allowed and
expected, modelled by upstream repeater delay choices.

## TimingError

```rust
pub enum TimingError {
    DataRace {
        net:   NetId,
        sink:  NodeIndex,
        paths: Vec<(NodeIndex, Tick)>,   // converging predecessors + arrival ticks
    },
}
```

Maps to exit code **9**. Under `--allow-timing-races`, the pipeline
prints the diagnostic to stderr and continues with whatever timing the
synthesised cells naturally produce.

## Use by downstream stages

- **Placement** (SA cost): may include `critical_path` length as a
  secondary cost term (plan-level decision — empirical tuning).
- **Routing**: does not consume `TimingMap` directly in v2 (route is
  agnostic to tick budget; repeater insertion happens inside routes
  to maintain signal strength, separate concern).
- **`--stats`**: prints `critical_path_ticks=N` and `max_data_skew=K`.

## Determinism

Pure function of `(netlist, cells, cfg)` — no RNG. `BTreeMap` for
output to keep iteration order stable.

## Backward compatibility

For v1 example HDL files (boolean only, no observers, no explicit
repeater with custom delay), every `cell_delay` is taken from the v1
cell library and the analysis produces the same arrival ticks as v1
implicitly assumed. `DataRace` cannot fire on v1 inputs because
combinational gates have a single driver per input. So v1 examples
pass under v2's new timing stage without modification.
