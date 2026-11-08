# Changelog

All notable changes to this project are in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.5] - 2026-11-09

### Added

- The "Update now" button in the footer and in Settings. It downloads the release exe from GitHub, checks its SHA-256 hash and size, swaps it in, and restarts Hexsight. If the new copy does not start, the old exe comes back. The button is off in champ select and in game. With "Check for new versions" off, Hexsight sends no request. A folder that Hexsight cannot write to shows the download page link instead.

### Note

- This version still needs a manual download. The button works from 0.1.5 on, for the next update.

## [0.1.4] - 2026-11-08

### Added

- The My games page shows your stats. Period chips (Today, 7 days, 30 days, All) set the range. A profile header shows your win rate, your streak, your average kills, deaths and assists, your KDA, your average game length, your last 10 results and a form line.
- Three tabs: Games, Champions and Augments. The Augments tab shows how often you picked each of your own augments and how many of those games you won, as counts only, with no percent.
- Champion highlights show your best and most played champions. Tap a champion or an augment to filter the list. All of it is computed on your PC from your own history, and nothing leaves it.

### Changed

- The Settings page is more compact. The notes move into hover tips. Advanced and About are folded. The card marks sit in one group. The build line and the tips line are merged. The keycap of a hotkey is the button that changes it. Move and reset share one row.

## [0.1.3] - 2026-11-08

### Added

- In champ select, a "Teammates" strip under the bench shows your teammates' champions with their tier badges. Click a teammate to ask for a trade, and click again to take the request back. A teammate who cannot trade shows dim.
- The "Next items" card follows the build path that you are on. After your first finished item, the header says "On your Malignance path" when Hexsight is sure of the path, and the next items come from it. A path that you leave is let go.
- A line under the item tips shows the next steps, for example "Then: Rabadon's Deathcap → Void Staff". It never lists more items than your free slots hold, and it is hidden when your slots are full.
- A rule for the fed enemy: when it deals mostly magic or mostly physical damage, the tips add a resist item with the reason "vs fed Syndra (9/1)". The summary line names its build, for example "Fed: Caitlyn 9/1/4 · 3 crit items".
- While you are dead, the overlay and the main window say who killed you: "Killed by Zed · 9/2/4 · 85% AD" or "Zed killed you 3 times". Hexsight names a killer only when exactly one enemy got a kill and no teammate died in the same moment. That enemy can raise a resist item with the reason "Zed killed you".
- The reasons of the rules name the enemies: "Amumu and Nautilus are tanky", "Soraka and Aatrox heal", "Vel'Koz, Lux: 70% AP". The overlay cells show their faces and a short word. When the names do not fit, the count form shows.
- While you are dead, a tip that your gold now pays for glows green, and its dot pulses twice.
- The tooltip of the summary card shows your own armor and magic resist.
- A tip that you cannot pay for yet names the part that your gold pays for now. The overlay shows it as a small icon.
- The overlay cells show the gold that is missing for an item, for example "-700".

### Changed

- The damage mix of the enemy team uses each enemy champion's damage split from the build data and the gold of its damage items, so it is right from the start and a support with few items counts less than a carry.
- The armor and magic resist tips look at your own armor and magic resist. With a lot of armor, they no longer ask for more.
- The resist and penetration bonus is half when you own one such finished item, and none with two.
- When the enemy team deals mostly magic or mostly physical damage and your build has no item with that resist, the tips add one resist item that fits your build.
- The item that cuts healing also comes for one healer when that healer is the fed enemy, and the reason then says "vs a fed healer". The rule stays only while that healer is still ahead.
- Before your first finished item, no rule lifts a tip above the first core item, and a rule tip takes the third place only when its points are close to the next item.
- Your AP or AD class and the tank build come from the core items of your build.
- The basic Boots no longer count as a pair of boots, so the boots upgrades stay in the tips.
- Hexsight reads the players' items each second while you are dead and for 10 seconds after a purchase, and a purchase of 150 gold now counts.
- The overlay cells say "fits an augment" for an item that pairs with an augment. The main window still shows the augment name.
- The footer of the main window shows the patch and the day of the data, for example "Patch 16.22 · Data: Oct 25" ("16.22 yaması · Veri: 25 Eki"), or "Today" and "Yesterday". The game count moves to the tooltip.
- Turkish: clearer words for healers ("iyileştiriyor"), crit items ("krit eşyası") and the killer cell ("öldürene karşı").

### Fixed

- Builds, item sets, later items and boots use the most played profile, route and items, not the ones with the best win rate. Rows with fewer than 300 games are left out. For Heimerdinger, the core is now Blackfire Torch, Liandry's Torment and Rylai's Crystal Scepter.
- The boots tip shows the most played pair of the build. Before, the cheaper pair could come first, for example Berserker's Greaves on Heimerdinger.
- The boots block says "most players skip them" only when half the players or more buy no boots.
- Doran's Blade, other parts with lifesteal, and Gluttonous Greaves no longer make an enemy a healer.
- A damage item with armor or magic resist, such as Zhonya's Hourglass, no longer makes an enemy count as tanky.
- A resist rule now always shows its item, also when the build has only a weak item of that resist.
- One tank or one healer reads as "vs 1 tank" or "vs 1 healer", not "vs 1 tanks".

## [0.1.2] - 2026-11-08

### Fixed

- Hexsight now finds an ARAM Mayhem game through the game itself when it cannot reach the League Client. Before, the window could stay at "League closed" for the whole game.
- When Hexsight cannot read the cards of a pick, it now says so and tells you to press the "Read the cards now" key. Before, it showed nothing.
- The note that card reading does not work now goes away when card reading works again.
- PRIVACY.md now says that the "Set to Borderless" button changes the window mode in `game.cfg`.

### Added

- `hexsight.log` gets one line each time the connection to the League Client changes. The line holds the port and the error, never the password.

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
