# The measurements

This directory holds the numbers of the comparison, in the format of
[`docs/PROTOCOL.md`](../docs/PROTOCOL.md), and the baseline of the
benchmarks of the launch path.

| File | What it is | Committed |
|---|---|---|
| `ours.json` | what **our** launcher measured on **this** machine | no |
| `reference.json` | what the **reference** launcher measured, by the same protocol | no |
| `baseline.json` | the baseline of the benchmarks of **this** machine | yes |

The first two are the numbers of a particular machine on a particular day.
They are ignored by git: a number of another machine would be a lie about
this one.

The baseline is committed on purpose, but it belongs to the machine of
its author: the numbers of a benchmark are comparable **within one
machine only**. The CI does not trust it — it measures the base of the
branch on the same runner as the head, right before it, and compares
the two. See below.

## How to take our measurement

```powershell
# the launcher has to be built first
cargo build --release

# a quick check on three runs
.\scripts\measure.ps1 -Runs 3 -Out benchmarks\ours.json

# the measurement of the protocol: thirty runs
.\scripts\measure.ps1 -Runs 30 -Out benchmarks\ours.json
```

The script asks the player to click "Play" once per run. It watches the
mouse, the processes and the clock — and nothing else.

## How to read the numbers

```powershell
cargo run -p ml-bench --bin report -- --ours benchmarks\ours.json --reference benchmarks\reference.json
```

The report prints the table M1–M8 with a verdict for every metric, and
returns `1` when a target was missed.

Without a reference file the table still appears: the reference column
holds "—" and the metrics that were not compared say so. Nothing is
guessed and nothing is silently zero.

## The benchmarks of the launch path

The metrics M1–M8 need a player, a game and a mouse. The CPU path of a
launch needs none of that: the benchmarks of `ml-bench` measure the
parsing of a version document, the resolution of its libraries, the
building of the command of the game — the work the launcher does while
the player waits.

```bash
cargo bench -p ml-bench
```

The run is compared with the baseline of the same machine:

```bash
# the baseline of this machine, written from the last run
cargo run --profile bench -p ml-bench --bin bench -- \
    --criterion target/criterion --save benchmarks/baseline.json

# the comparison
cargo run --profile bench -p ml-bench --bin bench -- \
    --criterion target/criterion --baseline benchmarks/baseline.json
```

A scenario is a **regression** when its median got more than 10 %
slower and the confidence intervals of the two medians do not overlap: a
slowdown that the noise of the machine can explain is not a regression,
and a gate that falls on noise is a gate nobody reads. The command
returns `1` when there is one, and the report says which scenario and by
how much.

The game metrics (M5, M6) are not among the benchmarks: they need a
graphics card and a live game. The CI covers the CPU path only, and the
report says so instead of hiding it.
