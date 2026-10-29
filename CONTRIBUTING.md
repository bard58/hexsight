# Contributing to Hexsight

Thank you for your help. This file tells you how to build the project, what a good change looks like, and which changes we cannot accept.

## Rules of the project

Hexsight stays inside the rules for tools that help during a game. We close a pull request that breaks one of these rules:

1. **No game memory access.** Do not read or write the memory of the game. Do not open the game process.
2. **No code injection.** No DLL injection and no hooks into the game.
3. **No input automation.** Do not press keys or click in the game for the player. A write call to the League Client must follow a click of the user.
4. **No pick advice.** Do not write "pick this" and do not rank the offered cards for the player. Show facts. The optional top-tier highlight is the limit. It stays a setting that is off by default. The same holds for the pairs hint.
5. **No augment win rates.** Not as a default, not in a setting, and not as a number derived from one (deltas against the average, pair gains, personal augment rates). Riot lists them as not approved for apps and overlays ([Riot developer docs](https://developer.riotgames.com/docs/lol), checked 2026-10-24).
6. **No new network host without a discussion.** Open an issue first. The data source must allow this use in writing.
7. **No Riot art or logos in the repository.** The only exception is the small test images that already sit in `tests\fixtures`.
8. **No ads, no payments, no telemetry.** The data licenses allow non-commercial use only.

## Build and test

You need the Rust stable toolchain (edition 2024) on Windows.

```powershell
cargo build --release
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

The CI runs the last three commands on every pull request. Run them before you push.

While you play League on the same PC, run Cargo at a low priority so the game does not lose frames. For example: `cmd /c start /low /wait cargo test`.

Read [docs/DEVELOPER.md](docs/DEVELOPER.md) for the architecture, the settings, the test hooks, and the mock League Client. To start the mock, first create its test certificate with `bash tools/mock/make_cert.sh`.

## Change a text or add a language

All interface texts are in `src/i18n.rs`. Each key has one line with the English text and the Turkish text. The tests check that each key has both texts and the same `{0}` places.

To review all texts in one file, run `node tools/texts.mjs export texts.tsv`. Edit the file. Then run `node tools/texts.mjs import texts.tsv --dry-run` to see the changes.

## Make a pull request

1. Open an issue first for a large change.
2. Make a branch. Keep one topic in one pull request.
3. Add or update a unit test for new behavior.
4. Run the four commands above.
5. Fill in the pull request template.

Write commit messages in the imperative, for example "Fix the trade button state". Write English in short, plain sentences.

## Report a bug

Use the bug report form. It tells you how to save the last scan in Settings. A saved scan can show other players' names. Check the image before you attach it. Do not attach `history.json` if you do not want to share your games.

## License of your contribution

When you contribute, you agree that your work has the same license as the project (see the README, section License). The user can choose MIT or Apache-2.0.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).
