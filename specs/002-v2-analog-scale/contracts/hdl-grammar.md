# Contract — HDL Surface Grammar (v2 deltas)

**Crate**: `rb-parser`
**Extends**: `specs/001-hdl-compiler-cli/contracts/hdl-grammar.md`

This document records **only the v2 additions**. The v1 Verilog-subset
grammar carries over unchanged.

## New keywords (added to `reserved`)

```text
"analog"
"comparator" "subtract" "compare"
"observer"
"target_block"
"DELAY" "LOCK"        // attributes used inside `repeater` connection list
```

`slab` and `glass` are **not** added to the HDL surface (router-only,
per FR-V07 / FR-V18). They never appear in source.

## Grammar additions

```text
// `wire` declaration now optionally typed with `analog`.
wire_decl   = { wire_kind ~ ident ~ ("," ~ ident)* ~ ";" }
wire_kind   = { kw_wire | (kw_analog ~ kw_wire) }

// `repeater` is promoted from internal router primitive to a
// first-class HDL primitive (parallel to v1's `and`/`or`/`not`/`xor`).
gate_kind   = { kw_and | kw_or | kw_not | kw_xor | kw_repeater | kw_target_block }

// `comparator` carries an explicit MODE attribute.
comparator_inst = {
    kw_comparator ~ ident ~ "(" ~ conn_list ~ "," ~ mode_attr ~ ")" ~ ";"
}
mode_attr   = { ".MODE" ~ "(" ~ (kw_compare | kw_subtract) ~ ")" }

// `observer` instantiation — exactly one input port, one output port.
observer_inst = {
    kw_observer ~ ident ~ "(" ~ conn ~ "," ~ conn ~ ")" ~ ";"
}

// `module_item` extended.
module_item = _{ wire_decl
               | gate_inst
               | comparator_inst
               | observer_inst
               | always_block }
```

`repeater r(.IN(a), .OUT(b), .DELAY(3), .LOCK(c));` parses as a
`gate_inst` whose `gate_kind == kw_repeater`; the `DELAY` and `LOCK`
attributes are recognised by the validator (not the grammar — they
look like ordinary named-port connections).

## New rejections (validation errors)

| Source pattern                      | Diagnostic                                                                  |
|-------------------------------------|-----------------------------------------------------------------------------|
| `analog wire X; and g(.A(X), ...)`  | `SignalKindMismatch { wire: X, expected: Boolean, found: AnalogStrength }` |
| `wire X; comparator c(.A(X), ...)`  | `SignalKindMismatch { wire: X, expected: AnalogStrength, found: Boolean }` |
| `repeater r(.DELAY(5), ...)`        | `BadDelay { actual: 5, range: "1..=4" }`                                   |
| `comparator c(.MODE(plus), ...)`    | `BadMode { actual: "plus", allowed: ["compare", "subtract"] }`             |
| `observer o(.A(x), .B(y));`         | `BadPort { inst: o, detail: "observer ports are .WATCH/.OUT, not .A/.B" }` |

## Backward compatibility

- All v1 grammar productions remain valid as-is.
- A bare `wire X;` continues to mean `Boolean` (FR-V02).
- A bare `repeater r(...)` with no `.DELAY(...)` defaults to delay 1
  (matches v1 internal usage).

## Example — v2 analog adder

```verilog
module analog_add(input clk, analog wire a, analog wire b, analog wire sum);
    // Mode: subtract — comparator computes max(0, A - !B) = arithmetic sum
    // when configured by the synthesiser per FR-V03.
    comparator c1(.A(a), .B(b), .Y(sum), .MODE(subtract));
endmodule
```

## Example — v2 monostable

```verilog
module monostable(input trigger, output pulse);
    wire pulse_pre;
    observer o(.WATCH(trigger), .OUT(pulse_pre));
    repeater r(.IN(pulse_pre), .OUT(pulse), .DELAY(1));
endmodule
```

## Example — v2 D-flip-flop with explicit repeater on clock

```verilog
module dff_clocked(input d, input clk_raw, output q);
    wire clk_buf;
    repeater clk_rep(.IN(clk_raw), .OUT(clk_buf), .DELAY(2));
    always @(posedge clk_buf) begin
        dtrigger ff(.D(d), .CLK(clk_buf), .Q(q));
    end
endmodule
```
