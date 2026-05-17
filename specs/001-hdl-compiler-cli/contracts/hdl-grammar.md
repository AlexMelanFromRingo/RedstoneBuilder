# Contract — HDL Surface Grammar (Verilog Subset)

**Crate**: `rb-parser`
**Grammar file**: `rb-parser/src/hdl.pest` (PEG, used by `pest` v2.x).

This document is the **normative grammar specification** required by
FR-002 / FR-015. The `.pest` file in source is the machine-readable mirror;
this document is the human-readable contract.

## Lexical

```text
WHITESPACE   = _{ " " | "\t" | "\r" | "\n" }
COMMENT      = _{ "//" ~ (!"\n" ANY)* | "/*" ~ (!"*/" ANY)* ~ "*/" }
IDENT        =  @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
KEYWORD      = @{
    "module" | "endmodule" | "input" | "output" | "wire"
  | "and" | "or" | "not" | "xor"
  | "always" | "posedge" | "negedge"
  | "dtrigger" | "memcell"
}
```

Identifiers MUST NOT collide with keywords (handled by `pest` rule
ordering: `KEYWORD ~ !ASCII_ALPHANUMERIC` matched before `IDENT`).

## Syntax

```text
file         = { SOI ~ module ~ EOI }
module       = { "module" ~ IDENT ~ "(" ~ port_list ~ ")" ~ ";"
               ~ module_item*
               ~ "endmodule" }

port_list    = { port_decl ~ ("," ~ port_decl)* }
port_decl    = { ("input" | "output") ~ IDENT }

module_item  = _{ wire_decl | gate_inst | always_block }
wire_decl    = { "wire" ~ IDENT ~ ("," ~ IDENT)* ~ ";" }

gate_kind    = { "and" | "or" | "not" | "xor" }
gate_inst    = { gate_kind ~ IDENT ~ "(" ~ conn_list ~ ")" ~ ";" }
conn_list    = { conn ~ ("," ~ conn)* }
conn         = { "." ~ IDENT ~ "(" ~ IDENT ~ ")" }   // Verilog named-port form

always_block = { "always" ~ "@" ~ "(" ~ edge_spec ~ ")"
               ~ "begin" ~ seq_assign+ ~ "end" }
edge_spec    = { ("posedge" | "negedge") ~ IDENT }
seq_assign   = { stateful_inst | nonblock_assign }

stateful_inst    = { ("dtrigger" | "memcell") ~ IDENT ~ "(" ~ conn_list ~ ")" ~ ";" }
nonblock_assign  = { IDENT ~ "<=" ~ IDENT ~ ";" }
```

## Out-of-subset constructs (must reject with diagnostic)

These tokens, if encountered, produce
`error[unsupported]: <token> is not supported in v1`:

- `parameter`, `localparam`
- `generate`, `endgenerate`
- Bit-select / part-select (`a[3]`, `a[7:0]`)
- Multi-bit busses (`wire [7:0] x;`)
- Arithmetic, logical, relational operators (`+ - * / & | ^ < >`)
- `assign` (continuous assignment) — gate primitives only in v1
- `function`, `task`, `case`, `if`/`else`, `for`, `while`
- `reg` declarations (we model state only via `dtrigger`/`memcell`)
- Inline include / preprocessing directives

## Connection naming convention (per gate kind)

Named-port form is required (positional connection is rejected). Expected
port names for each `gate_kind` / `stateful_inst`:

| Kind        | Required `.PORT(...)` names                                |
|-------------|-------------------------------------------------------------|
| `and`,`or`,`xor` | `.A(...)`, `.B(...)`, ... (≥ 2 inputs), `.Y(...)` output |
| `not`       | `.A(...)`, `.Y(...)`                                       |
| `dtrigger`  | `.D(...)`, `.CLK(...)`, `.Q(...)`                          |
| `memcell`   | `.DATA(...)`, `.WRITE(...)`, `.Q(...)`                     |

Unknown or missing port → `error[bad_port]: gate '<inst>' missing port <PORT>`.

## Example — half adder

```verilog
module half_adder(input a, input b, output sum, output carry);
    xor x1(.A(a), .B(b), .Y(sum));
    and a1(.A(a), .B(b), .Y(carry));
endmodule
```

## Example — clocked D flip-flop

```verilog
module dff_demo(input d, input clk, output q);
    always @(posedge clk) begin
        dtrigger ff(.D(d), .CLK(clk), .Q(q));
    end
endmodule
```

## Grammar evolution policy

Any change to this grammar is a **MAJOR** bump per project constitution
(see `.specify/memory/constitution.md`, Governance section), because it
changes the user-facing contract of FR-002.
