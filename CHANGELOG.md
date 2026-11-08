# Changelog

All notable changes to this project are in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- The builds and the item sets now use the most played profile and route of a style, not the one with the best win rate. For Heimerdinger, the core is now Blackfire Torch, Liandry's Torment, and Rylai's Crystal Scepter, as half the players buy it. Before, it came from a profile with 4.5% of the games.
- The later items and the boots now go by pick rate, and a row with fewer than 300 games is left out. Before, a rare item with a high win rate could come first.
- The boots tip now shows the most played pair of the build. The second pair gets 2 points less. Before, both pairs got the same points and the cheaper pair came first. For example, Heimerdinger got Berserker's Greaves instead of Sorcerer's Shoes.
- The boots block now says "most players skip them" only when half the players or more buy no boots. Before, it said so when the "no boots" row had the best win rate.

### Changed

- The damage mix of the enemy team now uses the damage split of each enemy champion from the build data, so it is right from the start of the game. Before, a team of Akali, Katarina, and Evelynn read as physical damage.
- The armor and magic resist tips now look at your own armor and magic resist. With a lot of armor, the tips no longer ask for more armor.
- An enemy is no longer a healer because of Doran's Blade, another part with lifesteal, or Gluttonous Greaves. Only a finished item of lifesteal or omnivamp counts.
- The item that cuts healing now also comes for one healer when that healer is the fed enemy. The reason then says "vs a fed healer".
- The basic Boots no longer count as a pair of boots. The upgrades of the boots now stay in the item tips.
- The damage mix of the enemy team now counts each enemy by the gold of its damage items. A support or a tank with few items counts less than a carry with six items.
- The bonus for magic resist, armor, or penetration is now half when you own one such finished item, and none when you own two.
- When the enemy team deals mostly magic or mostly physical damage and your build has no item with that resist, the tips now add one resist item that fits your build.
- Your AP or AD class and the tank build now come from the core items of your build.
- A tip from a rule of Hexsight now takes the third place before your first finished item only when its points are close to the next item.
- A damage item with armor or magic resist, such as Zhonya's Hourglass, no longer makes an enemy count as tanky.

### Added

- In champ select, a "Teammates" strip under the bench shows the champions of your teammates with their tier badges. A click on a teammate asks for a champion trade, and a second click on a teammate that you asked takes the request back. A teammate who cannot trade now shows dim.
- The "Next items" card now follows the build path that you are on. After your first finished item, the header says "On your Malignance path", and the next items come from that path. When Hexsight is not sure of the path, the card shows the build style as before.
- A line under the item tips now shows the next steps after them, for example "Then: Rabadon's Deathcap → Void Staff". It is hidden when your slots are full.
- A new rule for the fed enemy. When the fed enemy deals mostly magic or mostly physical damage, the tips add a resist item with the reason "vs fed Syndra (9/1)". The summary line names the build of the fed enemy, for example "Fed: Caitlyn 9/1/4 · 3 crit items".
- While you are dead, the overlay and the main window say who killed you: "Killed by Zed · 9/2/4 · 85% AD", "Zed killed you 3 times", or "Killed by Zed and Lux". An enemy that killed you can raise a resist item, with the reason "Zed killed you".
- The reasons of the rules now name the enemies: "Amumu and Nautilus are tanky", "Soraka and Aatrox heal", "Vel'Koz, Lux: 70% AP". The overlay cells show the faces of those enemies and a short word. When the names do not fit, the count form shows as before.
- While you are dead, a tip that your gold now pays for glows green, and its dot pulses twice.
- The tooltip of the summary card shows your own armor and magic resist.
- An item tip that you cannot pay for yet now names the part that your gold pays for now. The main window shows it in a line under the item. The overlay shows it as a small icon.
- The overlay cells now show the gold that is missing for an item, for example "-700".
- Hexsight now reads the items of the players each second while you are dead and for 10 seconds after a purchase. A purchase of 150 gold now counts too. Before, the list was read every 4 seconds, and only a purchase of 250 gold or more counted.
- The overlay cells now say "fits an augment" for an item that pairs with an augment. Before, the long name of the augment was cut. The main window still shows the name.

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
