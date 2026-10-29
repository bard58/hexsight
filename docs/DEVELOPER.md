# Hexsight developer guide

A light ARAM Mayhem helper for League of Legends

Hexsight is a light helper for ARAM Mayhem. It is one Windows exe, written in Rust with egui. It has no Electron and no WebView. Hexsight reads the three offered augment cards and shows the tier and the games of each one.

This guide is for developers. Players: read the README in the repository root. This guide describes version 0.1.0.

## 1. What it does

- **Card reading in game.** Hexsight reads the three offered augment cards with Windows OCR (optical character recognition). The pick card lists the cards from left to right. Each row shows where its card is (Left, Middle, or Right), the number of games, how much data stands behind the numbers (three bars and a word: high, medium, or low), and the tier. The header shows your level. Hexsight does not tell you which card to take. Two optional marks are off by default (section 6, "Fair play"). "Highlight top-tier cards" adds a green frame, a green dot, the word "top", and a header line about the top tier. "Show pair hints" adds a pairs line on the second, third, and fourth pick screen: a card pairs well with a card that an earlier pick screen offered, "if picked earlier". Hexsight cannot know which card you took.
- **No augment win rates.** Hexsight shows no win rate for an augment. It also shows no number that comes from one: no gain over the champion average, no gain for a pair, and no win rate of your own with an augment. No setting turns them on, because Riot's developer policy does not allow them (section 6). The pick views show tiers and game counts. The shop item sets carry no win rate and no pick rate either.
- **One main window that follows the game.** The window shows what the phase of the game needs: a checklist before the first game, your champion with the bench and the build in champ select, the next pick level between pick screens, the pick card on a pick screen, and the result after the game. In a game of another mode, it says that Hexsight works in ARAM Mayhem only.
- **Overlay.** A small click-through pill "Hexsight watching" shows between pick screens. When the reader finds cards, the pill grows into a strip with one cell for each card. A cell shows where the card is, the games, and the confidence. With the optional marks on, a cell also shows the word "top" and a green frame for a top-tier card, and a pairs mark with "if taken". The strip never shows an augment name, an icon, or a tier letter, because the game prints them. You can choose if recordings and streams show the overlay (Settings > Panel, section 3).
- **Bench swap.** In ARAM champ select, the bench card shows the bench champions as a grid of icons, five in a row (two rows for a bench of 10), best tier first. A badge on each icon shows the tier, or a dash without stats. The best tier among the bench champions that beats yours gets a green glow, and a line under the grid names it and your champion ("Best swap: Lux (S) → your Garen (C)"). The tooltip of an icon shows the name, the games, and "Click to swap". A click on an icon or on its badge sends the swap to the League Client. Your own champion is not in the grid: the champion row above shows it with its tier. When the page is taller than the window (two trade requests, or a small screen), the page scrolls and the footer stays.
- **Champion trades.** When a teammate asks for your champion, a highlighted card shows both champions and the buttons Accept and Decline (section 3, "Champion trades").
- **Item tips and game summary.** In an ARAM Mayhem game, between pick screens, the main window shows two more cards. The card "Next items" lists up to three items with a reason for each, and a chip: a check mark when you have the gold, or the gold that is missing (for example "-420"). The tips come from the build data of your champion (the build style that fits the items you own, the core items in order, the boots, the later items, and the items that most players take as the next item) and from four rules of Hexsight: more magic resist against an enemy team with 60 % ability power or more, more armor against 60 % attack damage or more, an item that cuts healing against two or more healers, and penetration against two or more tanky enemies. A reason that comes from a rule says so, and its tooltip says "Hexsight's own rule, not statistics". The card updates after each purchase. Hexsight buys nothing, and the tips show no win rate or pick rate. The card "This game" shows the damage mix of the enemy team (a bar, and "70% AP · 30% AD"), the fed enemy with its score line, and the healers and the tanks as small champion icons. It is an estimate from the enemy champions and their items, and its tooltip says so. The overlay shows a card with the same tips (section 3, "The overlay"). Section 3 also tells what the Live Client does not give: the augment that you took.
- **Builds and item set.** The build card shows up to three builds of your champion with the first items. Hexsight writes the builds to the in-game shop as item sets, one set for each build. A set has up to seven blocks, and a block with no items is left out: the start (with the skill order in the title), the other start, the core build in order, boots (or "most players skip them"), later items, situational items, and consumables. Repeated items become one row with a count. The titles carry no win rate and no pick rate. The code is `src\itemset.rs`, and the data comes from `kit_champion` in `src\data.rs`.
- **Summoner spells.** In champ select, a small card shows the most played spell pair of your champion from the aramkit data (the key `summoners` of the champion file): two icons, a short label, and the names. The pair is the one with the most games, which is the one with the highest pick rate (the games of a pair are its pick rate times the games of the champion). The rank of the source does not decide: for Heimerdinger it puts Flash and Clarity first at 2.4 % of the picks, while Flash and Ghost has 54.7 %. The pair needs the minimum games. The card shows no win rate, and Hexsight selects nothing: you choose the spells in the game. Without spell data, or without the spell names, the card stays away.
- **Update note and feedback.** The group About has a switch "Check for new versions" (on by default). Once a day, Hexsight asks the GitHub releases API of the public repo (the constant `REPO` in `src/update.rs`, `OWNER/hexsight` until the export puts in the owner) for the latest tag. A newer tag shows as the link "Update available" in About and in the footer. It opens the download page. Hexsight downloads and installs nothing. The button "Send feedback" opens a new GitHub issue page with the Hexsight version, the Windows version and the main settings in the text. It holds no Riot ID, no path and no game data.
- **Your games.** The My games page saves each ARAM Mayhem game that you play. Hexsight keeps your champion, your augments in pick order, the result, and your kills, deaths, and assists. The page shows your win rate, your last 10 results, the list "Recent games" with the augments that you took in each game, and your five most played champions. The switch "All champions" lists every champion that you played. The games stay on your PC (section 6).
- **A check for card reading.** The folded group "Card reading" on the Settings page shows what the card reader does: the reading language, the game window, the window mode, the source of your champion, the last scan, and the pick screens. The reading language shows as a name, for example "English (United States)". A button saves the last scan as a PNG file and a text file, so you can show what the reader saw. The group also holds the beta setting "Show badges on augment cards" (section 3).
- **English and Turkish labels.** The setting Language switches the labels of the app between English and Turkish. Auto follows the language of the League Client (section 2, "Language").

The measurements for size, memory, and processor use are still open. Each line below marks one gap. The numbers come from builds before the v2 window, so measure again before you quote them.

Size: `hexsight.exe` is 5.90 MB (6,184,448 bytes, release build with the list of all augments and the trades, 2026-10-24). It is the full app. The release build with the item tips is 6,327,808 bytes (6.03 MB, 2026-10-26). The data cache in `%LOCALAPPDATA%\Hexsight\cache` was 5.5 MB after the first runs. The augment icons are about 1 MB of it.

Memory (measured in champ select against the mock client, with the release exe, 2026-10-24): 47.5 MB to 52.6 MB private memory in 3 runs. The working set is 85 MB to 90 MB 12 seconds after start. It is 4.3 MB to 4.8 MB after Hexsight gives idle pages back to Windows, 45 seconds after start.

Processor use (CPU): 0.02 % to 0.03 % of all cores when idle (three runs of 30 seconds, each 45 seconds after the start, with a saved history of 41 games). The version with the Language setting gave 0 %, 0.01 %, and 0.02 % in three runs of the same kind (mock client in the Lobby phase, history of 3 games). The first 30 seconds after the start use 0.22 % to 0.23 % (two runs), because Hexsight loads its data. The history thread and the badge window use no processor time while they wait: the thread sleeps until a job comes, and the window exists only while cards show. One card scan takes about 90 ms (debug build, 1536x712 crop). Hexsight scans only in a Mayhem game. It scans every 1.2 seconds while a pick screen is due and every 0.6 seconds while cards show (0.3 seconds while the title band is unchanged, because such a scan skips OCR, section 4). A heartbeat capture runs every 1.5 seconds while picks are left, and it is read only when a cheap check sees cards (section 4).

The version with the list of all augments and the trades gave 0.01 %, 0.01 %, and 0 % idle processor use in three runs of 30 seconds. Each run started 45 seconds after the start of the installed exe, against the mock client in the Lobby phase. The private memory was 37.3 MB to 37.9 MB. The new work adds no timer: the data thread loads the global tiers about every 3 hours, and the trade calls run only when you click a button.

After a scan in a game, Hexsight keeps the last capture in memory for the button "Save the last scan". It keeps one image at most: 4.4 MB at 1920x1080, 7.8 MB at 2560x1440, and 17.5 MB at 3840x2160. Each new scan replaces the image. Hexsight frees it 10 minutes after the game, and it never writes it to disk until you press the button.

Hexsight gives unused memory pages back to Windows two times: 20 seconds after the start, and 5 seconds after a game ends. This lowers the working set. The pages come back at the next scan, so read the private memory too. The splash art in the header uses 3.5 MB of texture memory. Hexsight frees it when the header shows another champion, and when the overlay opens.

## 2. Install and first run

### Requirements

- Windows. The round corners need Windows 11. The glass background needs Windows 11 22H2 (build 22621). Older Windows versions ignore them. On such a version, the glass switch on the Settings page is off and dim.
- League of Legends and the League Client on the same PC.
- Windows OCR for card reading. It is part of Windows and needs no extra download.
- An internet connection. Hexsight downloads its data from the sites in section 5.

### Steps

1. Get `hexsight.exe` from section 7. Then install it for the current user:

   ```powershell
   powershell -File tools\install.ps1 -Exe "C:\path\to\hexsight.exe"
   ```

   The script needs no admin rights. It copies the exe to `%LOCALAPPDATA%\Programs\Hexsight` and adds a Hexsight shortcut to the Start menu and to the Desktop. The install is optional, because you can start `hexsight.exe` from any folder. The script also removes an install of the earlier name of this app: its folder in `%LOCALAPPDATA%\Programs` and its two shortcuts.

   CAUTION: The script stops a running copy of the app that was installed under `%LOCALAPPDATA%\Programs` before it copies the new exe. A copy in another folder (for example `target\release`) keeps running.

2. Start Hexsight from the Start menu or the Desktop. The pill in the title bar shows "League closed" until the League Client runs.
3. Start the League Client. The pill changes to "Client".
4. In the League settings, set the window mode to Borderless. Section 4 explains why.
5. Join an ARAM Mayhem game.

Until Hexsight knows your champion, the main window shows the checklist "Ready for Mayhem". It has five live checks: League detected, the window mode (Borderless), card reading, the data, and the hotkeys. A line under the checks says "Start a Mayhem game" and names the overlay key. The first time, the card also asks "Launch with Windows (minimized)?".

