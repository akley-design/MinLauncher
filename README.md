# MinLauncher

A Minecraft launcher that starts **at least 20 % faster** than the reference
launcher, and optimises the game itself on the way.

The speed promise is not a slogan — it is measured. The metrics are fixed in
advance, the protocol of the measurement is written down, and every result
carries the metadata of the machine it was taken on, so that it can be
reproduced. See [`docs/PLAN.md`](docs/PLAN.md) for the metrics and
[`docs/PROTOCOL.md`](docs/PROTOCOL.md) for the procedure.

---

## What is in here

```
crates/
  ml-core        shared types, the error taxonomy, the path layout, the rules
  ml-storage     SQLite + JSON persistence
  ml-minecraft   Mojang metadata, versions, libraries, assets, the Fabric loader
  ml-launch      JVM discovery, argument construction, process supervision
  ml-desktop     the Tauri shell: IPC commands, state, plugins
  ml-bench       the measurement harness and the comparison protocol
apps/
  ui             React 19 + TypeScript + Vite
  java/
    agent        a `-javaagent` that measures the start-up and the frames
    mod          a Fabric mod that makes the game itself faster
```

The launcher is written in Rust, the interface in TypeScript, and the part
that touches the game is Java: one piece is a Java agent that measures, the
other is a Fabric mod that changes the game.

## The two halves of the speed promise

**The launcher starts faster.** The whole CPU path of a launch fits in about
0.2 ms, so the win cannot come from the computations — it comes from what the
launcher *waits* for: the network, the disk, and the start of the JVM. The
launcher therefore caches the parsed version document, keeps its library store
content-addressed so that a second version reuses what the first one
downloaded, and does not pay for a round trip over IPC that it does not need.

**The game itself runs faster.** The agent measures how long the game took to
start and how it behaves while it runs. The mod changes the game: it turns off
the expensive effects that are on by default, tunes the renderer, and reads its
settings from a file the launcher writes, so the user keeps the last choice
between launches.

## How the promise is checked

The measurement is done by an outside observer. `scripts/measure.ps1` starts
the launcher, waits for its window, watches the mouse for the click on
"Play", waits for the window of the game, and writes down what it saw. It
reads the processes, the windows and the clock — and nothing else. The
launcher does not time itself, because a launcher that announces its own
speed does not measure its speed.

```bash
cargo build --release
```

```powershell
# thirty runs, the number the protocol asks for
.\scripts\measure.ps1 -Runs 30 -Out benchmarks\ours.json

# the table M1–M8, with a verdict for every metric
cargo run -p ml-bench --bin report -- --ours benchmarks\ours.json --reference benchmarks\reference.json
```

The report exits with `1` when a target was missed and with `2` when it
could not be built at all, so it can be a gate and not only a page.

The whole procedure — what is measured, when, and where it breaks — is
written down in [`docs/PROTOCOL.md`](docs/PROTOCOL.md). The reference side
of the comparison is a file that arrives from outside and was taken by that
same procedure: the reference launcher is never started, never reconfigured
and never touched.

Two of the eight metrics cannot be seen from outside at all — the length
of an installation and the behaviour of the game — so the launcher measures
those itself and writes them into its own directory, in the same format.
The measurer picks them up from there.

## The gate of the build

The eight metrics need a player, a game and a mouse, so they are not part
of the build. The **CPU path** of a launch is: the benchmarks of `ml-bench`
measure the parsing of a version document, the resolution of its libraries
and the building of the command of the game — the work the launcher does
while the player waits.

```bash
cargo bench -p ml-bench
```

The CI measures the base of the branch on the same runner as the head and
compares the two: a scenario is a regression when its median got more than
10 % slower and the noise of the machine cannot explain it. The numbers go
into the report of the run, and the job fails on a regression. The game
metrics (M5, M6) are not among them — they need a graphics card and a live
game — and the report says so instead of hiding it.

## The Fabric loader, and why it is in the way

