# The fixtures of the shapes

Eighteen documents, one per shape the document of a version can take.
They are copies of the documents of the corpus, but they are **pinned**:
they are not rewritten when the corpus is refreshed, so a version that
Mojang changes or drops leaves its shape behind.

The expectations of every fixture are written out in the test
`every_fixture_is_recognised` — the form of the arguments, the layout of
the assets, the Java the version asks for, the arguments it names, the
arguments it must not name and the placeholders it uses. They are not
taken from the corpus, so that a change of the corpus cannot change the
expectation with it.

| Fixture | What it pins |
|---|---|
| `26.3.json` | the newest release: `arguments`, the assets `34`, the Java 25, the `quickPlay` family and the resolution — and **no** `--userType` |
| `26.4-snapshot-5.json` | the newest snapshot: the same shape, the assets `36` |
| `1.21.4.json` | the version the mod is built for: `--userType` is back |
| `1.21.json` | the Java 21 |
| `1.18.2.json` | the Java 17 |
| `1.17.1.json` | the Java 16 |
| `1.16.5.json` | the Java 8 with the `arguments` block, and no `quickPlay` yet |
| `1.13.json` | the first version that uses the `arguments` block at all |
| `1.12.2.json` | the last release that spells its arguments as a string, with `--versionType` |
| `1.8.9.json` | the `userProperties` of the versions before 1.13 |
| `1.7.10.json` | the first release with `--assetIndex`: the assets change their layout here |
| `1.7.2.json` | the last version without `--assetIndex` and without `--session` |
| `1.6.4.json` | the `legacy` assets, `--session`, and **no** `javaVersion` at all |
| `1.5.2.json` | the `pre-1.6` assets and the positional arguments: the name and the session are bare, without a flag |
| `1.0.json` | the oldest release |
| `b1.7.3.json` | an `old_beta` version |
| `c0.30_01c.json` | an `old_alpha` version, with `--tweakClass` |
| `13w48b.json` | a plain version with `--accessToken` instead of `--session` |