If League runs in Fullscreen, an orange note in the overlay says so. The first-run card and the card reading rows of the Settings page then have a button "Set to Borderless" (the overlay note has it too, when the overlay takes clicks). Only a click starts it. Hexsight reads the game settings of the League Client, sets `General.WindowMode` to 2, sends them back, asks the client to save, and reads `game.cfg` again to show the result. It applies from the next game. While League is closed, the button says "Open League first".

The main window has one page that follows the phase of the game, and two more pages that you open on purpose: Settings (the gear icon in the title bar) and My games (a row on the main page). The back arrow in the title bar returns to the main page. The main page shows these screens:

| Phase | Screen |
|---|---|
| League closed, League open, Lobby | The checklist "Ready for Mayhem" |
| Champ select | Your champion with its tier, the bench and the trade requests, and the build card. A line "Added to shop" shows when the item sets are in the in-game shop. In the first step of a Mayhem champ select, while you have no champion, the card "Pick your champion" instead (see "The first step of a champ select" in section 3) |
| In game, between pick screens | Your champion, the cards "Next items" and "This game" (item tips, section 3), the card "Next pick" with the levels 3, 7, 11, and 15, and the build card |
| A pick screen (cards found) | Your champion, the pick card, and the build card |
| Post-game | The result: Victory or Defeat, your champion, the length, and the number of augments |
| In or after a game of another mode | The card "Not an ARAM: Mayhem game" |

When the game starts, the overlay opens, if the setting is on. Hexsight runs one copy at a time. A second start brings the first window to the front.

The pill in the title bar shows the state of the League Client: League closed, League open, Lobby, Champ select, In game, or Post-game. The title bar also has the gear and the close button. Hexsight finds the League folder from the running `LeagueClientUx.exe` process. If it finds no process, it reads `league_of_legends.live.product_settings.yaml` in `C:\ProgramData\Riot Games\Metadata\league_of_legends.live`. Then it tries `C:\Riot Games\League of Legends`.

The window height follows its content. It grows to 680 points at most, or less on a small monitor (the monitor height minus 80 points, and 320 points at least).

Hexsight treats these queue identifiers (IDs) as Mayhem: 2400, 2401, 2403, 2405, 2410, 2450, 3240, 3270, and 3280. In these queues, the champion row shows an ARAM MAYHEM tag. In a Mayhem champ select, Hexsight also writes the item sets by itself, one time for each champion. In other queues it writes nothing, and the bench card works in every ARAM queue. A game of another mode has no pick screens, so Hexsight saves no result for it.

### Language

Hexsight reads the client language from the League Client. It shows champion, augment, item, and spell names in that language. The card matcher uses the local names and the English names.

The setting Language sets the language of the labels of the app. It is in the group General on the Settings page. It has three values:

| Value | Labels |
|---|---|
| Auto | Turkish when the client language starts with `tr` (for example `tr_TR`). English for each other language |
| English | English |
| Türkçe | Turkish |

Auto is the default. Before the League Client answers, Auto uses the client language that Hexsight saw at the last start. Hexsight keeps it in `settings.json` as `last_locale`. At the first start, with no saved language, the labels are English until the client answers.

A change of the setting shows at once, and Hexsight saves it. The key `language` in `settings.json` holds `auto`, `en`, or `tr`. Any other word means `auto`.

The setting changes each label of the main window, the overlay, and the badges on the cards. It also changes the short messages at the bottom of the window and the errors in the footer. The block names of the item set in the in-game shop follow it too. The number format follows the language. Turkish writes "%67,2" in place of "67.2%", "+8,7" in place of "+8.7", and "1,2k" in place of "1.2k". Turkish capitals keep the dot, for example "ŞAMPİYON SEÇİMİ".

The setting does not change the names of champions, augments, items, and spells. They follow the client language (section 5). The names of the builds ("crit", "on hit") come from the data source and stay in English. The text file of a saved scan, `last-scan.txt`, is always in English, so a helper can read it.

This README names the English labels. The Turkish labels have the same meaning. These are the main ones:

| English | Turkish |
|---|---|
| Augments, Build, Bench, My games | Eklentiler, Dizilim, Yedekler, Maçlarım |
| Next pick, Level | Sonraki seçim, Seviye |
| Accept, Decline, Cancel | Kabul et, Reddet, İptal |
| League closed, League open, Lobby, Champ select, In game, Post-game | League kapalı, League açık, Lobi, Şampiyon seçimi, Oyunda, Maç sonu |
| Equal tiers, top | Kademeler eşit, en üst |
| Left, Middle, Right | Sol, Orta, Sağ |
| Best swap, Click to swap | En iyi takas, Takas için tıkla |
| Send the build to the in-game shop | Dizilimi oyun içi mağazaya gönder |

Turkish words are longer than English words. Tests measure the long texts in both languages (the title bar, the pill, the overlay cells, the buttons, and the checklist), so they fit their room.

### Settings

Open the Settings page with the gear icon in the title bar. The tables use the same groups as the page.

| General | Default | What it does |
|---|---|---|
| Language | Auto | Auto, English, or Türkçe. Sets the language of the labels (see "Language") |
| Launch with Windows (minimized) | Off | Starts Hexsight minimized with Windows |

| Overlay | Default | What it does |
|---|---|---|
| Show the overlay when a game starts | On | Opens the overlay in a Mayhem game and closes it after the game |
| Show or hide the overlay | F8 | The button Change sets another key (section 3) |
| Read the cards now | F9 | The button Change sets another key (section 3) |
| Hide the panel from recordings and screenshots | On | Hides the overlay from OBS, Discord and the Game Bar. Off shows it there, and the card reader then skips the panel text. It works from the next time the overlay opens |
| Highlight top-tier cards | Off | Adds the green frame, the word "top", and the top-tier line to the overlay strip, the pick card, and the badges. Off by default: Riot's rules for third-party apps (section 6) |
| Show pair hints | Off | Adds the pairs line to the pick card and the pairs mark to the overlay strip. Off by default, for the same reason |
| Mark augments that fit your build | Off | Adds the chip "fits" and the line "Fits your build: <item>" to a card of the pick card when the aramkit data ties the augment to an item that your build still wants. It shows no number and no frame, it does not rank the cards, and the overlay strip and the badges never show it. Off by default, for the same reason. One note under the three switches says why |

| Suggestions | Default | What it does |
|---|---|---|
| Suggest the next items | On | Shows the card "Next items" in the main window and the item cells of the overlay card (section 3). Hexsight reads the player list of the Live Client for it (section 6) |
| Show the game summary | On | Shows the card "This game" and the context line of the overlay card. It uses the same player list |

One note under both switches says that the tips come from build data and from rules of Hexsight, never from win rates, and that you decide.

| Build | Default | What it does |
|---|---|---|
| Send the build to the in-game shop | On | Writes the item sets once for each champion in a Mayhem champ select. Off deletes the sets of Hexsight from the shop |

| Card reading (folded) | What it shows |
|---|---|
| Reading language | The language of the Windows OCR engine as a name: "English (United States)" for en-US, "English (UK)" for en-GB, and "Turkish" for tr-TR. Another region of English or Turkish adds its tag, for example "English (en-AU)". Any other language shows its tag. The engine starts in a game, so the row shows "Shown in a game" before that. The text file of a saved scan has the same value |
| Game window | Found or Not found in a game. "Not in a game" outside a game |
| League window mode | Borderless, Windowed, or Fullscreen, from `game.cfg` |
| Champion source | Champ select or Live client: where Hexsight got your champion |
| Last scan | How long ago the last scan ran |
| Lines read | The text lines of the last scan. A note shows how many lines sat under the overlay. An orange note says that the OCR engine failed |
| Names matched | The augment names that matched in the last scan |
| Pick screens | The pick screens due at your level, and the pick screens done |
| Save the last scan | Button. Writes `last-scan.png` and `last-scan.txt` (section 4) |
| Unlock the overlay to move it | Shows the key that unlocks the overlay: Ctrl and the overlay key |
| Reset overlay position | Button. Puts the overlay at the default place |
| Show badges on augment cards (beta) | Off by default. Opens a window over the game that draws a badge above each offered card (section 3) |

Some settings have no switch on the page. Edit them in `settings.json` while Hexsight is closed:

| Key | Default | What it does |
|---|---|---|
| `overlay_scale` | 1 | The overlay size: 0 is small (zoom 0.9), 1 is medium (1.0), and 2 is large (1.2). It applies when the overlay opens. The main window keeps the zoom 1.0 |
| `overlay_in_captures` | false | False hides the overlay from screen captures. True shows it, and the card reader skips the text of the overlay (section 3). The switch "Hide the panel from recordings and screenshots" in the group Overlay sets it (the switch is on when this is false). It works from the next time the overlay opens |
| `min_games` | 300 | Rows with fewer games show dim, and a card needs this many games to count for the top tier |
| `scan_always` | false | Reads the cards all game long, not only when a pick is likely (more CPU) |
| `glass` | true | Shows the acrylic backdrop behind the window. It needs Windows 11 22H2 (build 22621) |
| `show_on_champ_select` | true | Restores a minimized window when champ select starts |

Hexsight has no setting for augment win rates. It shows none, and it shows no number that comes from one (section 6). An older `settings.json` can still hold the key `augment_rates`. Hexsight ignores it. The three optional marks have the keys `top_highlight`, `pair_hints`, and `fit_marks`. All are false by default, and each has its own switch in the group Overlay. The two suggestion switches have the keys `suggest_items` and `suggest_context`. Both are true by default. When the three marks and the two switches are all off, Hexsight does not read the player list. An older `settings.json` has none of the five keys, and it loads with these defaults. The old key `tiers_only` ("Hide win rates") has no switch and no effect any more, because Hexsight shows no augment win rate and the item sets carry no rate. Hexsight keeps the key so that an old file loads and saves as it was.

The group About names the data sources and carries the Riot disclaimer (section 6). The button "Open the data folder" opens `%LOCALAPPDATA%\Hexsight` (in portable mode, `Hexsight-data` next to the exe, see "Portable mode"). That folder holds `settings.json`, the `cache` folder, `history.json` (your saved games), and the files of a saved scan. The button "Download the data again" loads the data again. Hexsight also saves the window position in `settings.json`. The key `overlay_pos` holds the overlay place in points at zoom 1.0. A new overlay size then keeps the overlay on the same spot of the screen.

### Upgrade from the earlier name

This app had another name before it became Hexsight. At the first start, Hexsight takes over what the earlier name left on your PC:

