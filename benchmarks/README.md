# The measurements

This directory holds the numbers of the comparison, in the format of
[`docs/PROTOCOL.md`](../docs/PROTOCOL.md).

| File | What it is | Committed |
|---|---|---|
| `ours.json` | what **our** launcher measured on **this** machine | no |
| `reference.json` | what the **reference** launcher measured, by the same protocol | no |
| `baseline.json` | the numbers the CI compares against | yes |

The first two are the numbers of a particular machine on a particular day.
They are ignored by git: a number of another machine would be a lie about
this one. The baseline is committed on purpose — it is the point the CI
compares the new run against.

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
