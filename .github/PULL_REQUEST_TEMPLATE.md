## What does this change?

<!-- One or two sentences. Link the issue, for example: Fixes #123 -->

## How did you test it?

<!-- Unit tests, the mock League Client, or a real game? Say which. -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] I added or updated tests for new behavior
- [ ] I updated the texts in both languages (`src/i18n.rs`) if the change shows text
- [ ] I updated `CHANGELOG.md` under "Unreleased"

## Fair play

- [ ] This change does not read or write game memory, inject code, or send input to the game
- [ ] This change does not tell the player which augment to pick
- [ ] This change shows no augment win rate and no number derived from one
- [ ] This change keeps the top-tier highlight and the pairs hint off by default
- [ ] This change adds no new network host (or I opened an issue about it first)
- [ ] This change adds no Riot art or logo and no private data (player names, Riot IDs, game screenshots)
