# The corpus of the versions of the game

Every version of the game is described by a JSON document, and no two of
them are alike. The launcher has to start **every** version, so the
contract over those documents is checked over **all** of them and not
over a few examples.

This directory is that contract, frozen:

| | |
|---|---|
| `versions/<id>.json` | one document per version, stripped of the fields the launcher does not read — 920 files |
| `manifest.json` | the census of the corpus: one line per version — the SHA-1 of the document, the kind of the version, the form of its arguments, the layout of its assets, the number of the arguments and the placeholders it uses |

## What was taken and what was left out

The documents are the ones Mojang publishes, taken on **21 October 2026**
from `piston-meta.mojang.com`. The stripping is done in one place —
`ml_minecraft::corpus::strip()` — and it is used both by the binary that
builds the corpus and by the test that compares the corpus with the live
documents, so both sides always read the same shape.

What was left out and why:

* **the downloads of the libraries** — the path of a library is derived
  from its Maven coordinates. The corpus was checked against every
  version: the derived path and the path the document declares are the
  same everywhere, in all the libraries of all the versions. So dropping
  them changes nothing the launcher builds.
* **the logging configuration** — the launcher does not read it.
* **the server jar** — a launcher that starts the game has no use for it.

What was kept: the identifier, the kind, the entry point, the assets and
their index, the Java the version asks for, the libraries with their
rules, the arguments and the publication time.

The whole corpus is **5.4 MiB** — 4.2 MiB of libraries with their rules,
0.8 MiB of arguments and 0.1 MiB of the `minecraftArguments` strings.

## How to refresh it

```bash
cargo run -p ml-minecraft --bin fetch_corpus
```

The binary downloads the manifest and every document, strips them, writes
the files and rebuilds `manifest.json`. The documents of the versions
Mojang has dropped are removed.

**The number of versions is written out in the test.** A new version of
the game therefore fails `every_document_of_the_corpus_parses` until the
number in `tests/version_corpus.rs` is changed — a refresh is a decision,
not an accident.

## How to check it against Mojang

```bash
cargo test -p ml-minecraft --test version_corpus -- --ignored --nocapture
```

The test `the_corpus_is_the_one_of_mojang` downloads the manifest and
every document again, strips them the same way and compares them with the
corpus, byte for byte. It needs the network, so it is not run by itself.