- The data folder. When `%LOCALAPPDATA%\Hexsight` does not exist and the folder of the earlier name does, Hexsight renames that folder. Your settings, your saved games, and the cache come along. If the rename fails (the old app still runs), Hexsight copies `settings.json`, `history.json`, and the `cache` folder, and the old folder stays. The same copy runs when `%LOCALAPPDATA%\Hexsight` exists but holds no `settings.json` and no `history.json`. A file that Hexsight has already is never replaced. When `%LOCALAPPDATA%\Hexsight` has a `settings.json` or a `history.json`, Hexsight does nothing, and the folder of the earlier name stays as it is.
- "Launch with Windows". When the earlier name has a Run value in the registry, Hexsight deletes it and sets its own value `Hexsight`.
- The item sets in the shop. When Hexsight writes or deletes its item sets, it also deletes the files of the earlier name (`<name>.json` and `<name>-<number>.json`). It touches no other file.
- The install. `tools\install.ps1` deletes the install folder of the earlier name and its two shortcuts.

### Uninstall

1. Set "Launch with Windows" to Off on the Settings page. This deletes the registry value.
2. Close Hexsight.
3. Delete the folder `%LOCALAPPDATA%\Programs\Hexsight`.
4. Delete the two shortcuts with the name `Hexsight.lnk`. One is in the Start menu Programs folder and one is on the Desktop.
5. Delete the folder `%LOCALAPPDATA%\Hexsight`. It holds the settings, the cache, your saved games, the files of a saved scan, and the logs.
6. Delete the files `Hexsight-0.json`, `Hexsight-1.json`, and so on (and an old `Hexsight.json`) in the folder `Config\Global\Recommended` of your League folder. Or set "Send the build to the in-game shop" to Off before step 2: Hexsight then deletes its files itself.

### Portable mode

Portable mode is on when a file with the name `Hexsight.portable` is in the folder of the running exe. The content of the file does not matter. The release zip `hexsight-<tag>-windows-x64-portable.zip` holds `Hexsight.exe` and this file (its source is `assets\Hexsight.portable`, a short note in English and Turkish).

- In portable mode, the data folder is `Hexsight-data` next to the exe, not `%LOCALAPPDATA%\Hexsight`. It holds the same files: `settings.json`, `history.json`, the `cache` folder, the files of a saved scan, `crash.log`, and the data log `hexsight.log` (with `hexsight.log.1`).
- Hexsight checks once at the start that it can write there: it makes the folder, writes a small file with a new name, and deletes it. When that fails (for example under `C:\Program Files`, or on a read-only stick), Hexsight removes the folder again if it made it, and uses `%LOCALAPPDATA%\Hexsight`. The group About then says that the folder is not writable.
- Portable mode moves no data of the earlier name. The fallback to `%LOCALAPPDATA%\Hexsight` moves it as a normal start does. A portable copy (also after the fallback) does not take over the "Start with Windows" value of the earlier name. The test is `a_portable_copy_never_takes_over_the_legacy_run_value` in `src\win.rs`.
- The first-run card does not ask "Launch with Windows?" when the marker is there (also after the fallback). The switch on the Settings page stays. It writes the Run value with the path of the current exe.
- The group About shows one more line: "Portable mode: data is in the Hexsight-data folder next to the app." After the fallback, it shows the line about `%LOCALAPPDATA%\Hexsight`.

An exe in the cargo `target` folder has no marker, so the test hooks and the screenshot scripts work as before: they set `LOCALAPPDATA` to another folder. Do not put a `Hexsight.portable` file in `target\debug` or `target\release`.

The code is in `src\cache.rs`. The pure function `choose_data_dir` makes the decision from the exe folder, the marker, a write check, and `LOCALAPPDATA`. Its tests:

- `a_marker_and_a_writable_folder_put_the_data_next_to_the_exe`
- `a_marker_and_a_folder_that_is_not_writable_fall_back_to_localappdata`
- `no_marker_keeps_the_data_in_localappdata_and_checks_nothing`
- `no_localappdata_means_the_temp_folder_unless_the_portable_folder_works`
- `a_new_folder_is_writable_and_a_folder_under_a_file_is_not` and `an_old_probe_file_or_folder_does_not_block_the_check` (the write check `can_write`)
- `the_test_exe_in_the_target_folder_has_no_marker`
- `the_panic_hook_gets_the_chosen_folder_without_a_new_choice` (the panic hook reads the data folder with `chosen_app_dir`, which never chooses it again, so a panic during the choice cannot hang the app)

The tests in `src\app.rs` check the window: `a_portable_copy_never_asks_to_start_with_windows`, `the_about_group_says_where_the_portable_data_is`, and `the_portable_lines_fit_the_about_card_in_two_lines` (both texts in both languages, in the 246 points of a card).

## 3. Hotkeys

| Key | Action | Works |
|---|---|---|
| F8 | Show or hide the overlay | In any window, also when the game has the focus |
| Ctrl+F8 | Switch the overlay between movable and locked | Only while the overlay is on |
| F9 | Read the cards now | At any time, also outside a game |

These are the default keys. The Settings page changes the first and the last key. The unlock key is always Ctrl and the overlay key.

Hexsight saves the place of the overlay when you lock it. After F9, a short message shows the result, for example "Scanned 3 cards" or "No augment cards detected". 
Hexsight checks the state of the two hotkeys and the Ctrl key every 40 milliseconds in a game, and every 100 milliseconds at other times. It installs no keyboard hook and records no other key.

### Change a hotkey

1. Open the Settings page and find the group Overlay.
2. Press Change in the row of the hotkey. The row shows "Press a key".
3. Press the new key: F1 to F12, A to Z, or 0 to 9. Esc cancels.

The new key works at once, and Hexsight saves it in `settings.json`. The key thread reads the new key, so you do not restart Hexsight. While the row waits, the hotkeys do nothing, so the keys that you press cannot start an action. The wait ends after 15 seconds, and when you close the Settings page. Hexsight refuses the key of the other hotkey and shows a short message.

A letter or a digit also works. The hotkey then fires each time you press that key in a game, also while you type in the game chat. For another key (for example Insert), edit `overlay_key` and `scan_key` in `settings.json` while Hexsight is closed. The values are Win32 virtual-key codes in decimal. F8 is 119 and F9 is 120.

### The overlay

The overlay is a small window that stays on top of the game and lets mouse clicks pass through. It has no glass backdrop. It opens when a Mayhem game starts, if the setting is on. Press F8 to show or hide it. The window fits its content, so it covers as little of the game as it can.

The overlay has three forms:

- **The pill.** Between pick screens, the overlay is a pill with a green dot, the name "Hexsight", and the word "watching". If the reader cannot work, a hint shows under the pill: a black or frozen capture, or League in Fullscreen (section 4).
- **The strip.** When the reader finds cards, the pill grows into a strip, 340 points wide, with one cell for each card in screen order. The header has the Hexsight mark and your level. Each cell shows, from top to bottom: where the card is (LEFT, MIDDLE, or RIGHT), the number of games, and the confidence (three bars and a word). A card without stats shows "No data". With "Highlight top-tier cards" on, the header also has a line that says how many cards are in the top tier ("1 card in the top tier", "3 cards in the top tier") or "Equal tiers", and a top-tier cell has the word "top" and a green frame. With "Show pair hints" on, a cell with a pair has a pairs mark with the words "if taken". The strip stays 1.5 seconds after the cards go away.

- **The advice card.** Between pick screens, the pill can grow into a card of the same width (340 points, 144 points high at most) with the build style in the header ("AP build"), a context line ("70% AP · Fed: Zed 9/2/4 · 2 healers"), and up to three item cells. A cell shows the icon, the name, the reason, and a green dot on the icon when you have the gold. The card shows only while you are dead (you can shop), for 12 seconds after the list of items or a ready flag changed (a purchase, a new item, enough gold), and while the overlay is unlocked. The pick strip always wins over it. At other times the pill shows. The overlay lets the mouse pass in a game, so the card has no tooltip. A line under the cells says "Hexsight's own rule, not statistics." whenever the card shows an estimate (a reason from a rule, or the context line).

The strip never shows an augment name, an icon, or a tier letter. The game prints them on its own cards, and Hexsight adds only what the game does not show. It does not tell you which card to take.

The setting file key `overlay_scale` scales the whole overlay: 0 is zoom 0.9, 1 is 1.0, and 2 is 1.2. Hexsight applies the zoom only in the overlay. The main window keeps the zoom 1.0. When you change the size, the overlay stays on the same spot of the screen. Hexsight saves the place in `settings.json` as the key `overlay_pos`, in points at zoom 1.0.

By default, Hexsight hides the overlay from screen captures, so OBS, Discord, and the Windows Game Bar do not show it. This also keeps the card reader from reading its own text. The switch "Hide the panel from recordings and screenshots" (Settings > Panel, file key `overlay_in_captures`) turns this off. The overlay then shows in captures. The card reader drops each text line whose center lies inside the overlay window, so it still does not read the overlay. The overlay covers the game pixels under it, so keep it away from the cards.

To move the overlay, press Ctrl+F8, drag the overlay, and press Ctrl+F8 again to lock it. While the overlay is unlocked, it carries the hint "Drag to move". The button "Reset overlay position" in the group "Card reading" puts it at the default place. Outside a game the overlay takes clicks, and a click on the pill closes it.

### The pick card

The pick card shows in the main window when the reader finds cards. It lists the offered cards from left to right, in the order of the screen. The header has your level (or "Augment selection"). With "Highlight top-tier cards" on, it also has the line about the top tier. Each row has these items:

- With "Highlight top-tier cards" on: a green dot before the name on a top-tier card, and a green frame around the row.
- The name of the augment.
- A caption under the name: where the card is (Left, Middle, or Right), a dot, and the number of games. A card without stats says "No data". A title in the left 40 % of the read area is Left. A title in the right 40 % is Right. Other titles are Middle.
- The confidence at the right: three bars and a word (high, medium, or low). It grows with the games behind the numbers: 3,000 games or more is medium, and 8,000 or more is high. These limits are placeholders until they are tuned on real data. The games are the ones the row shows: the stats of this pick screen when the source has them, else the stats of all picks. The first is a part of the second, so the same augment can show fewer bars on a later screen.
- The tier badge at the far right. A dim badge means that the sample has fewer games than the minimum. A card without stats gets a dim ? badge.
- With "Show pair hints" on: a pairs line under the row, only on the second, third, and fourth pick screen: "Pairs well with <name> if picked earlier". It shows when the card forms a top pair with a card that an earlier pick screen of this game offered. The top pairs are the first 8 pairs of the champion data that have the minimum games. The line has no number. When the name of the earlier card is not known, the line says "an earlier pick".

With "Highlight top-tier cards" on, Hexsight highlights every top-tier card. These rules apply:

- A card is eligible when it has stats and at least the minimum games. The stats are the stats of this pick screen, or else the overall stats.
- The top tier is the best tier among the eligible cards. Hexsight highlights every eligible card of that tier, but only when another eligible card has a worse tier.
- When two or more cards are eligible and all have the same tier, Hexsight highlights no card. The header then says "Equal tiers".
- The pick card has no PICK tag and no ranking. The badges on the cards use the same rules, so both mark the same cards.