To change the game, the launcher puts the Fabric loader in front of it. The
contract of that loader was established by reading the bytecode of
`fabric-loader-0.19.5.jar` with `javap -p -c`, not by reading documentation
that may not match the version in use. The launcher therefore knows exactly
which system properties the loader reads, how it finds the game jar, and where
it looks for the mods.

Two things about that are worth knowing, because both of them break a launch
in a way no unit test can show:

* The loader refuses to start when two jars on the classpath carry the same
  class. The game of 1.21.4 carries ASM 9.6 and the loader carries ASM 9.10.1,
  so the library of the loader has to *replace* the library of the game with
  the same identity, not stand next to it.
* The path of a library is written down in the version document, and reading
  it back from the coordinate alone is not always the same thing: a version of
  two pieces, like `9.6`, reads as a version and an extension. Eight of the
  libraries of 1.21.4 are like that.

There is a test that proves the whole chain against the real game — it
downloads Minecraft, the loader and the mod, builds the command the launcher
builds, and starts the game:

```bash
MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test fabric_end_to_end -- --nocapture
```

It needs the network and a few hundred megabytes, so it does not run by
itself. What it asserts is simple and was not obvious before the game was
actually running:

```
Loading Minecraft 1.21.4 with Fabric Loader 0.19.5
Loading 5 mods:
	- fabricloader 0.19.5
	- java 26
	- minecraft 1.21.4
	- minlaucher-optimisations 0.1.0
```

There is a second one, for the step before that: it installs a real version
of the game from the servers of Mojang — the client jar, the libraries and
every one of the 4039 assets — and checks that the result on the disk is
the layout the launcher and the game expect:

```bash
MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test install_end_to_end -- --nocapture
```

It found a bug that no unit test could have: the address of an asset was
built from the *tail* of its hash, and the server answered 404 to every one
of them. The address of an asset is a two-letter directory and the **whole**
hash, and the store on the disk is laid out the same way — which is what
the game itself reads.

And a third one, for the Java the game needs: it downloads a runtime of
Mojang — every file of it separately, as the manifest of the runtime says —
and checks that the launcher finds the Java it just put on the disk:

```bash
MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test runtime_end_to_end -- --nocapture
```

It found a second bug: the Java of the system was newer than the runtime
Mojang publishes, and the launcher took it. Now the runtime the launcher
downloaded itself comes first — the version document of the game names the
Java it was tested with.

## Building it

The Rust side needs a Rust toolchain; the pinned one is in
`rust-toolchain.toml`.

```bash
cargo build --workspace          # the launcher
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The interface:

```bash
npm ci
npm run typecheck
npm run lint
npm run format:check
npm run test:run
```

The Java part needs a JDK 21 and Gradle:

```bash
cd apps/java
./gradlew build
```

### A note on the paths of this machine

This project is developed on a machine whose home directory contains Cyrillic
characters, and two tools do not cope with that:

* The resource compiler of Windows — `rc.exe` of the SDK — reads the file it
  was given through the Windows ANSI code page, and the paths of the icon and
  of the manifest are written *into* that file. The build directory is
  therefore moved to an ASCII-only path (`.cargo/config.toml`).
* The Gradle wrapper fails the same way, and the build directory of the Java
  part is moved for the same reason (`apps/java/gradle.properties`).

Neither of these is a property of the project — on a machine with an ASCII
path both lines can go away.

## Tests

| Part | Tests |
|------|-------|
| Rust | 477 |
| Java agent | 23 |
| Java mod | 18 |
| Interface | 68 |
| **Total** | **586** |

Plus the end-to-end tests against the real game and the real servers of
Mojang, which are switched on by hand.

## Releasing it

The release build needs the MSVC toolchain, the Windows SDK and the "Desktop
development with C++" workload of Visual Studio; `rust-toolchain.toml` pins
the toolchain, and [`docs/RELEASE.md`](docs/RELEASE.md) describes the whole
procedure — the environment, the installer, the signing of the update and the
check that the installed application works on a clean machine.

```bash
npm run tauri:build
```

## The plan

[`docs/PLAN.md`](docs/PLAN.md) holds the metrics, the architecture, the
phases, and the record of what was checked against the real data — including
the things that turned out to be wrong and had to be fixed.
