# The measurements

This directory holds the numbers of the comparison, in the format of
[`docs/PROTOCOL.md`](../docs/PROTOCOL.md), and the baseline of the
benchmarks of the launch path.

| File | What it is | Committed |
|---|---|---|
| `ours.json` | what **our** launcher measured on **this** machine | no |
| `reference.json` | what the **reference** launcher measured, by the same protocol | no |
| `baseline.json` | the baseline of the benchmarks of **this** machine | yes |
| `jvm-flags.json` | the start of the virtual machine with the flags of the version and with their variants | yes |
| `jvm-flags-recheck.json` | the same measurement, made again | yes |
| `flags-ab.json` | the profiles of the flags of the virtual machine, measured one after another | yes |

The first two are the numbers of a particular machine on a particular day.
They are ignored by git: a number of another machine would be a lie about
this one.

The baseline is committed on purpose, but it belongs to the machine of
its author: the numbers of a benchmark are comparable **within one
machine only**. The CI does not trust it — it measures the base of the
branch on the same runner as the head, right before it, and compares
the two. See below.

The three measurements of the flags are committed for the same reason as
the baseline, and for one more: **a flag of the virtual machine is only
allowed in the product when a measurement on this machine showed that it
wins.** The measurement is the reason the flag is there, so the reason
travels with the flag — and a flag without a measurement is a claim, and
claims are not shipped.

## How to take our measurement

```powershell
# the launcher has to be built first.
#
# It is `npm run tauri:build` and not `cargo build --release`: the
# command of Tauri turns on the feature `custom-protocol`, and without
# it the binary opens the address `localhost:1420` in its window
# instead of the interface that is built into it.
npm run tauri:build

# a quick check on three runs
.\scripts\measure.ps1 -Runs 3 -Scenario 26.3 -GameVersion 26.3 -Out benchmarks\ours.json

# the measurement of the protocol: thirty runs
.\scripts\measure.ps1 -Runs 30 -Scenario 26.3 -GameVersion 26.3 -Out benchmarks\ours.json
```

The scenario name matters: installation time is merged into the file
produced by the launcher for that version, and the file is named after the
scenario.

**Measure into a new file.** The script appends samples to the file you give
it. Measuring a second build into the same file mixes results from both
builds, so neither has its own series. Use a new file:

```powershell
.\scripts\measure.ps1 -Runs 30 -Scenario 26.3 -GameVersion 26.3 -Out benchmarks\ours-new.json
```

After the new file is complete, replace the old file with it. The script
writes each sample to disk, so an interrupted measurement keeps the runs
that already finished.

The script asks the player to click "Play" once per run. It watches the
mouse, the processes and the clock — and nothing else.

## How to take the measurement of the flags

Two measurements, at two levels.

```powershell
# the start of the virtual machine alone: the flags of the version
# 26.3 and their variants, thirty runs of each
.\scripts\measure-jvm.ps1 -Runs 30 -Out benchmarks\jvm-flags.json

# the game itself: the profiles of the flags, one after another
.\scripts\measure-profiles.ps1 -Runs 8 -Profiles default,measured,measured_g1 -Fresh

# the comparison out of the files of the profiles
.\scripts\measure-profiles.ps1 -Report -Profiles default,measured,measured_g1
.\scripts\compare-profiles.ps1
```

**Measure on a quiet machine.** A number of a busy machine is a number
of the other work, and the series of a busy machine are wide enough to
hide the effect that is being measured. The script
`scripts\wait-for-quiet.ps1` waits until the machine has been quiet
before the measurement starts:

```powershell
.\scripts\wait-for-quiet.ps1
if ($LASTEXITCODE -eq 0) { .\scripts\measure-jvm.ps1 -Runs 30 }
```

The threshold of the quiet is **45 % of the processor**, and that is not
a round number chosen to be lenient: the machine of a person at work is
never empty. A browser, a chat, the system itself take twenty to thirty
per cent of it all the time, and that is not "another work" — it is the
machine. What the guard has to keep out is the **big** work, and that
takes eighty per cent and more.

The threshold does not decide whether the numbers are good. The variants
are taken one after another, round by round, so a background that does
not change takes the same share out of every variant, and the middle of
thirty rounds puts it out. That is not a guess: two runs of the same
eight variants — one with the machine at thirty per cent, one at seventy
— gave middles that differ by three per cent. The load that really was
there is written into the file of the measurement (`load_percent` — the
worst moment, `load_median_percent` — the usual one), so that a reader
can see it without asking, and the noise of the series decides whether
they may be compared at all.

The first measurement needs no game and no player: it starts the virtual
machine with the flags of the version, the machine builds its heap — which
is where the flags act — finds no class to run and leaves. That is where
the cost of a flag shows: the flags are read when the virtual machine
starts.

The second measurement is the game, with the profiles of the flags. The
profiles are measured **one after another** — profile, profile, profile
— and not each one in a long run of its own: a machine that gets busier
changes the numbers of every profile in the same way, and when the
profiles are measured one after another, the difference between them is
still the difference of the flags.

**The archive of the classes is not among the profiles.** The profile
`measured_cds` was the archive of the classes: the game is started once
with `-XX:ArchiveClassesAtExit`, the virtual machine writes the archive
when it leaves, and the runs of that profile are started with
`-XX:SharedArchiveFile`. The archive cannot be made with the flags of
the version 26.3: the version prescribes `-XX:+UseCompactObjectHeaders`,
and the virtual machine **never finishes starting** with that flag and
with `-XX:ArchiveClassesAtExit` at once. The measurement of that is in
the section 9 of `docs/RESULTS.md`, and the profile is left out of the
run rather than measured on a machine that does not come.

## TLauncher game FPS baseline

For a same-PC comparison on Minecraft Forge 1.20.1, the reference setup
uses a physical copy of the TLauncher game directory. It copies the game
libraries, assets, mods, options, configuration, and saves so Minecraft
cannot write benchmark settings or gameplay data into the source
`.minecraft` directory.

The current machine's isolated copy is prepared at
`E:\C_Offload\MinLauncher\benchmarks\TLauncherReference`. To create a new
copy on another machine, run:

```powershell
.\scripts\prepare-tlauncher-reference.ps1
```

Collect 30 runs with the same save, viewpoint, resolution, and graphics
settings. The script temporarily points TLauncher at the copy, then restores
its previous game-directory setting after TLauncher is closed:

```powershell
.\scripts\measure-tlauncher-fps.ps1 -Runs 30
```

The script writes a JSON file under `%APPDATA%\MinLauncher\benchmarks` and
retains completed runs if collection is interrupted. Import that file from
MinLauncher → Производительность → Импортировать замеры TLauncher. The
importer accepts only 30 or more matched FPS/frame-time samples from this PC
and Minecraft 1.20.1.

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