Hexsight cannot know which card you took. It sees only the offers of each pick screen. It remembers all the cards that each pick screen offered (a new read of the same screen replaces its entry), and the pairs line says "if picked earlier". The overlay mark says "if taken". The pairs line is a caption. It never changes the highlight. Hexsight clears the list of earlier cards when a game ends. Hexsight never shows an augment as taken.

If your champion has no stats, the cards show in screen order with no highlight. With "Highlight top-tier cards" on, the header says "No data".

Between pick screens, the main window shows the card "Next pick": the levels 3, 7, 11, and 15 as a timeline. The next level has a green dot, and the levels that are past are grey. The card says nothing about what you took. After the fourth level, it says "No more pick screens".

The pick card fades in over 150 ms. It does not move, so the height of the window stays the same. The card shows no augment win rate and no gain over an average, with any setting.

### Item tips

The tips are for ARAM Mayhem games only. They start when the Live Client Data API answers. The item tips also need the stats of your champion. The summary needs only the player list: when the stats of your champion are missing (the load failed, or the champion is unknown), the item card says "No build data for this champion, so no item tips." and the summary still shows. While the stats are still loading, the card says that the tips come when the game data arrives. The code is `src\suggest.rs`. It is a pure module: it takes plain data and gives plain data back, and it draws nothing and reads no network. The poll thread calls it once for each pass (about once a second) and stores the result only when it changed, so the window draws again only for a change.

**What Hexsight reads for it.** The Live Client gives your gold (`activeplayer.currentGold`) once for each pass. The list `playerlist` gives, for each of the ten players, the champion, the side, the items (the six item slots, without potions and the trinket), the score line (kills, deaths, assists), the level, and the death state. Hexsight reads `playerlist` every 4 seconds, and at once when your gold falls by 250 or more between two passes (a purchase). It reads it only in a Mayhem game and only while a suggestion setting is on. Hexsight keeps no player name: the list holds the champion, the side, the items and the score line, and the name is used only to find your row. Nothing goes to disk or to the internet. The files of CommunityDragon give the price, the recipe, and the tags of each item (`items.json`, the same file as before) and the roles of each champion (`champion-summary.json`). The aramkit file of your champion gives what it gave before, and also the items that players take as their 1st to 6th item (`items.filtered.slots`), the items tied to augments (`items.unfiltered.all[].augmentSpecific`), and the damage split of your champion (`champion.stats`). No new file, host, thread, or crate was added.

**How Hexsight picks the items.** Each item gets points, and the best three show.

