# Changelog

All notable changes to this project are in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-10-31

### Fixed

- The item tips no longer suggest an item that you already upgraded in the game. For example, Muramana now also counts as Manamune and Tear of the Goddess. The same applies to Seraph's Embrace and Fimbulwinter.

## [0.1.0] - 2026-10-29

First public release.

### Added

- Card reader for the ARAM Mayhem augment picks. It reads the offered cards from the game window with the Windows built-in OCR.
- Overlay strip at each pick screen: the tier letter, the number of games and a three-bar confidence meter for each card. A card with few games says "limited".
- Pick card in the main window with the augment names and icons.
- Optional marks, all off by default: a green frame on the top-tier cards, a pairs hint, and a mark for augments that fit your build.
- Champion select: the three offered champions, the bench as an icon grid with tiers, a one-click swap to the best bench champion, trade requests to accept or decline, the summoner spell pair, and the builds.
- Item sets in the in-game shop, one for each build of your champion.
- In-game tips: the next items with the gold you still need, the damage mix of the enemy team, and the most fed enemy.
- My games: win rate, last 10 results, a list of recent games with champion, K/D/A, length and augments, and your champions with an "All champions" switch. Games stay in `history.json` on your PC.
- English and Turkish interface. Auto follows the League Client language.
- Hotkeys you can change: F8 shows or hides the overlay, Ctrl+F8 unlocks it to move, F9 reads the cards now.
- Portable zip: with the file `Hexsight.portable` next to the exe, all data stays in a `Hexsight-data` folder next to it.
- Update check: at most once a day, Hexsight asks GitHub for the newest release.

### Notes

- Hexsight shows no augment win rates. There is no setting and no hidden mode for them.
- Hexsight does not read game memory, inject code, or send input to the game. It acts only when you click.
- Releases are not code-signed yet. Windows SmartScreen can warn at the first start.
