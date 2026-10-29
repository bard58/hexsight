#!/usr/bin/env bash
# Render app views against the mock client and save PNGs (works with a locked screen).
# Needs: node tools/mock/mock.mjs running. Usage: tools/capture.sh <exe> <out-dir> [views...]
# Views: select waiting pick games settings overlay overlay-preview overlay-offer overlay-black
# settings-black badges trade first-run
# (select = champ select with the bench and the build, waiting = in a game between pick screens,
# pick = a pick screen in the main window, games = the My games page, overlay = the pill of a game
# between pick screens, overlay-preview = the pill opened outside a game, overlay-offer = the strip of a
# pick screen, overlay-black = the pill with the hint for a black capture, settings-black = Settings
# with a black capture, badges = the badge layer drawn in the main window, trade = champ select with
# a trade request from a teammate, first-run = the checklist, before a champion is known).
# The views waiting and overlay also show the item tips (the card "Next items", the card "This game", and for
# a few seconds the advice card in the overlay) from the ten players that the mock gives to the Live Client.
# overlay-advice = the overlay in the 12 seconds after the first tips, so use DELAY=6 or less (not run here).
set -u
EXE="$1"; OUT="$2"; shift 2
HERE="$(cd "$(dirname "$0")" && pwd)"
VIEWS="${*:-select waiting pick games settings overlay overlay-offer}"
# The mock has fake games, and Hexsight saves them in history.json. Refuse the real data folder.
real="$(cygpath -m "$USERPROFILE/AppData/Local" 2>/dev/null || echo "$USERPROFILE/AppData/Local")"
now="$(cygpath -m "${LOCALAPPDATA:-}" 2>/dev/null || echo "${LOCALAPPDATA:-}")"
if [ "${HEXSIGHT_CAPTURE_REAL:-}" != 1 ] && [ "${now,,}" = "${real,,}" ]; then
  echo "Refused: LOCALAPPDATA is your real folder, and the mock would put fake games in your history.json." >&2
  echo "Set LOCALAPPDATA to another folder (with a copy of Hexsight\cache), or set HEXSIGHT_CAPTURE_REAL=1." >&2
  exit 1
fi
mkdir -p "$OUT"
phase() { curl -sk -u riot:testpass "https://127.0.0.1:52811/mock/phase/$1" >/dev/null; }
# State of the trade with the teammate in cell $1 (the mock gives ids 100 + cell).
trade() { curl -sk -u riot:testpass "https://127.0.0.1:52811/mock/trade/$1/$2" >/dev/null; }
for v in $VIEWS; do
  taskkill //IM "$(basename "$EXE")" //F >/dev/null 2>&1
  extra=()
  case "$v" in
    select) phase ChampSelect ;;
    # The teammate in cell 1 asks for your champion. The state goes back to AVAILABLE after the view.
    trade) phase ChampSelect; trade 1 RECEIVED ;;
    waiting) phase InProgress ;;
    # HEXSIGHT_PICKS=1206 is a card that pick screen 1 offered, so the cards of screen 2 show the pairs hint.
    pick) phase InProgress; extra=(HEXSIGHT_OFFER=1058,3078,1337 HEXSIGHT_PICKS=1206) ;;
    games) phase Lobby; extra=(HEXSIGHT_VIEW=games) ;;
    settings) phase Lobby; extra=(HEXSIGHT_VIEW=settings) ;;
    # In a game the mouse passes to the game, and between pick screens the overlay is a small pill.
    overlay) phase InProgress; extra=(HEXSIGHT_VIEW=overlay) ;;
    # Outside a game the overlay takes clicks, and a click on the pill closes it.
    overlay-preview) phase Lobby; extra=(HEXSIGHT_VIEW=overlay) ;;
    overlay-offer) phase InProgress; extra=(HEXSIGHT_VIEW=overlay HEXSIGHT_OFFER=1058,3078,1337 HEXSIGHT_PICKS=1206) ;;
    overlay-advice) phase InProgress; extra=(HEXSIGHT_VIEW=overlay) ;;
    overlay-black) phase InProgress; extra=(HEXSIGHT_VIEW=overlay HEXSIGHT_CAPTURE_FAULT=black) ;;
    settings-black) phase Lobby; extra=(HEXSIGHT_VIEW=settings HEXSIGHT_CAPTURE_FAULT=black) ;;
    badges) phase InProgress; extra=(HEXSIGHT_VIEW=badges HEXSIGHT_OFFER=1058,3078,1337 HEXSIGHT_PICKS=1206) ;;
    # No champion yet: the first-run card. HEXSIGHT_FIRST_RUN asks "Start with Windows?" even
    # when Windows starts Hexsight already.
    first-run) phase Lobby; extra=(HEXSIGHT_FIRST_RUN=1) ;;
    *) phase Lobby; extra=(HEXSIGHT_VIEW="$v") ;;
  esac
  env HEXSIGHT_LEAGUE_DIR="$HERE/mock/league" HEXSIGHT_SHOT="$OUT/$v.png" HEXSIGHT_SHOT_DELAY="${DELAY:-9}" "${extra[@]}" "$EXE"
  echo "$v -> $OUT/$v.png ($(stat -c %s "$OUT/$v.png" 2>/dev/null || echo missing) bytes)"
  [ "$v" = trade ] && trade 1 AVAILABLE
done
phase ChampSelect