1. The build style is the build of your champion with the most items in common with the items you own. A tie goes to the first build.
2. The candidates and their first points: the core items in order (100, then 8 less for each), boots (90 after your first finished item, 55 before it, and none when the build skips boots), the later items (62, then 4 less for each), the items that most players take as the next item (58, then 3 less), the items of the other builds (30, then 2 less), and an item that the data ties to an augment (60 for a known augment, 30 for an offered one).
3. Dropped: an item you own, a part of an item you own, a second pair of boots, an item that the shop does not sell, and, when your six slots are full, an item that has no part among your items.
4. You get 8 points for an item when you own a part of it, so the plan does not jump after a purchase.
5. The four rules add points: magic resist (28 for a tank, utility, or bruiser build, 16 for the others), armor (the same), an item that cuts healing (30; with two healers and no such item in the list, Hexsight adds Morellonomicon, Oblivion Orb, Mortal Reminder, Chempunk Chainsword, Executioner's Calling, Thornmail, or Bramble Vest, by your class), and armor or magic penetration (20; by your damage split). An item with more than six tags is never moved by a rule.
6. An offered augment that the data ties to an item adds 14 points. A known augment adds 35. The tie is the share of the players with the augment who buy the item (half of them or more). The aramkit file also has a win rate change for each pair (`synergyDelta`). Hexsight does not read it, so no item tip and no "fits" mark depends on the win rate of an augment.
7. Nothing under 40 points shows. At most one pair of boots shows. The first two places follow the points. The third place goes to the best item that a rule raised (60 points at least), else to the next item by points.

All the numbers are first values, in one block at the top of `src\suggest.rs`, and they are placeholders to tune on real games (as the limits of the confidence). The tests pin the behavior, not the numbers, where it is possible. The rules and the lists (the healers, the anti-heal items) are Hexsight's own, not statistics, and the card says so in a tooltip. The chip of an item says if you have the gold. The gold is not part of the points.

**The summary "This game".** The damage mix of the enemy team starts from the first role of each enemy champion (a mage is all ability power, a marksman all attack damage, and so on, worth 1500 gold of items). As the enemy buys items, their gold takes over. A fed enemy counts 1.5 times. An enemy is fed when its kills minus its deaths are 5 or more and it has 6 kills or more. The mix needs three known enemy champions and goes in steps of 5. A healer is an enemy champion from a short list of names, or any enemy with an item of lifesteal or omnivamp. A tanky enemy has two armor or magic resist items (boots do not count), or the role tank and one such item.

**Limits.** The fields were checked once, against the Live Client of a real Mayhem game: `playerlist` gave the champion, `team` (`ORDER` or `CHAOS`), `isDead`, `level`, `scores`, and the items of all ten players, and `activeplayer.currentGold` was a decimal number. Three things in that list differ from a guess: the Refillable Potion has no `consumable` flag (it stays among the items, and it holds a slot in the game), the item slots can skip a number, and the trinket is in the slot 6. The test `the_player_list_of_a_real_mayhem_game_parses` in `src\suggest.rs` uses an edited copy of that list, with other names and other champions (`tests\fixtures\playerlist-real.json`). Nobody watched a whole game with the tips yet, so the numbers for the fed enemy and the healers are still first values. If the Live Client gives no items for the enemy, or no side, the mix falls back to the roles, and the rules that need the missing part stay off. The Live Client Data API has no field for the augment that you took (the documented fields have none). So Hexsight infers nothing: it knows no augment that you took, and an offered augment only adds a few points to an item. The reason then says "pairs with <augment>", and the tooltip says that Hexsight cannot know if you took it. The chip "-420" counts the gold at the moment of the last draw of the window, and the window draws again only for a change, so the number can be old until the next draw.

### My games

The My games page shows your results in ARAM Mayhem. Open it with the row "My games" on the main page. It has three groups:

- **Overall.** Your win rate, your games, wins, and losses, and your last 10 results. A green dot is a win and a red dot is a loss. The newest result is at the right.
- **Recent games.** Your newest 10 games, one row each, the newest first: the champion, the result, your kills, deaths, and assists, the date, the length of the game, and the augments that you took, in pick order. The button "Show all" lists up to 100 games.
- **Your champions.** The five champions that you played most. Each row shows the tier of the champion, your games, your wins, and your win rate.

Without saved games, the page says "Play an ARAM: Mayhem game. Hexsight saves your augment picks here." After a game, the main window shows the result card, and a short message says "Game saved to My games".

How Hexsight saves a game:

1. In a game of a Mayhem queue, the poll thread reads the game ID from the gameflow session of the League Client.
2. When the game ends, the history thread asks the match history of the League Client for that game. The game can show up late, so Hexsight tries 5 times: 4, 14, 34, 64, and 124 seconds after the end of the game.
3. Hexsight finds you in the game by your player ID (`puuid`). It adds one record to `history.json`: the game ID, the date (UTC), the queue, your champion, your augments in pick order, the result, and your kills, deaths, and assists. A remake (a game under 5 minutes, or an early surrender in the first 10 minutes) is not a game and Hexsight does not save it. Hexsight saves Mayhem games only.
4. Hexsight keeps the 500 newest games. It writes the file through a temp file and renames it, so a cut write never breaks the file. A file that does not parse moves to `history.bad.json`.

At the first start, when `history.json` does not exist, Hexsight reads the newest 20 games from the match list of the League Client once. It saves the Mayhem games of them, so the page is not empty. If the client is not signed in yet, Hexsight tries again every 30 seconds, up to 10 times. A start with no Mayhem game in the list still writes an empty file, so the import runs only once. Delete `history.json` to import again. Hexsight does not save the games that you play while it is closed.

The fields come from the match history that the MIT project Yhprum/mayhem-tracker reads (`src/main/lcu.ts` and `src/main/db/payloads.ts`). Hexsight uses no code of that project. Section 5 has the field names.

### The first step of a champ select

A new Mayhem champ select can start with a step where the client offers a few champions (three) and you have none yet. The session of the League Client then names champion 0 for you. Hexsight handles this in two ways:

- **It forgets the champion of the game before.** At the start of a champ select, and while the session names champion 0, Hexsight has no champion of yours. The champion row says "No champion yet", and the page shows the card "Pick your champion" in place of the bench, the spells, and the build. Hexsight writes no item set while it has no champion, so item sets exist only for the champion that you picked in this champ select. The code is `keep_picked` and `next_champion` in `src\worker.rs`.
- **It shows the offered champions.** While you have no champion in a Mayhem queue, Hexsight asks `/lol-lobby-team-builder/champ-select/v1/subset-champion-list` once for each pass (every 0.7 seconds). In another queue it sends no such request (`wants_choices` in `src\worker.rs`), and it stops as soon as you have a champion. The answer is a JSON array of champion ids. The card shows each champion as an icon with its tier badge (a dash without stats) and its games, from the stats index, in the order of the client. The name shows when you hover the icon. It shows no win rate, it ranks nothing, and it has no button: you choose in the League Client. An empty list, a missing route (an older client), an error, or a list of more than 6 ids (a pool, not an offer) gives the line "Choose your champion in League." in place of the icons. When you pick, the session names your champion, the card goes away, and the normal champ select page shows.

Limits: nobody tested the route against a real League Client. The shape (an array of ids) comes from the type `subsetChampionIds: number[]` of the inventory draft in the community type files (dysolix/hasagi-types). The parser (`parse_subset` in `src\lcu.rs`) also takes a list of objects with `championId` or `id`, and an object with `subsetChampionIds` or `championIds`. The tests are `the_offered_champions_are_asked_for_only_in_the_first_step_of_a_mayhem_champ_select` in `src\worker.rs`, `the_subset_list_is_an_array_of_champion_ids` and `an_empty_or_strange_subset_answer_gives_no_choices` in `src\lcu.rs`, and `the_first_step_shows_the_offered_champions_with_tier_and_games_and_no_rate` in `src\app.rs`.

### Champion trades

In a champ select, the League Client lists the trades that you can make with each teammate in the field `trades` of the session. Each trade has an ID, the cell of the teammate, and a state. The champ select screen uses the state:

| State | What the champ select screen shows |
|---|---|
| RECEIVED | A highlighted card above the bench: "A teammate wants your champion". It shows your champion and the champion of the teammate with two arrows between, and the line "You get <name>; they get <name>". The buttons Accept and Decline are at the right. The card has an orange glow |
| AVAILABLE, SENT, BUSY, INVALID | Nothing. The window has no rows for the teammates, so you cannot ask for a trade or cancel one in Hexsight. The test hook `HEXSIGHT_TRADE` (section 8) can still send the calls ask and cancel |

After each click, a short message shows the result: "Trade request sent", "Trade accepted", "Trade declined", "Trade request cancelled", or "Trade failed" with the reason. The buttons of that trade stay dim for 1.5 seconds, so a second click cannot send the same call again. A message that comes while another one shows waits for its turn, so the result of an accepted trade stays on the screen before the message about the item set of your new champion.

Hexsight sends each call as a POST request. It tries the route `/lol-champ-select/v1/session/trades/<id>/<request|accept|decline|cancel>` first. If that route fails, it tries `/lol-champ-select/v1/session/champion-swaps/<id>/...`, the route of some client versions. If both fail, the message shows the error of the first route. Hexsight never sends a trade call by itself. Each call follows a click of yours.

Limits: nobody tested the trades against a real League Client. The mock client (section 8) models the states and the routes. The card shows under your champion while the request stands.

### Show badges on augment cards (beta)

This feature is off by default. It sits in the folded group "Card reading" on the Settings page, as the switch "Show badges on augment cards (beta)". The v2 design leaves the tier to the game, which prints it on its own cards, so the badges are an optional extra. While the pick screen shows in a Mayhem game, Hexsight opens a second window that covers the game. The window draws a pill above the title of each card: the tier badge and the number of games. With "Highlight top-tier cards" on, every top-tier card also gets a green edge and a soft glow (the rules are in "The pick card"). A card without stats gets a light grey ? badge. Every pill sits on an opaque dark plate with a thin light outline, and the tier colors have full strength. A sample with fewer games than the minimum gets a dashed outline, not a faded badge. The pills never show a win rate.

- The window is transparent and click-through. It is always on top and has no frame and no taskbar button. It never takes the focus.
- Hexsight hides the window from screen captures (`SetWindowDisplayAffinity`), so the card reader never reads it. The window is also not in recordings and streams, whatever the setting "Hide the panel from recordings and screenshots" says.
- The window exists only while the cards show. It closes when the offer ends, when the game ends, and when you turn the setting off. It also opens when the main window is minimized.
- Hexsight places each pill from the position that the card reader found for the title of the card. The reader gives the position as fractions of the capture area. Hexsight converts them to screen pixels with the position and size of the capture area at the time of the scan. It then converts the pixels to points with the scale of the monitor. The size of the pills follows the height of the game window, so a pill has the same share of the card on each screen.
- The pill sits just above the first line of the title. When "Highlight top-tier cards" is on, the green edge uses the same rule as the pick card, so both mark the same cards.

Limits: nobody tested the badges in a real game yet. The cards of the real game can differ from the synthetic pick screen of the tests, so a pill can sit over a part of the card icon. The window covers the whole game while the cards show. Windows can then draw the game in the composed mode, which can cost a little frame rate for these seconds. While the window exists, the private memory of Hexsight grows by about 12 MB (release build, game area of 1920x1080, with the mock client). It returns to the old value when the window closes. Borderless window mode is necessary, as for card reading.

## 4. How card reading works and its limits

No documented Riot API (application programming interface) lists the offered augment cards. Hexsight reads them from the screen. Open source helpers that detect the cards use screen OCR too.

### When Hexsight reads

Hexsight reads the screen only in a game of a Mayhem queue. It also needs your level from the Live Client Data API. Three scan rates apply:

- Every 0.6 seconds while cards show, and every 0.3 seconds while the title band has not changed since the last scan. Before OCR, Hexsight makes a short signature of the title band (a 128x24 grid of averages, 0.21 ms in a release build). When it equals the signature of the last read, the pick screen is the same, so Hexsight reuses the last reading and skips OCR (about 90 ms). A reroll or a pick changes the band and is read at once. A reading is reused for 10 seconds at most, and the scan hotkey always reads. Only a reading of two or more cards from the band alone is reused. A scan that skips OCR costs about 5 ms, so 0.3 seconds costs less than the old 0.6 seconds (about 2.5 % of one core against 13.7 %, with a 5 ms capture and a 90 ms read). A band that changes all the time (an animation) keeps the pause of 0.6 seconds.
- On a locked screen, a secure prompt (UAC), or a disconnected remote desktop, Hexsight asks Windows with `OpenInputDesktop` before each scan. It then does not capture, and the group "Card reading" and the overlay say "The screen is locked". In a connected remote desktop session, a black image gets its own hint ("Remote desktop shows a black game image") and not the Borderless hint. The code is `desk_state` and `capture_state` in `src\ocr.rs`.
- Every 1.2 seconds while a pick screen is due. The four pick screens are due at level 3, 7, 11, and 15. The setting "Read the cards all game" keeps this rate all game.
- Every 1.5 seconds, as a heartbeat, while Hexsight counts fewer than four pick screens. A pick screen at an unexpected level (for example one before level 3) is then still seen. A heartbeat capture first goes through a cheap check (114 pixels, a few microseconds) that looks for the three cards. Hexsight runs OCR on it only when the check sees at least two cards. A full OCR read still runs at least every 30 seconds, so a screen layout that the check does not know is still seen. A pick that is due, a card strip that shows, the setting "Read the cards all game", and F9 always read.

If a pick screen is due and no cards show for 45 seconds, Hexsight stops waiting for it. It counts the screen as missed. You can still open those cards later: for the rest of that game, the number of a new screen is never higher than the number of screens due at your level, so the late cards get the number of the screen that Hexsight gave up on, and they do not count twice. A screen keeps its number while its cards show, also when your level goes up. Cards that come back after a reroll or a close keep the number of their screen. The code is `offer_screen` and `count_closed` in `src\worker.rs`. Tests: `cards_opened_late_after_the_give_up_are_the_screen_that_was_given_up`, `cards_that_come_back_keep_their_screen`, `a_screen_keeps_its_number_while_the_level_goes_up`, `cards_of_level_7_opened_at_level_11_cost_one_screen`, `extra_screens_count_as_before_until_a_give_up`, `a_late_start_counts_the_screens_before_it`, and `offer_screen_edges`. Press F9 to read at any time, also outside a game. After two reads that find fewer than two cards, Hexsight hides the cards.

Hexsight assumes that the four pick screens come at level 3, 7, 11, and 15. The pick card and the worker use the same list, so they agree on the next level. Nobody confirmed the levels in a real game. The heartbeat finds a screen at another level, but the screen number can then be wrong.

### What happens in one read

1. Hexsight gets your champion from the League Client API in champ select. In game, it gets your champion, level, game time, and death state from the Live Client Data API. For the item tips (section 3), it also gets your gold and the list of the ten players.
2. Hexsight copies one part of the screen into memory. The part covers 10 to 90 percent of the game window width. It covers 12 to 78 percent of the height. If Hexsight cannot find the game window, it uses the primary screen.
3. The Windows OCR engine returns the text lines in the image. Hexsight does not save the image on its own (see "The saved scan" below).
   - The engine has a size limit (`OcrEngine.MaxImageDimension`). It is 10000 pixels on the test PC. If the engine does not say, Hexsight uses 2600.
   - If a side of the image is longer than the limit, Hexsight halves the image. It halves it again until the image fits.
   - Each new pixel is the average of a 2x2 block. This applies to 4K and ultrawide screens on a Windows build with the smaller limit.
   - When the overlay shows in recordings, Hexsight then drops each text line whose center lies inside the overlay window. It converts the x and y fractions of a line to screen pixels with the position of the capture area.
4. Hexsight normalizes each line. It makes the letters lower case, removes accents, and maps Turkish letters to Latin letters. It also merges letters that the English engine confuses. The map is `l`, `1`, `j`, `!`, and `|` to `i`, then `m` to `n`, `q` to `c`, `0` to `o`, and `5` to `s`.
5. Hexsight compares each line, and each pair of wrapped lines, with the augment names. It uses the names in your client language and in English. Similarity is 1 minus the edit distance, divided by the length of the longer text.
6. A name matches when its similarity reaches a limit. The limit depends on the number of characters in the name. Spaces and punctuation do not count.
   - 6 characters or fewer: 0.90.
   - 7 to 14 characters: 0.78.
   - 15 characters or more: 0.72.
7. Hexsight keeps one augment for each card column, from left to right, and three at most. Each card keeps the x center, the y center, and the height of its title line. The pick card shows the x center as Left, Middle, or Right, and the badges use all three. A title on two lines gives the place of its first line. Hexsight shows the cards only when it reads two or more.
8. Hexsight keeps the cards in screen order, from left to right. It does not rank them.
9. Hexsight uses the stats of the current pick screen when the source has them. The sample must have the minimum games. Otherwise it uses the overall stats of the champion.
10. Hexsight highlights every eligible card of the best tier (see "The pick card"). The pick card, the overlay, and the badges show the result.

Hexsight counts each new set of cards as one pick screen, from 1 to 4 in each game. The number picks the stats of that pick screen.

### The saved scan

After each scan in a game, Hexsight keeps the capture and the result in memory. It keeps one scan: the next scan replaces it, and Hexsight frees it 10 minutes after the game ends. The button "Save the last scan" on the Settings page writes two files in `%LOCALAPPDATA%\Hexsight`:

- `last-scan.png`: the capture area, as the reader saw it. It can hold a part of your game screen.
- `last-scan.txt`: a UTF-8 text file with a byte order mark, so Turkish names read right in Notepad. It holds the rows of the group Card reading and the position of the capture area. It also lists each text line with its x and y (fractions of the capture area, from 0 to 1) and the matched augments with the x and y of each card. The labels in this file are always English, whatever the setting Language says.

Hexsight writes these files only when you press the button. If you ask for help, send the two files. The PNG files of real pick screens also give test images for `src\ocr.rs`.

When the main window shows in a game, the capture area can include Hexsight itself. Hexsight then keeps the older scan, so the saved image shows the game and not Hexsight. The overlay is not in the capture (unless you turn on "Show the overlay in recordings and streams"), so the older scan is the one from the overlay time. A tip: press F9 while the overlay shows, then press F8 and open the Settings page.

### Limits

- **Borderless window mode is necessary.** Hexsight copies pixels from the screen, so the game window must be visible on the screen. Set the game window mode to Borderless. Hexsight does not support other window modes.
- **The Fullscreen note.** Each time the client phase changes, Hexsight reads the key `WindowMode` in the `[General]` section of `Config\game.cfg` in the League folder. The value 2 means Borderless, 1 means Windowed, and 0 means Fullscreen. For Fullscreen, the overlay shows an orange note: "League runs in Fullscreen. Card reading needs Borderless (Settings > Video > Window Mode)." If the file or the key does not exist, Hexsight shows no note.
- **OCR language.** Hexsight asks Windows for an OCR engine for your profile languages. If it finds none, it uses en-US. If Windows has no OCR engine, card reading is off, and the checklist and the Settings page say what to do: add English (United States) in Settings > Time & language > Language & region.
- **Turkish text on an English engine.** The test PC has only the en-US OCR language. The English engine reads Turkish titles with some errors. The matcher folds Turkish letters, merges look-alike letters, and uses a fuzzy match. The unit tests in `src\ocr.rs` include en-US OCR output for Turkish titles.
- **Two cards at least.** Hexsight shows cards only when it reads two of the three titles. If it reads one title or none, it shows nothing.
- **Titles only.** Hexsight compares text lines with augment names. It does not compare icons.
- **The pick screen number can be wrong.** Hexsight counts the pick screens that it reads and the screens that it gave up on after 45 seconds. A pick screen at another level than 3, 7, 11, or 15 can make the number wrong after a screen that Hexsight gave up on: the extra screen then shares the number of the last screen that is due by level. In a game without a give-up, Hexsight counts an extra screen correctly. The tiers then come from the wrong stage list. Hexsight also assumes that the four stage lists of aramkit match the four pick screens. Nobody confirmed this.

### Test status

Test result: 9 of 9 card names matched on synthetic 1920x1080 pick screens and on synthetic cards of 720p size. The synthetic cards use Windows fonts, not the game font. A real in-game test is still open.

Test result for the badges: a PC with the mock client and a synthetic 1920x1080 pick screen. The card reader found the three cards. The badge window opened on the game area (1920x1080 pixels) and drew a pill above each title. It hid from captures (display affinity 0x11), it let mouse clicks pass, it never took the focus, and it closed when the game ended. It also opened with the main window minimized. A real in-game test is still open.

Test result for My games: the mock client returned a match list and whole games in the shape of the League Client. The first import saved 3 Mayhem games and skipped the game of another queue. After a game ended, the mock answered "not found" two times, and the third try saved the game. A real game is still open: nobody checked the fields against a live client.

We found no augment field in the documented Live Client Data API. A live check during a Mayhem game is still open.

## 5. Data sources and credits

### Sources and credits

- **Stats, builds, and augment pairs:** aramkit.com (host `data.aramkit.com`). This is the main source.
- **Fallback stats:** aramhex-data by czgreat (`https://github.com/czgreat/aramhex-data`, host `raw.githubusercontent.com`), licensed CC BY-NC 4.0. The license allows non-commercial use only and needs credit.
- **Names, icons, and the Mayhem augment list:** CommunityDragon (host `raw.communitydragon.org`), in your client language. The item list also gives the price, the recipe and the tags of each item, and the champion list gives the roles of each champion, for the item tips. They come from the files that Hexsight loads already.
- **Patch number:** the version list of aramkit. Hexsight has no call to Data Dragon (`ddragon.leagueoflegends.com`) and shows no splash art.
- **Match history fields:** the MIT project Yhprum/mayhem-tracker (`https://github.com/Yhprum/mayhem-tracker`). Hexsight reads these fields of the League Client match history, and it uses no code of that project:
  - `/lol-summoner/v1/current-summoner`: `puuid`.
  - `/lol-gameflow/v1/session`: `gameData.gameId` and `gameData.queue.id`.
  - `/lol-match-history/v1/products/lol/<puuid>/matches?begIndex=0&endIndex=19`: `games.games[]` with `gameId`, `queueId`, and `gameCreation`. The list holds 20 games at most, and the client can keep an old list for a long time. So Hexsight reads a finished game by its ID.
  - `/lol-match-history/v1/games/<gameId>`: `queueId`, `gameCreation` (milliseconds), `gameDuration` (seconds), `participants[].championId`, `participants[].stats` (`win`, `kills`, `deaths`, `assists`, `playerAugment1` to `playerAugment6`), and `participantIdentities[].player.puuid`. The augments are in pick order: four picks and up to two bonus slots.
- **Interface:** Rust, egui, and eframe.

Hexsight loads the stats from aramkit first. If aramkit fails, Hexsight loads aramhex-data. The fallback has the champion table and the win rate of each augment. It has no builds, augment pairs, pick-screen lists, or tooltips. Hexsight computes the tiers of the fallback itself. It compares the win rate of each augment with the champion average, after a smoothing step for small samples.

Hexsight uses the KIWI list of CommunityDragon as the exact set of Mayhem augments.

The aramhex-data authors say that a win rate counts the players who hold the augment when the game ends. It shows a link, not a cause. The fallback data come from Chinese (Tencent) regions. Nobody confirmed the region of the aramkit games. They probably come from Chinese servers.

### How Hexsight gets the data

- At start, Hexsight loads the stats index. It does this again every 3 hours. After a failure, it tries again after 60 seconds. During a refresh, the screens keep the old details of a champion until the new details arrive. They never switch to "Loading stats".
- After the names and lists load, Hexsight downloads all augment icons one time (about 1 MB). The first pick screen then has its augment icons ready. A request from the window for an icon always goes before this download.
- Hexsight downloads the file of a champion when it needs it. The fetch thread looks at your own champion every 5 seconds until its file loads. It does this in every phase, also in the lobby and after the game, because Hexsight keeps the champion of the last game until the next champ select. The cost is one request each minute while the file does not load. After a failed try it waits 60 seconds, then tries again. A download of another champion that works ends this wait at once. While Hexsight waits, the build card says "Stats didn't load. Trying again…". It says "No data for this champion" only when the source has no data for the champion: the server answers 404, 410, or 403 for a champion that is not in the stats index, or the fallback table does not have the champion. Hexsight then asks again only after the next stats index. A champion that the stats index lists has a file, so a 404 for it means that the file is not on the server yet: Hexsight tries again after 60 seconds (test `a_missing_file_of_a_listed_champion_is_tried_again` in `src\data.rs`). The code is `fetch_loop`, `Shared::next_fetch`, `Shared::store_fetch`, and `Shared::no_stats` in `src\worker.rs`.
- In the game, the champion of the champ select of this game decides. Hexsight asks the Live Client for the champion only when it saw no champ select (it started during the game). The Live Client names the champion by its alias, and the shown name is only a fallback. The champion list of CommunityDragon also has copies for another mode (ids 60001 and up, alias `Jade_<alias>`) with the same shown name. Hexsight drops them when it loads the list, because no stats source has data for them. Tests: `the_shown_name_can_pick_the_jade_copy_of_a_champion`, `the_shown_name_is_only_a_fallback_and_takes_the_lowest_id`, and `the_summary_keeps_the_real_champions_and_drops_the_copies` in `src\data.rs`, and `the_champ_select_champion_stays_for_its_game` in `src\worker.rs`.
- Each failed download goes to the data log `hexsight.log` in the data folder, also when an old file from the cache covers the failure. See "The data log" below.
- Hexsight keeps each file in the `cache` folder. A file stays fresh for a time that depends on its type:
  - Version indexes: 30 minutes.
  - Stats: 6 hours.
  - Names, lists, and text: 1 day.
  - Icons and art: 30 days.
- If a download fails and the cache has an old file, Hexsight uses it. If no file exists, the footer shows "Data not updated" with the error.
- The refresh button in the footer reloads the data. Hexsight downloads only the files that passed their age limit. To download everything again, close Hexsight and delete the `cache` folder.

### The data log

`hexsight.log` in the data folder (`%LOCALAPPDATA%\Hexsight`, or `Hexsight-data` next to the exe in portable mode) gets one line for each data download that failed, and for each file that came but did not parse. A line holds the UTC time, the host and the path of the URL, the reason, and the bytes that came:

```
2026-10-25T01:40:13Z data.aramkit.com/data/16.21-20261024-79beb9ff65a7/stats/all/champion-details/60074.json status 404, 0 bytes
```

The reasons are `status <code>`, `timeout`, `no connection`, `body cut or too large`, `not a valid file` (for example an HTML page instead of JSON), and `client error`. The line never holds a query, a login, a Riot ID, a PUUID, or a path on your PC. When the log would pass 256 KB, Hexsight renames it to `hexsight.log.1` (an older `hexsight.log.1` goes) and starts a new log. When the rename fails (for example, a read-only `hexsight.log.1`), Hexsight empties `hexsight.log` and starts it again. The two files never hold more than 512 KB. Hexsight never sends the log. You can delete both files at any time.

The code is `src\datalog.rs` and `get_checked` in `src\cache.rs`. The tests write only to temp folders: `a_log_line_has_the_time_the_host_the_path_the_reason_and_the_bytes`, `a_log_line_never_holds_a_query_a_login_or_a_second_line`, `lines_go_to_the_end_of_the_log`, `a_full_log_moves_to_the_old_file_and_replaces_it`, and `a_full_log_that_cannot_move_starts_again_and_stays_under_the_cap` in `src\datalog.rs`, and `a_stale_file_answers_and_the_failed_download_is_still_reported`, `a_missing_file_on_the_server_is_status_404`, and `an_html_page_with_status_200_is_an_invalid_file_with_its_size` in `src\cache.rs`.

### Terms of use

The aramkit terms of use allow personal, non-commercial reference and say nothing about apps (https://aramkit.com/en-US/terms, read 2026-10-24). Read the terms of every data source before you share a build.

## 6. Privacy and safety

### What Hexsight reads

- The League Client API on your PC (127.0.0.1). Hexsight gets the port and the password from the `lockfile` in the League folder. It keeps them in memory and uses them only for local calls.
- The Live Client Data API on your PC (127.0.0.1, port 2999). In game, it gives the level, the death state, and the game time. While a suggestion setting is on, in a Mayhem game, it also gives your gold and the list of all ten players: the champion, the side, the items, the score line, the level, and the death state of each. They stay in memory for the game, they are never written or sent, and Hexsight keeps no player name.
- The match history of the League Client API (127.0.0.1): your player ID, the list of your newest games, and each finished Mayhem game of yours. Hexsight keeps only your own record of a game: champion, augments, result, kills, deaths, and assists. It does not keep the other nine players.
- A screen capture of the game area for card reading. Hexsight keeps the last image in memory (one image at most) and frees it 10 minutes after a game. It writes the image to disk only when you press "Save the last scan". It never sends the image.
- The file `Config\game.cfg` in the League folder. Hexsight reads one value from it, `WindowMode`, each time the client phase changes. It does not change the file.

Both local APIs use a self-signed certificate. Hexsight skips the certificate check for these two local addresses only.

### What Hexsight does not do

- It does not read or write game memory.
- It does not inject a DLL (dynamic-link library) into the game. It installs no hook.
- It does not open the game process (`League of Legends.exe`). It opens the `LeagueClientUx.exe` process only to read its file path.
- It does not record your keys. It checks only the state of the two hotkeys (F8 and F9 at first) and Ctrl. When you change a hotkey, Hexsight reads one key press in its own window.
- It does not send your games anywhere. `history.json` stays on your PC, and no network call carries it.
- It does not pick, ban, or chat for you. The write calls to the League Client are the bench swap, the trade calls, and the window mode fix (a click on "Set to Borderless"). The window has buttons for the bench swap and for accept and decline. Each call runs when you click a button, and Hexsight never sends one by itself. The calls ask and cancel run only from the test hook `HEXSIGHT_TRADE`.

### What Hexsight sends to the internet

Hexsight sends plain GET requests for public data files to the hosts in section 5. It sends no account data, no key, and no login. Each request has the user agent `Hexsight/0.1 (+https://github.com/<owner>/hexsight)`. `src\cache.rs` builds it from the constant `REPO` in `src\update.rs`. The file path of a request shows your client language and the champion IDs that you open. The only other request is the update check: once a day, a GET of `https://api.github.com/repos/<owner>/hexsight/releases/latest` (host `api.github.com`), only while the setting "Check for new versions" is on, and never while the repo in `src/update.rs` is the placeholder `OWNER`. It sends no body and no data. The source code has no other internet host.

### What Hexsight writes to your PC

- In portable mode, the files below go to `Hexsight-data` next to the exe, not to `%LOCALAPPDATA%\Hexsight` (section 2, "Portable mode").
- `%LOCALAPPDATA%\Hexsight\settings.json` and `%LOCALAPPDATA%\Hexsight\cache`. They hold the settings and the downloaded files.
- `%LOCALAPPDATA%\Hexsight\history.json`. It holds your saved Mayhem games, 500 at most, one line for each game. Delete it to clear your games.
- `%LOCALAPPDATA%\Hexsight\last-scan.png` and `%LOCALAPPDATA%\Hexsight\last-scan.txt`. Hexsight writes them only when you press "Save the last scan". They can show a part of your game screen.
- `%LOCALAPPDATA%\Hexsight\crash.log`. Hexsight writes one line to it only when it crashes. The file stays on your PC.
- `%LOCALAPPDATA%\Hexsight\hexsight.log` and `hexsight.log.1`. One line for each data download that failed: the time, the host and path of the URL, the reason, and the bytes. No player data. 256 KB at most for each file (section 5, "The data log"). The files stay on your PC.
- `<League folder>\Config\Global\Recommended\Hexsight-0.json`, `Hexsight-1.json`, and so on: one item set for each build. Hexsight writes only these files in the League folder, and it deletes its older files there. It deletes them all when you turn off "Send the build to the in-game shop". A file that Windows keeps locked stays, and Hexsight tries to delete it again in a later champ select.
- One registry value with the name `Hexsight` in `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`. Hexsight writes it only when the setting "Launch with Windows" is on. It deletes the value when you set the setting off.

The install script `tools\install.ps1` writes the exe copy and the two shortcuts. Hexsight itself does not write them.

### Fair play

- Hexsight shows the game count and the confidence of all offered cards, and the tier in the main window.
- By default, Hexsight draws no conclusion during play. The highlight of the top tier and the pair hints are off. You can turn each one on in the group Overlay of the Settings page. Hexsight never picks a card for you.
- Hexsight shows no augment win rate, and no number that comes from one. No setting turns them on.
- Hexsight suggests items from build data and from its own rules. It buys nothing and picks nothing. The reasons that name the enemy team are rules of Hexsight, not statistics, and the card says so. The item tips show no win rate and no pick rate. You can turn the tips off in the group Suggestions of the Settings page.
- Hexsight does not read game memory.
- Hexsight does not inject a DLL.
- Hexsight does not automate input. It does not click or press keys in the game.

Hexsight isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc.

The group About of the Settings page shows the same notice.

### Riot developer policy note

Riot publishes a list of unapproved use cases in its developer documentation. The list says that products cannot display win rates for augments or Arena items. It names websites, applications, and overlays. The page does not name Mayhem. For this reason, Hexsight shows no augment win rate in any form, and it has no setting for them. It also shows no number that comes from one: no gain over the average, no gain for a pair, and no win rate of your own with an augment. The win rate of a pair only picks the best pair. It never shows. The My games page shows the results of your own games and champions, not win rates of augments.

These are the approval rules for products that need a Riot production key. Hexsight uses no Riot API key. Nobody confirmed if Riot applies the rule to an app like Hexsight.

The same list names apps that dictate player decisions. For this reason, the pick views have no PICK tag and no words that tell you which card to take. They show the games and the confidence of all offered cards. The highlight of the best tier (the green frame, the word "top", and the header line) and the pair hints (the pairs line and the pairs mark) are off by default. Each one has its own switch, and one note says why. When you turn the pair hints on, the pairs line and the overlay mark always say "if picked earlier" or "if taken", because Hexsight cannot know your pick. The item tips (the cards "Next items" and "This game", and the advice card of the overlay) are closer to this line than the tier marks, so they have an off switch, show up to three items with a reason for each, use words that suggest and do not command, and show no win rate or pick rate. They are on by default. Whether Riot allows a live list of suggested items built from public build data in Mayhem is an open question for Riot Developer Relations. To start with the tips off, set `suggest_items` and `suggest_context` to false in `Settings::default` in `src\app.rs`. The beta badges are the only part that draws a tier on the game cards. They are off by default. Riot also says that it does not officially support the League Client API for third-party apps. The general policies ask products that use this API to register in the Developer Portal. Read the Riot developer policies before you share or publish Hexsight. The source is the Riot developer docs (https://developer.riotgames.com/docs/lol, read 2026-10-24).

## 7. Build from source

1. Install a Rust toolchain that supports edition 2024. `Cargo.toml` sets `edition = "2024"`.
2. Open a terminal in the source folder.
3. Run the build command:

   ```powershell
   cargo build --release
   ```

4. Find `hexsight.exe` in the `release` folder of the target directory.

By default, Cargo writes the build output to the `target` folder in the source folder. The file `.cargo\config.toml` holds a commented example for another folder.

The release profile builds a small exe: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, and `strip = true`. The exe is 6.06 MB.

The script `build.rs` adds `assets\icon.ico`, the product name, and the file description to the exe with the `winresource` crate. If this step fails, the build still works and Cargo prints a warning. The compiler also includes `assets\icon-64.png` in the exe. It is the window icon and the app mark in the title bar and the overlay.

To make the icon files from a logo image, use `tools\logo.py` (needs Python with Pillow and numpy). It writes `icon-16.png` to `icon-256.png`, `icon.ico`, and `social-preview.png` (the GitHub repository card) to `assets`. A transparent PNG of 512 px or more works best. On an image without transparency, the script makes the color of the four corners transparent. Then build again.

```powershell
py -3 tools\logo.py logo.png --preview logo-preview.png
```

The options: `--small` gives a simpler logo for 16 and 24 px, `--keep-bg` keeps the background, and `--preview` writes a sheet with all sizes on dark and light backgrounds.

The temporary icon (a gradient tile with a white H) comes from `tools\make_icon.ps1` and `tools\pack_ico.mjs`:

```powershell
powershell -File tools\make_icon.ps1 assets
node tools\pack_ico.mjs assets assets\icon.ico
```

To run the unit tests, use `cargo test`. The tests are in `src\app.rs`, `src\badges.rs`, `src\cache.rs`, `src\data.rs`, `src\history.rs`, `src\i18n.rs`, `src\itemset.rs`, `src\kit.rs`, `src\lcu.rs`, `src\main.rs`, `src\ocr.rs`, `src\suggest.rs`, `src\theme.rs`, `src\update.rs`, `src\win.rs`, and `src\worker.rs`. There are 438 tests. Two of them (`card_presence_on_real_frames` and `the_files_of_the_heimerdinger_games`) are ignored and run only with `--ignored` and real frames or files. The match history parser has a test game in the shape of the League Client. The parser of the champ select session has a test session with all five trade states, with a trade that Hexsight drops, and without `trades`. A test checks that a trade call tries the `trades` route first and the `champion-swaps` route second. Other tests cover the screen for each game phase, the next pick level, the window height (each main page screen at the heights 680 and 520, with a bench of 1 to 10 and up to two trade requests), the item set cleanup (also with a locked file), the pick cells with their confidence, and the queue of short messages. The bench tests check the grid (five in a row, a bench of 10 in two rows), the order, the green glow on the best swap only, the line under the grid, and that the badge takes the click of its icon and stays clear of the glow of the next icon. A test checks that a frame that egui cannot lay out again asks for the next frame at once. The tests of a failed client call check that the message names the reason and never the raw error. The coordinate mapping of the badges has tests for the scales 1.0 to 3.0, for a game on a second monitor, and for the sizes of the pills.

The text table of the two languages is in `src\i18n.rs`. Each line of the table holds the English text and the Turkish text of one key. The tests check that each key has a text in both languages. They also check that both texts fill the same places (`{0}`, `{1}`). Other tests check that Auto picks Turkish for `tr_TR` and English for `en_US`. Other tests measure the title bar, the pill, the overlay cells, the trade buttons, the build row, the best swap line, the reading language row, and the checklist in the font of the app. They run in both languages. The tests use 246 points (`CARD_INNER`) for the inside of a card: 272 points less the margins of 12 and the 1 point edge at each side.

## 8. Developer test hooks

The commands use Windows PowerShell. Only `capture.sh` needs Git Bash. Run them in the source folder. In this section, `$exe` is the path of `hexsight.exe`.

```powershell
$exe = "$PWD\target\release\hexsight.exe"
```

### Mock League Client

The script `tools\mock\mock.mjs` is a Node script. It fakes the League Client API and the Live Client Data API on your PC.

1. Start the mock. The phase is ChampSelect, InProgress, Lobby, or None. The default is ChampSelect.

   ```powershell
   node tools\mock\mock.mjs ChampSelect
   ```

2. In a second terminal, start Hexsight with `HEXSIGHT_LEAGUE_DIR` set to the `tools\mock\league` folder.

   ```powershell
   $env:HEXSIGHT_LEAGUE_DIR = "$PWD\tools\mock\league"
   & $exe
   ```

3. Change the phase while the mock runs.

   ```powershell
   curl.exe -k -u riot:testpass https://127.0.0.1:52811/mock/phase/InProgress
   ```

4. When you finish, remove the variable.

   ```powershell
   Remove-Item Env:HEXSIGHT_LEAGUE_DIR
   ```

The mock writes `tools\mock\league\lockfile` and `tools\mock\league\Config\game.cfg` when it starts. The game file holds `WindowMode=2` (Borderless). These paths change the mock while it runs. They use port 52811 and the same user and password as the League Client API:

- `/mock/phase/<phase>`: the client phase (ChampSelect, InProgress, Lobby, or None).
- `/mock/me/<champion id>`: your champion in champ select. Use an ID without stats (for example 60001) to test the no-data screens.
- `/mock/bench/<0 or 1>`: 0 gives a champ select without a bench.
- `/mock/me/0` gives a player without a champion (the first step of a champ select). The route `/lol-lobby-team-builder/champ-select/v1/subset-champion-list` then answers `[164, 235, 12]`. `/mock/subset/<id,id,id>` sets the offer, `/mock/subset/empty` gives `[]`, and `/mock/subset/missing` makes the route answer 404, as an older client does.
- `/mock/window/<0, 1, or 2>`: writes `WindowMode` into `game.cfg`. Use 0 to test the Fullscreen note.
- `/mock/late/<n>`: the game of the gameflow session (ID 7000100) answers "not found" for the next n requests. Use it to watch the tries of the history thread.
- `/mock/trade/<cell>/<state>`: sets the state of the trade with the teammate in that cell (0, 1, 3, or 4; you are in cell 2). The state is AVAILABLE, BUSY, INVALID, RECEIVED, or SENT. The ID of a trade is 100 plus the cell. At the start, cell 0 and cell 1 are AVAILABLE, cell 3 is BUSY, and cell 4 is INVALID. Use `/mock/trade/1/RECEIVED` to see the card "A teammate wants your champion".
- `/mock/route/<trades or swaps>`: picks the route of the trade calls that exists. The other route answers 404, as an older client does. Use `swaps` to see the fall back of Hexsight from `trades` to `champion-swaps`.
- `/mock/trades`: shows the champions, the trades, and the route of the mock. Use it to check the result of a click.
- `/lol-game-settings/v1/game-settings` (GET and PATCH) and `/lol-game-settings/v1/save` (POST): the game settings, as the client keeps them. A PATCH with `General.WindowMode` waits, and the save writes `game.cfg`. They serve the button "Set to Borderless". Use `/mock/window/0` first.

The mock answers a POST on `/lol-champ-select/v1/session/<trades or champion-swaps>/<id>/<request, accept, decline, or cancel>` as the client does. `request` needs the state AVAILABLE and gives SENT. `accept` needs RECEIVED and swaps your champion with the champion of the teammate. `decline` needs RECEIVED, and `cancel` needs SENT. After these three, the state is AVAILABLE. A call in the wrong state answers 400. The session of the phase ChampSelect has the field `trades`. A restart of the mock puts all states and champions back.

The mock also answers the match history calls of section 5, in the shape of the League Client. `/lol-summoner/v1/current-summoner` gives the player ID `mock-puuid-0001`. The match list holds 3 Mayhem games (champions Yasuo and Jinx) and 1 ARAM game. The game 7000100 is the game of the session. It is not in the list, as with an old list in the real client. A first start without `history.json` saves the 3 Mayhem games. A switch to the phase EndOfGame saves the game 7000100.

The mock gives the Live Client ten players with items, sides and score lines (made-up names, Soraka as a healer, Zed fed at 9/2/4) and `currentGold` 1234.5 in `activeplayer`, so the item tips show. The view `overlay-advice` of `tools\capture.sh` needs a short `DELAY` (6 or less), because the advice card of the overlay shows for 12 seconds after the first tips.

The mock listens on 127.0.0.1, port 52811, for the League Client API (user `riot`, password `testpass`). It listens on port 2999 for the Live Client Data API, and it answers there only in the phase InProgress. It uses `key.pem` and `cert.pem` in `tools\mock`. Make them first with `bash tools/mock/make_cert.sh` (needs openssl). In a mock run, the item set goes to `tools\mock\league\Config\Global\Recommended\Hexsight-0.json` and its siblings, not to your real League folder.

CAUTION: The mock uses port 2999, the port of the real Live Client Data API. Do not run the mock during a real game.

CAUTION: The mock has fake games. A copy of Hexsight that runs against it saves them in `history.json`. Start Hexsight against the mock only with another `LOCALAPPDATA` (see "Capture hooks"), or the fake games go into your own history.

### Capture hooks

These environment variables change how Hexsight starts. They work with a locked PC screen.

- `HEXSIGHT_SHOT`: the path of a PNG file. Hexsight saves a screenshot of its window and exits.
- `HEXSIGHT_SHOT_DELAY`: the seconds to wait before the screenshot. The default is 10.
- `HEXSIGHT_VIEW`: one of `games`, `settings`, `rebind`, `overlay`, or `badges`. The view `games` shows the My games page. The view `settings` shows the Settings page. The view `overlay` opens the overlay at once. The view `badges` draws the badge layer in the main window, over three fake cards. It needs `HEXSIGHT_OFFER`. It uses the same code that places the badges over the game, so you can see the layout while the PC is locked. The view `rebind` shows the Settings page while it waits for a new overlay hotkey.
- `HEXSIGHT_OFFER`: three augment IDs with commas. It shows a fake set of cards on pick screen 2. The cards sit left, middle, and right, in the order of the IDs. The game area is 1920x1080 at the corner of the screen. With the setting "Show badges on augment cards (beta)" on, the badge window opens over that area. The main window shows the pick card in every phase.
- `HEXSIGHT_PICKS`: augment IDs with commas. They act as the cards that the earlier pick screens offered: the first part is for screen 1. A part can list several IDs with a slash, for example `1206/1058`. With `HEXSIGHT_OFFER` and `"pair_hints": true` in `settings.json`, the cards then show the pairs line.
- `HEXSIGHT_SCROLL`: a number of points. The Settings page and the My games page scroll to this offset. A large number shows the end of the page.
- `HEXSIGHT_TRADE`: `<call>:<trade id>:<seconds>`, for example `accept:101:8`. The call is `request`, `accept`, `decline`, or `cancel`. At that time after the start, Hexsight sends the call as a click on its button does, so you can test the trade path and the message without a click. The seconds are optional (the default is 5). Pair it with the mock phase ChampSelect.
- `HEXSIGHT_BADGE_SHOT`: the path of a PNG file. The badge window saves what it draws, with a transparent background, once. `HEXSIGHT_BADGE_SHOT_DELAY` sets the wait in seconds after the first drawing (the stats need time to load). This works while the PC is locked, because it reads the picture from the window and not from the screen.
- `HEXSIGHT_SAVE_SCAN`: a number of seconds. At this time after the start, Hexsight acts as if you pressed "Save the last scan".
- `HEXSIGHT_SCAN_IMAGE`: the path of a PNG file. The card reader reads this image in place of the screen, with the same crop. A game window counts as found. Use it with the mock phase InProgress and a pick screen from `tools\fake_pick.ps1`. The scan, the card, and the Settings numbers then work without a game.
- `HEXSIGHT_OCR_MAX_SIDE`: a number of pixels. Hexsight uses it as the size limit of the OCR engine. With a small number you can test the halving of large images. The test PC has a limit of 10000 pixels.
- `HEXSIGHT_WINDOWS_BUILD`: a Windows build number. Hexsight uses it in place of the real build. With 19045 (Windows 10) the glass is off and the glass switch is dim.

The script `tools\mock\make_history.mjs` writes a fake `history.json` with many games, so the My games page has many rows. It needs the data folder of a test run as its first argument. It refuses the real folder `%LOCALAPPDATA%\Hexsight`, so your own history stays clean.

```powershell
node tools\mock\make_history.mjs "$env:TEMP\hexsight-test\Hexsight" 40
```

The script `tools\capture.sh` renders each view against the mock and saves `<out-dir>\<view>.png`. The default views are select, waiting, pick, games, settings, overlay, and overlay-offer. The extra views are overlay-preview (the pill outside a game), overlay-black (the pill with the hint for a black capture), settings-black, badges (the badge layer), trade (champ select with a trade request from a teammate, then the trade goes back to AVAILABLE), and first-run (the checklist). The views `pick` and `overlay-offer` also set `HEXSIGHT_PICKS=1206`, so the card of Yasuo shows a pairs line (with `"pair_hints": true` and `"top_highlight": true` in `settings.json`; both are off by default) (the augment IDs follow the data of patch 16.21). Start the mock first. The `DELAY` variable sets the wait in seconds. The default is 9.

The captures read and write the settings in `%LOCALAPPDATA%\Hexsight`. To keep your own settings, set the environment variable `LOCALAPPDATA` to another folder for the run. Copy the `Hexsight\cache` folder there first, or Hexsight downloads the data again. A run against the mock also saves the fake games of the mock in `history.json`. So `capture.sh` refuses to run when `LOCALAPPDATA` is your real folder. Set `HEXSIGHT_CAPTURE_REAL=1` to run anyway. A `settings.json` with only `{"overlay_scale": 2}` shows the overlay at size L. A `settings.json` with `{"language": "tr"}` shows the labels in Turkish, and `{"language": "en"}` shows them in English. The key `width` sets the width of the window in points.

```bash
bash tools/capture.sh "target/release/hexsight.exe" shots
```

CAUTION: Before each view, `capture.sh` stops every running process with the same exe name, for example your own `hexsight.exe`.

### Window screenshot

The script `tools\shot.ps1` saves a PNG of the main window of a running process with PrintWindow. This works when another window covers the window. Add `screen` as the third argument to copy from the screen instead. The script also prints the window size and position, the working set, and the private memory (in MB).

```powershell
powershell -File tools\shot.ps1 hexsight shot.png
```

### Memory and processor use

The script `tools\measure.ps1` measures memory and processor use over a time window. It adds up all processes with the same name. The defaults are 30 seconds and the name `hexsight`. For each name, it prints the process count, the working set, and the private memory. It also prints the thread count and the CPU share of all cores.

```powershell
powershell -File tools\measure.ps1 -Seconds 30 -Names hexsight
```

Start the apps first. For idle values, leave the apps idle. For in-game values, run the script during a game.

### Card reading on an image file

`HEXSIGHT_OCR_FILE` runs the card reader on an image file and exits. It uses the same crop as the live capture. It writes the result to the file path in `HEXSIGHT_OCR_OUT`. The default is `ocr-result.txt` in the current folder. `HEXSIGHT_LOCALE` sets the client language of the augment names. The default is `tr_TR`. The hook downloads the names or reads them from the cache.

The exe has no console window, so the result goes to the file. The first line of the file shows the OCR time in milliseconds and the number of text lines. Then each line shows its center position and its text. The last line lists the augment IDs and names that Hexsight found.

The script `tools\fake_pick.ps1` draws a synthetic 1920x1080 pick screen with three cards. The optional parameters `-Font` (default Georgia) and `-Size` (default 22) set the title font and the title size.

```powershell
powershell -File tools\fake_pick.ps1 pick.png "Mistik Yumruk" "Titanın Azmi" "Kaçış Planı"
$env:HEXSIGHT_OCR_FILE = "pick.png"
Start-Process -Wait $exe
Get-Content ocr-result.txt
Remove-Item Env:HEXSIGHT_OCR_FILE
```
