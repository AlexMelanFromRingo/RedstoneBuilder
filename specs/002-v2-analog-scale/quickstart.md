# v2.0 Quickstart — Analog Signals & 10 000-Gate Scale

This complements `specs/001-hdl-compiler-cli/quickstart.md` (v1
quickstart, still current). v2 adds:

1. Analog wires + comparator-based arithmetic.
2. Observer / repeater (delay/lock) / target as user-instantiable
   primitives.
3. Hard memory & routing-iteration caps for the 10 000-gate scale.
4. `--stats` for end-of-compile per-stage breakdown.
5. A new placement algorithm (simulated annealing); v1's row-based
   placer remains available via `--placer greedy`.

## Hello, analog adder

```verilog
// examples/analog_add.hdl
module analog_add(analog wire a, analog wire b, analog wire sum);
    comparator c1(.A(a), .B(b), .Y(sum), .MODE(subtract));
endmodule
```

Compile:

```bash
target/release/redstonebuilder examples/analog_add.hdl -o analog_add.litematic --stats
```

Expected stdout:

```text
wrote analog_add.litematic (gates=1, blocks=N, footprint=WxHxD, time=T s)
```

Expected stderr (because `--stats`):

```text
stats: parse=Ts synth=Ts timing=Ts place=Ts route=Ts(iters=K) nbt=Ts peak_ram=MMB
```

Load in Minecraft 26.1 + Litematica. Drive both inputs with redstone
levers feeding comparator-based strength sources at varying levels;
observe `sum` reads the arithmetic sum (clamped at 15).

## Hello, monostable

```verilog
// examples/monostable.hdl
module monostable(input trigger, output pulse);
    wire pulse_pre;
    observer o(.WATCH(trigger), .OUT(pulse_pre));
    repeater r(.IN(pulse_pre), .OUT(pulse), .DELAY(1));
endmodule
```

In-game test: hold the trigger lever for various durations; `pulse`
goes high for exactly 1 redstone tick on each press.

## Scaling to 10 000 gates

```bash
# Generate a synthetic stress design (script in tools/stress-gen.py
# — to be added in /speckit-tasks).
python3 tools/stress-gen.py --gates 10000 > big.hdl

# Default flags handle this. --stats shows where time goes.
target/release/redstonebuilder big.hdl -o big.litematic --stats
```

If you hit a cap:

| Symptom (exit code) | Meaning                                  | Quick fix                                 |
|---------------------|------------------------------------------|-------------------------------------------|
| 7 (routing iters)   | Router didn't converge in budget         | `--max-routing-iterations 256`            |
| 8 (RAM cap)         | Compile would exceed 4 GB default        | `--max-ram 8192`                          |
| 9 (timing race)     | Static timing detected a data race       | Add an explicit `repeater` to skew slow path, or use `--allow-timing-races` to suppress |

## Swapping placers

```bash
# Default — simulated annealing, scales to 10k+ gates.
redstonebuilder big.hdl -o big.litematic

# v1-compatible row-based — faster for ≤ 20 gates, identical output to v1.
redstonebuilder big.hdl -o big.litematic --placer greedy
```

## Backward compatibility

All v1 example files (`half_adder.hdl`, `dff_demo.hdl`,
`full_adder.hdl`, `ripple_adder_8bit.hdl`) compile under v2 with no
source edits:

```bash
for f in examples/half_adder.hdl examples/dff_demo.hdl examples/full_adder.hdl examples/ripple_adder_8bit.hdl; do
  redstonebuilder "$f" -o "/tmp/$(basename $f .hdl).litematic"
done
```

Both `--placer sa` (default) and `--placer greedy` produce valid
schematics for these inputs. With `--placer greedy`, output is
byte-identical to v1.

## Reproducible builds

Same input + same flags + same tool version → byte-identical
`.litematic` (v1 FR-017 carried over). Verify:

```bash
redstonebuilder big.hdl -o a.litematic --seed 42
redstonebuilder big.hdl -o b.litematic --seed 42
sha256sum a.litematic b.litematic   # same hash
```

`--seed` now also seeds the SA placer (in addition to the legacy
router RNG slot).
