//! Interface language: English and Turkish. One static table, and no file is
//! read at run time.
//!
//! `Key` names each text. `Lang::t` gives the text. `Lang::fmt` fills the
//! places `{0}`, `{1}` ... of a text. The number helpers follow the language
//! too: English writes "67.2%", Turkish writes "%67,2".
//!
//! Names of champions, augments and items are not here. They come from the
//! client language (CommunityDragon), so a Turkish client shows Turkish names
//! in both languages of the interface.

use crate::data::LoadError;
use crate::lcu::CallFailure;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Write};

/// The language of the interface, after "Auto" is resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Tr,
}

/// The setting "Language". Auto follows the language of the League Client.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangPref {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "tr")]
    Turkish,
    /// Also the value for a text that Hexsight does not know (a hand-edited file).
    #[default]
    #[serde(rename = "auto", other)]
    Auto,
}

impl LangPref {
    /// The language of the interface for this setting. `locale` is the locale of
    /// the client, for example "tr_TR". It is empty until the client answers: the
    /// caller then passes the locale that Hexsight saw the last time.
    pub fn resolve(self, locale: &str) -> Lang {
        match self {
            LangPref::English => Lang::En,
            LangPref::Turkish => Lang::Tr,
            LangPref::Auto if locale.trim().to_ascii_lowercase().starts_with("tr") => Lang::Tr,
            LangPref::Auto => Lang::En,
        }
    }
}

/// Defines `Key` (one name for each text), `KEYS` (all of them), and the table
/// of texts. Each line holds the English text and the Turkish text, so the two
/// cannot slip out of line.
macro_rules! table {
    ($($key:ident = $en:literal, $tr:literal;)*) => {
        /// One text of the interface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Key {
            $($key,)*
        }

        /// Every key, in table order. The tests walk it.
        #[cfg(test)]
        pub const KEYS: &[Key] = &[$(Key::$key,)*];

        /// The texts: English first, then Turkish. The index is the key.
        static TEXTS: &[[&str; 2]] = &[$([$en, $tr],)*];
    };
}

table! {
    // ---- Title bar
    PhaseOffline = "League closed", "League kapalı";
    PhaseClient = "League open", "League açık";
    PhaseLobby = "Lobby", "Lobi";
    PhaseChampSelect = "Champ select", "Şampiyon seçimi";
    PhaseInGame = "In game", "Oyunda";
    PhasePostGame = "Post-game", "Maç sonu";
    // The X of the main page hides the window. The app keeps running.
    TipClose = "Minimize the window. Hexsight keeps running.", "Pencereyi küçült. Hexsight çalışmaya devam eder.";
    TipSettings = "Settings", "Ayarlar";
    TipBack = "Back", "Geri";
    HotkeyCode = "Key {0}", "Tuş {0}";

    // ---- Main page: your champion, and the first-run card
    HeroReady = "Ready for Mayhem", "Şamata’ya hazır";
    // {0} is the overlay key. It shows twice: once alone, once after "Ctrl+".
    HeroStartGame = "Start an ARAM: Mayhem game. In game, {0} shows or hides the overlay, and Ctrl+{0} unlocks it so you can drag it.",
        "Bir Şamata maçına gir. Oyunda {0} ile paneli açıp kapatırsın, Ctrl+{0} ile kilidini açıp taşırsın.";
    // The title of the first-run card while League is closed.
    HeroOpenLeague = "Open League", "League’i aç";
    // The buttons under the checklist of the first-run card.
    RetryButton = "Try again", "Yeniden dene";
    OpenLangSettings = "Language settings", "Dil ayarları";
    FullscreenNote = "League is in fullscreen mode. Switch to Borderless to read augment cards (Settings > Video > Window Mode).",
        "League tam ekran modunda. Kartları okumak için çerçevesiz moda geç (Ayarlar > Video > Pencere Modu).";
    HeroLoading = "Loading stats…", "İstatistikler yükleniyor…";
    HeroRetrying = "Stats didn't load. Trying again…", "İstatistikler yüklenemedi, yeniden deneniyor…";
    TipChampTier = "Champion tier in ARAM: Mayhem", "ARAM: Şamata’da şampiyonun kademesi";
    // The mode under the champion name, shown in capital letters.
    ModeName = "ARAM: Mayhem", "ARAM: Şamata";
    NoChampion = "No champion yet", "Henüz şampiyon yok";
    SecBench = "Bench", "Yedekler";
    SecBuild = "Build", "Dizilim";
    // The first step of a Mayhem champ select: the client offers some champions, and the player has none yet.
    SecChoose = "Pick your champion", "Şampiyonunu seç";
    ChooseNote = "You choose in League. Hexsight picks nothing.", "Seçimi League’de sen yaparsın. Hexsight seçim yapmaz.";
    ChooseEmpty = "Choose your champion in League.", "Şampiyonunu League’de seç.";
    SecAugments = "Augments", "Eklentiler";

    // The button next to the Fullscreen warning. It sets League to Borderless, only when clicked.
    FixBorderless = "Set to Borderless", "Çerçevesiz yap";
    // The same button while League is closed.
    FixOpenLeague = "Open League first", "Önce League’i aç";
    FixTip = "Sets the window mode of League to Borderless. It applies from the next game.",
        "League’in pencere modunu çerçevesiz yapar. Bir sonraki maçta geçerli olur.";
    FixDone = "League is set to Borderless. It applies from the next game.", "League çerçevesiz moda alındı. Bir sonraki maçta geçerli olur.";
    FixStill = "League took the change, but still shows the old window mode. Restarting League may apply it.",
        "League değişikliği kabul etti ama pencere modu hâlâ eski görünüyor. League’i yeniden başlatmak işe yarayabilir.";
    FixSent = "Sent to League. Check the window mode in Settings > Video.", "League’e gönderildi. Pencere modunu Ayarlar > Video bölümünden kontrol et.";
    FixFailed = "Couldn't set Borderless: {0}", "Çerçevesiz moda alınamadı: {0}";

    // ---- Summoner spells in champ select
    SpellsLabel = "Spells", "Büyüler";
    SpellsTip = "Suggested summoner spells. You pick them yourself in League's champ select.",
        "Önerilen sihirdar büyüleri. Seçimi League’de, şampiyon seçiminde sen yaparsın.";

    // ---- First-run checklist
    CheckLeagueOk = "League detected", "League bulundu";
    CheckLeagueNo = "Open League so Hexsight can find it", "League’i aç, Hexsight onu bulsun";
    CheckBorderlessOk = "League is in Borderless mode", "League çerçevesiz modda";
    CheckWindowedOk = "League is in Windowed mode, which also works", "League pencereli modda, bu da olur";
    CheckBorderlessNo = "Set League to Borderless: Settings > Video > Window Mode", "League’i çerçevesiz moda al: Ayarlar > Video > Pencere Modu";
    CheckOcrOk = "Card reading is ready", "Kart okuma hazır";
    CheckOcrNo = "Add English (United States) to Windows to read cards", "Kart okuma için Windows’a İngilizce (ABD) dilini ekle";
    CheckOcrWait = "Getting card reading ready…", "Kart okuma hazırlanıyor…";
    CheckDataOk = "Data loaded", "Veriler yüklendi";
    CheckDataNo = "Couldn't download data. Check your internet.", "Veriler indirilemedi. İnternetini kontrol et.";
    CheckDataWait = "Loading data…", "Veriler yükleniyor…";
    CheckHotkeyOk = "Hotkeys {0} and {1} are available", "{0} ve {1} tuşları kullanılabilir";
    CheckHotkeyNo = "{0} is taken by another app. Change it in Settings.", "{0} başka bir uygulamada kullanılıyor, Ayarlar’dan değiştir";
    // The one line of the card when every check passes.
    CheckAllOk = "Everything is ready", "Her şey yolunda";
    AskAutostart = "Start with Windows in the background?", "Windows açılınca arka planda başlasın mı?";
    AskYes = "Yes", "Evet";
    AskNo = "No", "Hayır";

    // ---- Footer, loading and empty states
    OfflineData = "Data not updated", "Veri güncellenemedi";
    FooterSource = "{0} · patch {1}", "{0} · {1} yaması";
    // The left text of the footer. The name of the data source is in its tooltip.
    FooterPatch = "Patch {0}", "{0} yaması";
    LoadingData = "Loading data…", "Veriler yükleniyor…";
    ReloadData = "Download the data again", "Verileri yeniden indir";
    NoDataChampion = "No data for this champion", "Bu şampiyon için veri yok";
    NoBuildData = "No build data", "Dizilim verisi yok";

    // ---- Waiting in a game: the next pick
    // The one line of the next-pick card. {0} is the level of the next pick screen.
    NextPickAt = "Next pick: level {0}", "Sonraki seçim: {0}. seviye";
    PicksDone = "No more pick screens", "Seçim ekranı kalmadı";
    Level = "Level {0}", "Seviye {0}";

    // ---- Pick screen (main window and overlay strip)
    PickTitle = "Augment selection", "Eklenti seçimi";
    // The header line. Turkish keeps one form after a number, English has two.
    TopOne = "1 card in the top tier", "1 kart en üst kademede";
    TopMany = "{0} cards in the top tier", "{0} kart en üst kademede";
    TierEqual = "Equal tiers", "Kademeler eşit";
    TopLabel = "top", "en üst";
    // Next to the link mark in a strip cell, which has room for about 8 characters.
    // The word says that the augment pairs with another one. The caveat that Hexsight
    // cannot know the earlier pick stays in `PairsLine` and `TipPairsOffered`.
    PairMark = "pairs", "eşleşir";
    ConfHigh = "high", "yüksek";
    ConfMid = "medium", "orta";
    // The only confidence word that a cell shows. "low" read as "this card is weak". The strip
    // cell has 84 points for the bars (34), a gap (6) and this word, so it must stay under 44
    // points at 11.5 points semibold: "few games" (56 points) did not fit, "limited" is 36.
    ConfLow = "limited", "az veri";
    PosLeft = "Left", "Sol";
    PosMiddle = "Middle", "Orta";
    PosRight = "Right", "Sağ";
    // A count with its noun. Turkish keeps one form after a number, English has two.
    CapGame1 = "{0} game", "{0} maç";
    CapGames = "{0} games", "{0} maç";
    CapWin1 = "{0} win", "{0} galibiyet";
    CapWins = "{0} wins", "{0} galibiyet";
    CapAugment1 = "{0} augment", "{0} eklenti";
    CapAugments = "{0} augments", "{0} eklenti";
    PairsLine = "Pairs well with {0} if picked earlier", "Önceden seçtiysen {0} ile iyi gider";
    // The pairs line when Hexsight has no name for the partner augment: a whole line, not a name
    // inside `PairsLine`, so Turkish does not read "Önceden seçtiysen önceki seçimin ile".
    PairsLineEarlier = "Pairs well with an earlier pick", "Önceki seçiminle iyi gider";

    // ---- After the game
    AfterWin = "Victory", "Galibiyet";
    AfterLoss = "Defeat", "Mağlubiyet";
    AfterOver = "Game over", "Maç bitti";
    AfterWaiting = "The result shows here when the game is saved.", "Sonuç, maç kaydedilince burada görünür.";
    // The card for a game that is not ARAM Mayhem.
    OtherMode = "Not an ARAM: Mayhem game", "ARAM: Şamata maçı değil";
    OtherModeNote = "Hexsight reads cards and saves results in ARAM: Mayhem only.", "Hexsight yalnızca ARAM: Şamata maçlarında kartları okur ve sonuçları kaydeder.";

    // ---- Bench
    // The tooltip of the swap button: {0} is the bench champion and its tier, {1} is yours.
    // `best_swap_parts` splits the text at {0} and {1}, so {0} stays before {1}.
    BenchBestSwap = "Best swap: {0} · yours is {1}", "En iyi değişim: {0} · seninki {1}";
    // The button under the bench grid. {0} is the bench champion and its tier, for example "Yasuo (S)".
    BenchSwapButton = "Swap to {0}", "{0} ile değiştir";
    // The last line of the tooltip of a bench champion. A swap with the bench is a "değişim";
    // a trade with a teammate is a "takas".
    BenchClickToSwap = "Click to swap", "Değiştirmek için tıkla";
    NoData = "No data", "Veri yok";
    TradeAccept = "Accept", "Kabul et";
    TradeDecline = "Decline", "Reddet";
    TradeWants = "A teammate wants your champion", "Takım arkadaşın şampiyonunu istiyor";
    // {0} is the champion you get, {1} the one your teammate gets. Turkish cannot add the
    // accusative to a name in a template, so it says "comes to you" and "goes to your teammate".
    TradeGetGive = "You get {0}; they get {1}", "{0} sana gelir, {1} takım arkadaşına gider.";

    // ---- My games page
    MyGames = "My games", "Maçlarım";
    GamesNone = "No games yet", "Henüz maç yok";
    GamesNoneBody = "Play an ARAM: Mayhem game. Your results show here.",
        "Bir ARAM: Şamata maçı oyna. Sonuçların burada görünür.";
    GamesImporting = "Checking your last 20 games…", "Son 20 maçına bakılıyor…";
    GamesImportingBody = "Adding your Mayhem games from match history.",
        "Hexsight, maç geçmişindeki Şamata maçlarını ekliyor.";
    GamesOverall = "Overall", "Genel";
    GamesWinRate = "Win rate", "kazanma oranı";
    GamesLast10 = "Last 10", "Son 10";
    GamesLast10Tip = "Oldest on the left, newest on the right", "Solda en eski, sağda en yeni";
    // The score after the ten dots, and the end of the totals line. {0} is the wins, {1} the
    // losses. English writes W and L, Turkish writes G (galibiyet) and M (mağlubiyet).
    GamesLast10Score = "{0}W {1}L", "{0}G {1}M";
    GamesYourChampions = "Your champions", "Şampiyonların";
    // The list of recent games, one row each: its title, and the button that shows the older rows
    // (and takes them back).
    GamesRecent = "Recent games", "Son maçlar";
    GamesShowAll = "Show all", "Tümünü göster";
    GamesShowLess = "Show fewer", "Daha az göster";
    // The switch above the champions: the five most played, or every champion with a game.
    GamesAllChampions = "All champions", "Tüm şampiyonlar";
    // The result of a row, as a letter next to the disc or the ring. They are the letters of the
    // "Last 10" score: W and L in English, G (galibiyet) and M (mağlubiyet) in Turkish.
    GameWinShort = "W", "G";
    GameLossShort = "L", "M";
    // The date of a row. {0} is the time "15:31" (24 hours, local time).
    GameToday = "Today {0}", "Bugün {0}";
    GameYesterday = "Yesterday {0}", "Dün {0}";
    // An older date: {0} is the day, {1} the short month, {2} the year. The year shows only for
    // another year than this one. English writes "Oct 26" and Turkish writes "26 Eki".
    GameDate = "{1} {0}", "{0} {1}";
    GameDateYear = "{1} {0}, {2}", "{0} {1} {2}";
    // The short names of the months. March and May read the same in both languages.
    MonthJan = "Jan", "Oca";
    MonthFeb = "Feb", "Şub";
    MonthMar = "Mar", "Mar";
    MonthApr = "Apr", "Nis";
    MonthMay = "May", "May";
    MonthJun = "Jun", "Haz";
    MonthJul = "Jul", "Tem";
    MonthAug = "Aug", "Ağu";
    MonthSep = "Sep", "Eyl";
    MonthOct = "Oct", "Eki";
    MonthNov = "Nov", "Kas";
    MonthDec = "Dec", "Ara";
    GamesSavedNote = "Your games stay on this PC. Hexsight never sends this data anywhere.",
        "Maç geçmişin yalnızca bu bilgisayarda saklanır. Hexsight bu verileri başka yere göndermez.";

    // ---- Settings: groups and rows
    SettingsTitle = "Settings", "Ayarlar";
    GroupGeneral = "General", "Genel";
    GroupOverlay = "Overlay", "Panel";
    GroupBuild = "Build", "Dizilim";
    GroupCardReading = "Card reading", "Kart okuma";
    GroupAbout = "About", "Hakkında";
    SetAutoOverlay = "Show the overlay when a game starts", "Oyun başladığında paneli göster";
    // The three marks (the top tier, the pairs and the fit) are off by default. One note covers all three.
    SetTopHighlight = "Highlight top-tier cards", "En üst kademedeki kartları vurgula";
    SetPairHints = "Show pair hints", "Eşleşme ipuçlarını göster";
    SetMarksNote = "Off by default. Turn them on for extra marks on pick screens.",
        "Varsayılan olarak kapalı. Açarsan seçim ekranında ek işaretler görünür.";
    // The English texts say "overlay" (the Turkish ones say "panel").
    SetHideInCaptures = "Hide the overlay from recordings and screenshots", "Paneli ekran kayıtlarından ve ekran görüntülerinden gizle";
    SetHideInCapturesNote = "Turn it off to show the overlay in streams. Hexsight then skips the overlay text when it reads cards.",
        "Kapatırsan panel yayınlarda görünür. Hexsight kartları okurken panelin yazısını atlar.";
    SetBadges = "Show badges on augment cards (beta)", "Kartlara kademe rozeti ekle (beta)";
    SetBadgesNote = "A separate window shows a tier badge and game count above each augment card. It does not show in recordings or screenshots.",
        "Oyun üzerindeki ayrı bir pencere, her kartın üstüne kademe rozeti ve maç sayısı ekler. Bu pencere ekran kayıtlarında ve ekran görüntülerinde görünmez.";
    SetResetOverlay = "Reset overlay position", "Panelin konumunu sıfırla";
    SetItemSets = "Send the build to the in-game shop", "Dizilimi oyun içi mağazaya gönder";
    SetLanguage = "Language", "Dil";
    SetAutostart = "Start with Windows in the background", "Windows açılınca arka planda başlat";
    // The size of the overlay: three steps.
    OverlaySize = "Overlay size", "Panel boyutu";
    SizeSmall = "Small", "Küçük";
    SizeMedium = "Medium", "Orta";
    SizeLarge = "Large", "Büyük";
    LangAuto = "Auto", "Otomatik";
    LangEnglish = "English", "English";
    LangTurkish = "Türkçe", "Türkçe";

    // ---- Settings: hotkeys
    HotOverlay = "Show or hide the overlay", "Paneli göster veya gizle";
    HotUnlock = "Unlock the overlay to move it", "Paneli taşımak için kilidini aç";
    HotScan = "Read the cards now", "Kartları şimdi oku";
    HotChange = "Change", "Değiştir";
    HotCancel = "Cancel", "İptal";
    HotPress = "Press a key", "Bir tuşa bas";
    HotNoteWaiting = "Press F1–F12, a letter, or a number. Press Esc to cancel.",
        "Yeni kısayol için F1–F12 tuşlarından birine, bir harfe veya rakama bas. İptal etmek için Esc.";
    HotTaken = "{0} is already assigned to the other hotkey", "{0} zaten diğer kısayol için kullanılıyor";
    HotSet = "Hotkey set to {0}", "Kısayol {0} olarak ayarlandı";

    // ---- Settings: card reading
    ReadOcrLanguage = "Reading language", "Okuma dili";
    ReadStartsInGame = "Shown in a game", "Oyunda belli olur";
    OcrLangEnUs = "English (United States)", "İngilizce (ABD)";
    OcrLangEnGb = "English (UK)", "İngilizce (Birleşik Krallık)";
    OcrLangEn = "English", "İngilizce";
    OcrLangTr = "Turkish", "Türkçe";
    ReadGameWindow = "Game window", "Oyun penceresi";
    ReadFound = "Found", "Bulundu";
    ReadNotFound = "Not found", "Bulunamadı";
    ReadNotInGame = "Not in a game", "Oyunda değil";
    ReadWindowMode = "League window mode", "League’in pencere modu";
    ModeBorderless = "Borderless", "Çerçevesiz";
    ModeWindowed = "Windowed", "Pencereli";
    ModeFullscreen = "Fullscreen", "Tam ekran";
    ModeUnknown = "Unknown", "Bilinmiyor";
    ReadChampSource = "Champion source", "Şampiyon kaynağı";
    SourceChampSelect = "Champ select", "Şampiyon seçimi";
    SourceLive = "In game", "Oyun içi";
    SourceUnknown = "Not available yet", "Henüz bilinmiyor";
    ReadCapture = "Screen capture", "Ekran görüntüsü";
    CaptureOk = "OK", "Sorunsuz";
    CaptureBlack = "Black", "Siyah";
    CaptureFrozen = "Frozen", "Dondu";
    CaptureBlackHint = "The game image is black. Switch League to Borderless.",
        "Oyun görüntüsü siyah geliyor. League’i çerçevesiz moda al.";
    CaptureFrozenHint = "The game image is frozen. Switch League to Borderless.",
        "Oyun görüntüsü değişmiyor. League’i çerçevesiz moda al.";
    CaptureLocked = "Locked", "Kilitli";
    CaptureRemote = "Remote desktop", "Uzak masaüstü";
    CaptureLockedHint = "The screen is locked. Hexsight reads again when you unlock it.",
        "Ekran kilitli. Kilidi açınca Hexsight yine okumaya başlar.";
    CaptureRemoteHint = "Remote desktop shows a black game image. Play on the PC itself.",
        "Uzak masaüstünde oyun görüntüsü siyah. Bilgisayarın başında oyna.";
    ReadLastScan = "Last scan", "Son tarama";
    ReadNoScan = "No scan yet", "Henüz tarama yok";
    AgeSeconds = "{0} s ago", "{0} sn önce";
    AgeMinutes = "{0} min ago", "{0} dk önce";
    AgeHours = "{0} h ago", "{0} sa önce";
    ReadLines = "Lines read", "Okunan satırlar";
    ReadLinesUnder = "{0} ({1} under the overlay)", "{0} ({1} satır panelin altında)";
    ReadEngineFailed = "Reading failed", "Okunamadı";
    ReadMatched = "Names matched", "Eşleşen adlar";
    ReadPickScreens = "Pick screens", "Seçim ekranları";
    ReadPickScreensValue = "Expected: {0} · completed: {1}", "Beklenen: {0} · tamamlanan: {1}";
    ReadInGameNote = "During a game, Hexsight keeps the previous scan while this window covers the game area.",
        "Bu pencere oyunun üstündeyken Hexsight önceki taramayı gösterir.";
    SaveScan = "Save the last scan", "Son taramayı kaydet";
    SaveScanNote = "Saves a picture and the text of the last scan to the data folder. Nothing is saved until you click.",
        "Son taramanın resmini ve metnini veri klasörüne kaydeder. Bu dosyalar yalnızca bu düğmeye basınca oluşur.";
    ScanSaved = "Last scan saved to the data folder", "Son tarama veri klasörüne kaydedildi";
    ScanSaveFailed = "Couldn't save scan: {0}", "Kaydedilemedi: {0}";
    ScanNone = "No scan available to save", "Kaydedilecek tarama yok";

    // ---- Settings: about
    AboutText = "Stats: aramkit.com (fallback: aramhex-data, CC BY-NC 4.0). Names and icons: CommunityDragon. Card reading uses Windows OCR on screen captures. Hexsight never reads or modifies game memory.",
        "İstatistikler: aramkit.com (yedek: aramhex-data, CC BY-NC 4.0). Adlar ve simgeler: CommunityDragon. \
         Hexsight kartları ekran görüntüsünden Windows OCR ile okur. Oyun belleğini okumaz ve değiştirmez.";
    AboutRiot = "Hexsight isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. \
                 Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc.",
        "Hexsight, Riot Games tarafından onaylanmamıştır ve Riot Games’in ya da Riot Games ürünlerinin yapımında veya yönetiminde resmen yer alan kişilerin görüşlerini yansıtmaz. \
         Riot Games ve ilgili tüm varlıklar, Riot Games, Inc. şirketinin ticari markaları veya tescilli ticari markalarıdır.";
    // Portable mode: the file Hexsight.portable sits next to the exe.
    AboutPortable = "Portable mode: data is in the Hexsight-data folder next to the app.",
        "Taşınabilir mod: veriler uygulamanın yanındaki Hexsight-data klasöründe.";
    AboutPortableFallback = "Portable mode: the app folder isn't writable, so data is in %LOCALAPPDATA%\\Hexsight.",
        "Taşınabilir mod: uygulama klasörüne yazılamadı, veriler %LOCALAPPDATA%\\Hexsight klasöründe.";
    // The button in About that really quits. The X of the window only hides it.
    QuitApp = "Quit Hexsight", "Hexsight’tan çık";
    // The update note and the feedback link.
    UpdateShort = "Update available", "Yeni sürüm var";
    UpdateAvailable = "Update available: {0}", "Yeni sürüm var: {0}";
    UpdateTip = "Opens the download page", "İndirme sayfasını açar";
    SetUpdateCheck = "Check for new versions", "Yeni sürüm var mı diye bak";
    SetUpdateCheckNote = "At most once a day, Hexsight asks GitHub for the number of the latest version. It sends nothing else.",
        "Hexsight en fazla günde bir kez GitHub’dan son sürümün numarasını sorar. Başka bir şey göndermez.";
    SendFeedback = "Send feedback", "Geri bildirim gönder";
    SendFeedbackNote = "Opens a new GitHub page. Your Hexsight version, Windows version and main settings fill in the text. It holds no Riot ID and no file paths.",
        "GitHub’da yeni bir sayfa açar. Hexsight sürümün, Windows sürümün ve ana ayarların metne yazılır. Riot kimliğin ve dosya yolların yazılmaz.";
    OpenDataFolder = "Open the data folder", "Veri klasörünü aç";
    OcrUnavailable = "Card reading isn't working. Add English (United States) to Windows: Settings > Time & language > Language & region.", "Kart okuma çalışmıyor. Windows’a İngilizce (ABD) dilini ekle: Ayarlar > Saat ve dil > Dil ve bölge.";

    // ---- Overlay
    OverlayDrag = "Drag to move · Ctrl+{0} to lock", "Taşımak için sürükle · Kilitle: Ctrl+{0}";
    OverlayWatching = "watching", "takipte";
    // The word after "Hexsight" in the pill, in a game with a known level. {0} is the level of the next pick.
    OverlayNextPick = "pick at level {0}", "{0}. seviyede seçim";
    OverlayPicksDone = "all picks done", "seçimler bitti";
    OverlayClose = "Click or press {0} to close", "Kapatmak için tıkla veya {0} tuşuna bas";

    // ---- Messages from the worker threads (toasts)
    // The toast fires after the swap with the bench worked, so it says so.
    SwapSent = "Swapped", "Şampiyon değiştirildi";
    SwapFailed = "Swap failed: {0}", "Değişim yapılamadı: {0}";
    TradeRequested = "Trade request sent", "Takas isteği gönderildi";
    TradeAccepted = "Trade accepted", "Takas kabul edildi";
    TradeDeclined = "Trade declined", "Takas reddedildi";
    TradeCancelled = "Trade request cancelled", "Takas isteği iptal edildi";
    TradeFailed = "Trade failed: {0}", "Takas yapılamadı: {0}";
    // The reason in a failed swap or trade, instead of the raw error of the client.
    ClientNotRunning = "League isn't running", "League açık değil";
    ClientRefused = "League didn't allow it", "League izin vermedi";
    ClientNoAnswer = "League didn't answer", "League yanıt vermedi";
    Reloading = "Downloading fresh data", "Veriler yeniden indiriliyor";
    // The toast when the X hides the window (the first time in a run).
    HiddenToast = "Hexsight keeps running. It comes back in champ select.",
        "Hexsight arka planda çalışıyor. Şampiyon seçiminde geri gelir.";
    // The note at the right of the build header, with a check mark, when the item sets are in the shop.
    ShopReady = "In shop", "Mağazada";
    ItemSetFailed = "Couldn't add item set: {0}", "Eşya seti eklenemedi: {0}";
    GameSaved = "Game saved to My games", "Maç, “Maçlarım” bölümüne kaydedildi";
    GamesSaveFailed = "Couldn't save your games: {0}", "Maçların kaydedilemedi: {0}";
    ScanRead = "Scanned {0} cards", "{0} kart okundu";
    ScanNoCards = "No augment cards detected", "Eklenti kartı bulunamadı";

    // ---- Errors
    ErrDownload = "Couldn't download {0}", "{0} indirilemedi";
    ErrInvalid = "Couldn't read {0}", "{0} okunamadı";
    ErrNames = "Couldn't download augment names ({0})", "eklenti adları indirilemedi ({0})";
    ErrBoth = "{0}; backup: {1}", "{0}; yedek: {1}";
    ErrLeagueFolder = "League not found. Open League.", "League bulunamadı, League’i aç";
    ErrDataLoading = "the data is still loading", "veriler hâlâ yükleniyor";
    ErrNoBuild = "no build data", "dizilim verisi yok";
    FileChampions = "champion list", "şampiyon listesi";
    FileAugmentSet = "augment set", "eklenti seti";
    FileAugmentList = "augment list", "eklenti listesi";
    FileItems = "item list", "eşya listesi";
    FileAramkitIndex = "aramkit patch list", "aramkit yama listesi";
    FileAramkitChampions = "aramkit champion list", "aramkit şampiyon listesi";
    FileAramhexIndex = "aramhex patch info", "aramhex yama bilgisi";
    FileAramhexChampions = "aramhex champion list", "aramhex şampiyon listesi";
    FileAramhexAugments = "aramhex augment table", "aramhex eklenti tablosu";

    // ---- Item set in the in-game shop
    ItemChampion = "Champion", "Şampiyon";
    ItemStart = "Start", "Başlangıç";
    ItemStartSkills = "{0} · Skills: {1}", "{0} · Yetenek sırası: {1}";
    ItemStartOther = "Start (other option)", "Başlangıç (diğer seçenek)";
    ItemCore = "Core build (in order)", "Çekirdek dizilim (sırayla)";
    ItemBoots = "Boots", "Botlar";
    ItemBootsSkip = "Boots: most players skip them", "Bot: çoğu oyuncu almıyor";
    ItemLater = "Later items", "Sonradan alınanlar";
    ItemSituational = "Situational", "Duruma göre";
    ItemConsumables = "Consumables", "Tüketilebilir eşyalar";

    // ---- Suggestions: the next items and the summary of the game (in a Mayhem game)
    SecNextItems = "Next items", "Sonraki eşyalar";
    SecThisGame = "This game", "Bu maç";
    SuggestStyle = "{0} build", "{0} dizilimi";
    SuggestNote = "From build data and Hexsight's own rules. Hexsight buys nothing.",
        "Öneriler dizilim verisinden ve Hexsight’ın kurallarından gelir. Hexsight eşya almaz.";
    SuggestWait = "Item tips show when the game data arrives.", "Eşya önerileri oyun verisi gelince görünür.";
    SuggestFull = "Your item slots are full.", "Eşya yuvaların dolu.";
    SuggestNoBuild = "No build data for this champion, so no item tips.", "Bu şampiyonun dizilim verisi yok, o yüzden eşya önerisi de yok.";
    SuggestNone = "No item tip right now.", "Şu an önerilecek eşya yok.";
    // The reason under an item. {0} is a number, or a share (70% in English, %70 in Turkish), or a name.
    ReasonCore = "core item {0}", "{0}. çekirdek eşya";
    ReasonBoots = "boots", "botlar";
    ReasonLater = "after the core", "çekirdekten sonra";
    ReasonSlot = "common item {0}", "sık alınan {0}. eşya";
    ReasonOtherBuild = "in other builds", "diğer dizilimlerde";
    ReasonVsAp = "vs {0} AP", "{0} AP’ye karşı";
    ReasonVsAd = "vs {0} AD", "{0} AD’ye karşı";
    ReasonAntiHeal = "vs {0} healers", "{0} iyileştirene karşı";
    ReasonVsTanks = "vs {0} tanks", "{0} tanka karşı";
    ReasonPairs = "pairs with {0}", "{0} ile uyumlu";
    // `ReasonPairs` when Hexsight has no name for the partner augment.
    ReasonPairsEarlier = "pairs with an earlier pick", "önceki seçiminle uyumlu";
    TipReasonData = "From build data.", "Dizilim verisinden.";
    TipReasonRule = "Hexsight's own rule, not statistics.", "Hexsight’ın kendi kuralı, istatistik değil.";
    TipPairsOffered = "This augment was offered to you. Hexsight can't know if you took it.", "Bu eklenti sana sunuldu. Seçip seçmediğini Hexsight bilemez.";
    TipGoldShort = "{0} gold short", "{0} altın eksik";
    TipGoldReady = "You have enough gold.", "Altının yeter.";
    CtxEnemyDamage = "Enemy damage", "Düşman hasarı";
    CtxMix = "{0} AP · {1} AD", "{0} AP · {1} AD";
    CtxFed = "Fed: {0} {1}", "Fed: {0} {1}";
    CtxHealers = "Healers", "İyileştirenler";
    CtxHealersN = "{0} healers", "{0} iyileştiren";
    CtxTanks = "Tanks", "Tanklar";
    CtxTip = "Estimated from the enemy champions and their items. Hexsight's own rule, not statistics.",
        "Düşman şampiyonlara ve eşyalarına bakılarak tahmin edilir. Hexsight’ın kendi kuralı, istatistik değil.";
    GroupSuggest = "Suggestions", "Öneriler";
    SetSuggestItems = "Suggest the next items", "Sonraki eşyaları öner";
    SetSuggestContext = "Show the game summary", "Maç özetini göster";
    SetSuggestNote = "Tips come from build data and Hexsight's own rules, never from win rates. You decide: Hexsight buys and picks nothing.",
        "Öneriler dizilim verisinden ve Hexsight’ın kendi kurallarından gelir, kazanma oranından gelmez. Karar senin: Hexsight eşya almaz, seçim yapmaz.";
    SetFitMarks = "Mark augments that fit your build", "Dizilimine uyan eklentileri işaretle";
    FitMark = "fits", "uyar";
    FitLine = "Fits your build: {0}", "Dizilimine uyar: {0}";
    FitTip = "This augment is tied to an item in your build. Hexsight's own rule, not statistics.", "Bu eklenti dizilimindeki bir eşyaya bağlı. Hexsight’ın kendi kuralı, istatistik değil.";

    // ---- Build styles (the archetypes of the data). Used in the title of an item set and on the build card.
    StyleAp = "AP", "AP";
    StyleAd = "AD", "AD";
    StyleTank = "Tank", "Tank";
    StyleBruiser = "Bruiser", "Dövüşçü";
    StyleCrit = "Crit", "Kritik vuruş";
    StyleLethality = "Lethality", "Zırh deşme";
    StyleOnHit = "On-hit", "İsabet etkisi";
    StyleUtility = "Utility", "Destek";
    StyleHybrid = "Hybrid", "Hibrit";
}

impl Lang {
    /// The text of a key.
    pub fn t(self, key: Key) -> &'static str {
        TEXTS[key as usize][self as usize]
    }

    /// The text of a key with its places `{0}`, `{1}` ... filled. A place with
    /// no argument stays as it is.
    pub fn fmt(self, key: Key, args: &[&dyn Display]) -> String {
        let mut rest = self.t(key);
        let mut out = String::with_capacity(rest.len() + 16);
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                out.push_str(&rest[open..]);
                return out;
            };
            match after[..close]
                .parse::<usize>()
                .ok()
                .and_then(|n| args.get(n))
            {
                Some(arg) => {
                    let _ = write!(out, "{arg}");
                }
                None => out.push_str(&rest[open..open + close + 2]),
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    /// A number with a fixed count of decimals. Turkish uses the comma: "67,2".
    pub fn num(self, v: f32, decimals: usize) -> String {
        let text = format!("{v:.decimals$}");
        if self == Lang::Tr {
            text.replace('.', ",")
        } else {
            text
        }
    }

    /// A share in percent (67.2 for 67.2 %): "67.2%" in English, "%67,2" in Turkish.
    pub fn pct(self, v: f32, decimals: usize) -> String {
        match self {
            Lang::En => format!("{}%", self.num(v, decimals)),
            Lang::Tr => format!("%{}", self.num(v, decimals)),
        }
    }

    /// A count of games with its noun: "1 game", "12 games" ("12 maç" in Turkish).
    /// `shown` is the count as text, for example "1.2k". `n` is the exact count
    /// that picks the form: only 1 takes the singular.
    pub fn games(self, n: u32, shown: &str) -> String {
        self.fmt(
            if n == 1 { Key::CapGame1 } else { Key::CapGames },
            &[&shown],
        )
    }

    /// "1 win", "3 wins" ("3 galibiyet" in Turkish).
    pub fn wins(self, n: u32) -> String {
        self.fmt(if n == 1 { Key::CapWin1 } else { Key::CapWins }, &[&n])
    }

    /// The totals line of My games: "62 games · 37W 25L" ("62 maç · 37G 25M" in
    /// Turkish). The short score keeps the line on one row at 300 points.
    pub fn totals(self, games: u32, wins: u32, losses: u32) -> String {
        format!(
            "{} · {}",
            self.games(games, &games.to_string()),
            self.fmt(Key::GamesLast10Score, &[&wins, &losses])
        )
    }

    /// The short name of a month, 1 to 12: "Oct" ("Eki" in Turkish). An empty text for a number out of range.
    pub fn month(self, month: u32) -> &'static str {
        const MONTHS: [Key; 12] = [
            Key::MonthJan,
            Key::MonthFeb,
            Key::MonthMar,
            Key::MonthApr,
            Key::MonthMay,
            Key::MonthJun,
            Key::MonthJul,
            Key::MonthAug,
            Key::MonthSep,
            Key::MonthOct,
            Key::MonthNov,
            Key::MonthDec,
        ];
        month
            .checked_sub(1)
            .and_then(|i| MONTHS.get(i as usize))
            .map_or("", |k| self.t(*k))
    }

    /// Capital letters. Turkish keeps the dot: "i" becomes "İ" and "ı" becomes "I".
    pub fn upper(self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match (self, c) {
                (Lang::Tr, 'i') => out.push('İ'),
                (Lang::Tr, 'ı') => out.push('I'),
                _ => out.extend(c.to_uppercase()),
            }
        }
        out
    }

    /// The name of a build style for the key of the data ("tank", "on hit", "on_hit").
    /// A key that Hexsight does not know shows as written, with a capital first letter.
    pub fn style(self, key: &str) -> String {
        let key = key.trim();
        let known = match key.to_ascii_lowercase().replace('_', " ").as_str() {
            "ap" => Some(Key::StyleAp),
            "ad" => Some(Key::StyleAd),
            "tank" => Some(Key::StyleTank),
            "bruiser" => Some(Key::StyleBruiser),
            "crit" => Some(Key::StyleCrit),
            "lethality" => Some(Key::StyleLethality),
            "on hit" => Some(Key::StyleOnHit),
            "utility" => Some(Key::StyleUtility),
            "hybrid" => Some(Key::StyleHybrid),
            _ => None,
        };
        if let Some(k) = known {
            return self.t(k).to_string();
        }
        let mut chars = key.chars();
        match chars.next() {
            Some(first) => self.upper(&first.to_string()) + chars.as_str(),
            None => String::new(),
        }
    }

    /// The name of the language of the OCR engine for its tag: "en-US" reads
    /// "English (United States)". Another region of a known language keeps its tag
    /// ("English (en-AU)"). A language that Hexsight does not know shows its tag.
    pub fn language_name(self, tag: &str) -> String {
        let tag = tag.trim();
        let lower = tag.to_ascii_lowercase().replace('_', "-");
        let (base, region) = lower.split_once('-').unwrap_or((lower.as_str(), ""));
        let exact = match (base, region) {
            ("en", "us") => Some(Key::OcrLangEnUs),
            ("en", "gb") => Some(Key::OcrLangEnGb),
            ("en", "") => Some(Key::OcrLangEn),
            ("tr", "" | "tr") => Some(Key::OcrLangTr),
            _ => None,
        };
        if let Some(k) = exact {
            return self.t(k).to_string();
        }
        match base {
            "en" => format!("{} ({tag})", self.t(Key::OcrLangEn)),
            "tr" => format!("{} ({tag})", self.t(Key::OcrLangTr)),
            _ => tag.to_string(),
        }
    }

    /// The reason of a failed call to the client, for a message such as "Swap
    /// failed: {0}". The raw error (`raw`, for example "http status: 404") is
    /// English and technical, so the message names only the kind of failure.
    pub fn client_error(self, raw: &str) -> &'static str {
        self.t(match crate::lcu::failure_of(raw) {
            CallFailure::NotConnected => Key::ClientNotRunning,
            CallFailure::Refused => Key::ClientRefused,
            CallFailure::NoAnswer => Key::ClientNoAnswer,
        })
    }

    /// The text of a failed data load, for the footer.
    pub fn load_error(self, e: &LoadError) -> String {
        match e {
            LoadError::Download(file) => self.fmt(Key::ErrDownload, &[&self.t(*file)]),
            LoadError::Invalid(file) => self.fmt(Key::ErrInvalid, &[&self.t(*file)]),
            LoadError::Names(folder) => self.fmt(Key::ErrNames, &[folder]),
            LoadError::Both(main, fallback) => self.fmt(
                Key::ErrBoth,
                &[&self.load_error(main), &self.load_error(fallback)],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers inside the braces of a text, in order of appearance.
    fn places(text: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let close = after.find('}').expect("a place needs a closing brace");
            out.push(after[..close].parse().expect("a place holds a number"));
            rest = &after[close + 1..];
        }
        out
    }

    #[test]
    fn every_key_has_a_text_in_both_languages() {
        assert_eq!(KEYS.len(), TEXTS.len());
        assert!(KEYS.len() > 150, "the table lost entries: {}", KEYS.len());
        for (i, key) in KEYS.iter().enumerate() {
            // The key is the index of its row.
            assert_eq!(*key as usize, i, "{key:?}");
            for lang in [Lang::En, Lang::Tr] {
                assert!(
                    !lang.t(*key).trim().is_empty(),
                    "{key:?} has no text in {lang:?}"
                );
            }
        }
    }

    #[test]
    fn both_languages_fill_the_same_places() {
        for key in KEYS {
            let (en, tr) = (places(Lang::En.t(*key)), places(Lang::Tr.t(*key)));
            let (mut a, mut b) = (en.clone(), tr.clone());
            // A place may show twice in a text ("F8 ... Ctrl+F8"), so equal numbers collapse.
            a.sort_unstable();
            a.dedup();
            b.sort_unstable();
            b.dedup();
            assert_eq!(a, b, "{key:?}: English {en:?}, Turkish {tr:?}");
            // Places count from 0 without a gap.
            assert!(a.iter().enumerate().all(|(i, n)| *n == i), "{key:?}: {a:?}");
        }
    }

    #[test]
    fn a_text_is_translated_unless_it_is_a_name() {
        // The names of the two languages read the same in both.
        // The styles AP, AD and Tank are the same word in both languages, and so is the mix "70% AP · 30% AD".
        // "Fed" is the word Turkish players use too (the owner's choice).
        // March and May have the same short name in both languages ("Mar", "May").
        let same = [
            Key::LangEnglish,
            Key::LangTurkish,
            Key::StyleAp,
            Key::StyleAd,
            Key::StyleTank,
            Key::CtxMix,
            Key::CtxFed,
            Key::MonthMar,
            Key::MonthMay,
        ];
        for key in KEYS.iter().filter(|k| !same.contains(k)) {
            let (en, tr) = (Lang::En.t(*key), Lang::Tr.t(*key));
            assert_ne!(en, tr, "{key:?} is the same in both languages");
        }
    }

    #[test]
    fn build_styles_have_names_in_both_languages() {
        let all = [
            ("ap", "AP", "AP"),
            ("ad", "AD", "AD"),
            ("tank", "Tank", "Tank"),
            ("bruiser", "Bruiser", "Dövüşçü"),
            ("crit", "Crit", "Kritik vuruş"),
            ("lethality", "Lethality", "Zırh deşme"),
            ("on hit", "On-hit", "İsabet etkisi"),
            ("utility", "Utility", "Destek"),
            ("hybrid", "Hybrid", "Hibrit"),
        ];
        for (key, en, tr) in all {
            assert_eq!(Lang::En.style(key), en, "{key}");
            assert_eq!(Lang::Tr.style(key), tr, "{key}");
        }
        // The key of the data keeps its underscore in some places. Case and spaces do not matter.
        assert_eq!(Lang::En.style("on_hit"), "On-hit");
        assert_eq!(Lang::Tr.style(" Tank "), "Tank");
        assert_eq!(Lang::Tr.style("AP"), "AP");
    }

    #[test]
    fn an_unknown_style_shows_as_written() {
        assert_eq!(Lang::En.style("mage"), "Mage");
        assert_eq!(Lang::Tr.style("mage"), "Mage");
        assert_eq!(Lang::En.style("true damage"), "True damage");
        // The first letter takes the rule of the language. The rest stays as it is.
        assert_eq!(Lang::Tr.style("iyileştirici"), "İyileştirici");
        assert_eq!(Lang::En.style("sKirmisher"), "SKirmisher");
        assert_eq!(Lang::En.style(""), "");
    }

    #[test]
    fn auto_follows_the_language_of_the_client() {
        assert_eq!(LangPref::Auto.resolve("tr_TR"), Lang::Tr);
        assert_eq!(LangPref::Auto.resolve("en_US"), Lang::En);
        // A locale in other letters, and a locale that is not Turkish.
        assert_eq!(LangPref::Auto.resolve("TR_tr"), Lang::Tr);
        assert_eq!(LangPref::Auto.resolve("de_DE"), Lang::En);
        assert_eq!(LangPref::Auto.resolve("ja_JP"), Lang::En);
        // No locale yet (the client did not answer, nothing saved): English.
        assert_eq!(LangPref::Auto.resolve(""), Lang::En);
    }

    #[test]
    fn a_fixed_language_ignores_the_client() {
        for locale in ["tr_TR", "en_US", ""] {
            assert_eq!(LangPref::English.resolve(locale), Lang::En);
            assert_eq!(LangPref::Turkish.resolve(locale), Lang::Tr);
        }
        assert_eq!(LangPref::default(), LangPref::Auto);
        assert_eq!(Lang::default(), Lang::En);
    }

    #[test]
    fn the_setting_is_saved_as_a_short_code() {
        let text = |p: LangPref| serde_json::to_string(&p).unwrap();
        assert_eq!(text(LangPref::Auto), "\"auto\"");
        assert_eq!(text(LangPref::English), "\"en\"");
        assert_eq!(text(LangPref::Turkish), "\"tr\"");
        for p in [LangPref::Auto, LangPref::English, LangPref::Turkish] {
            assert_eq!(serde_json::from_str::<LangPref>(&text(p)).unwrap(), p);
        }
        // A word that Hexsight does not know (a hand-edited file) means Auto.
        assert_eq!(
            serde_json::from_str::<LangPref>("\"fr\"").unwrap(),
            LangPref::Auto
        );
    }

    #[test]
    fn places_get_their_values() {
        assert_eq!(Lang::En.fmt(Key::ScanRead, &[&3]), "Scanned 3 cards");
        assert_eq!(Lang::Tr.fmt(Key::ScanRead, &[&3]), "3 kart okundu");
        // The order of the places can differ between the languages.
        assert_eq!(
            Lang::En.fmt(Key::PairsLine, &[&"Kral"]),
            "Pairs well with Kral if picked earlier"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::PairsLine, &[&"Kral"]),
            "Önceden seçtiysen Kral ile iyi gider"
        );
        // A place with no argument stays visible, and a brace that is no place stays too.
        assert_eq!(
            Lang::En.fmt(Key::TradeGetGive, &[&"Ahri"]),
            "You get Ahri; they get {1}"
        );
    }

    #[test]
    fn one_game_one_win_take_the_singular() {
        assert_eq!(Lang::En.games(1, "1"), "1 game");
        assert_eq!(Lang::En.games(0, "0"), "0 games");
        assert_eq!(Lang::En.games(2, "2"), "2 games");
        assert_eq!(Lang::En.games(12_000, "12k"), "12k games");
        assert_eq!(Lang::En.wins(1), "1 win");
        assert_eq!(Lang::En.wins(3), "3 wins");
        // The totals line ends with the short score, so it stays on one row at 300 points.
        assert_eq!(Lang::En.totals(1, 1, 0), "1 game · 1W 0L");
        assert_eq!(Lang::En.totals(3, 1, 2), "3 games · 1W 2L");
        assert_eq!(Lang::En.totals(62, 37, 25), "62 games · 37W 25L");
        // Turkish keeps the noun in one form after a number.
        assert_eq!(Lang::Tr.games(1, "1"), "1 maç");
        assert_eq!(Lang::Tr.games(7, "7"), "7 maç");
        assert_eq!(Lang::Tr.wins(1), "1 galibiyet");
        assert_eq!(Lang::Tr.totals(1, 1, 0), "1 maç · 1G 0M");
        assert_eq!(Lang::Tr.totals(62, 37, 25), "62 maç · 37G 25M");
        // No text of the table has a plural word that follows the number 1.
        for lang in [Lang::En, Lang::Tr] {
            assert_ne!(lang.games(1, "1"), "1 games");
        }
    }

    #[test]
    fn the_months_have_short_names_in_both_languages() {
        let en = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let tr = [
            "Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara",
        ];
        for m in 1..=12 {
            assert_eq!(Lang::En.month(m), en[m as usize - 1], "{m}");
            assert_eq!(Lang::Tr.month(m), tr[m as usize - 1], "{m}");
        }
        // A number out of range gives no text, not a wrong month.
        for lang in [Lang::En, Lang::Tr] {
            assert_eq!((lang.month(0), lang.month(13)), ("", ""));
        }
    }

    #[test]
    fn a_game_date_puts_the_day_and_the_month_in_the_order_of_the_language() {
        assert_eq!(
            Lang::En.fmt(Key::GameDate, &[&26, &Lang::En.month(10)]),
            "Oct 26"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::GameDate, &[&26, &Lang::Tr.month(10)]),
            "26 Eki"
        );
        assert_eq!(
            Lang::En.fmt(Key::GameDateYear, &[&26, &Lang::En.month(10), &2025]),
            "Oct 26, 2025"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::GameDateYear, &[&26, &Lang::Tr.month(10), &2025]),
            "26 Eki 2025"
        );
        assert_eq!(Lang::En.fmt(Key::GameToday, &[&"15:31"]), "Today 15:31");
        assert_eq!(Lang::Tr.fmt(Key::GameToday, &[&"15:31"]), "Bugün 15:31");
        assert_eq!(
            Lang::En.fmt(Key::GameYesterday, &[&"21:04"]),
            "Yesterday 21:04"
        );
        assert_eq!(Lang::Tr.fmt(Key::GameYesterday, &[&"21:04"]), "Dün 21:04");
        // The result letters match the letters of the "Last 10" score.
        assert!(
            Lang::En.fmt(Key::GamesLast10Score, &[&1, &2])
                == format!(
                    "1{} 2{}",
                    Lang::En.t(Key::GameWinShort),
                    Lang::En.t(Key::GameLossShort)
                )
        );
        assert!(
            Lang::Tr.fmt(Key::GamesLast10Score, &[&1, &2])
                == format!(
                    "1{} 2{}",
                    Lang::Tr.t(Key::GameWinShort),
                    Lang::Tr.t(Key::GameLossShort)
                )
        );
    }

    #[test]
    fn numbers_follow_the_language() {
        assert_eq!(Lang::En.pct(67.24, 1), "67.2%");
        assert_eq!(Lang::Tr.pct(67.24, 1), "%67,2");
        assert_eq!(Lang::En.pct(75.0, 0), "75%");
        assert_eq!(Lang::Tr.pct(75.0, 0), "%75");
        assert_eq!(Lang::En.num(5.0, 2), "5.00");
        assert_eq!(Lang::Tr.num(5.0, 2), "5,00");
    }

    #[test]
    fn the_riot_notice_uses_the_template_text() {
        assert_eq!(
            Lang::En.t(Key::AboutRiot),
            "Hexsight isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. \
             Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc."
        );
        assert!(
            Lang::Tr
                .t(Key::AboutRiot)
                .starts_with("Hexsight, Riot Games tarafından onaylanmamıştır")
        );
    }

    #[test]
    fn no_text_names_an_augment_win_rate() {
        // Riot bars augment win rates in any form. No text of the table offers or explains one.
        for key in KEYS {
            for lang in [Lang::En, Lang::Tr] {
                let text = lang.t(*key).to_lowercase();
                assert!(
                    !text.contains("augment win rate") && !text.contains("eklenti kazanma"),
                    "{lang:?} {key:?}: {text}"
                );
            }
        }
    }

    #[test]
    fn the_suggestion_texts_say_who_decides() {
        // The notes name Hexsight as the one that buys and picks nothing, in both languages.
        for lang in [Lang::En, Lang::Tr] {
            for key in [Key::SuggestNote, Key::SetSuggestNote] {
                assert!(lang.t(key).contains("Hexsight"), "{lang:?} {key:?}");
            }
        }
        assert!(
            Lang::En.t(Key::SuggestNote).contains("buys nothing")
                && Lang::En
                    .t(Key::SetSuggestNote)
                    .contains("buys and picks nothing")
        );
        assert!(
            Lang::Tr.t(Key::SuggestNote).contains("eşya almaz")
                && Lang::Tr.t(Key::SetSuggestNote).contains("seçim yapmaz")
        );
        // A reason that comes from a rule of Hexsight is not statistics, and the tooltip says so in both languages.
        for key in [Key::TipReasonRule, Key::CtxTip, Key::FitTip] {
            assert!(
                Lang::En.t(key).contains("not statistics")
                    && Lang::Tr.t(key).contains("istatistik değil"),
                "{key:?}"
            );
        }
        // No suggestion text tells the player what to do: no "buy", "take" or "pick" as a command.
        for key in [
            Key::SuggestNote,
            Key::SuggestWait,
            Key::SuggestFull,
            Key::SuggestNone,
            Key::ReasonCore,
            Key::ReasonPairs,
            Key::FitLine,
        ] {
            let text = Lang::En.t(key).to_lowercase();
            assert!(
                !text.starts_with("buy") && !text.starts_with("take") && !text.starts_with("pick"),
                "{key:?}: {text}"
            );
        }
        // A share reads with the sign of the language: "vs 70% AP" and "%70 AP’ye karşı".
        assert_eq!(
            Lang::En.fmt(Key::ReasonVsAp, &[&Lang::En.pct(70.0, 0)]),
            "vs 70% AP"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::ReasonVsAp, &[&Lang::Tr.pct(70.0, 0)]),
            "%70 AP’ye karşı"
        );
        assert_eq!(Lang::Tr.fmt(Key::ReasonSlot, &[&2]), "sık alınan 2. eşya");
        assert_eq!(Lang::En.fmt(Key::ReasonSlot, &[&2]), "common item 2");
        // A Turkish ordinal takes a dot and the noun after it, never a "#".
        assert_eq!(Lang::Tr.fmt(Key::ReasonCore, &[&1]), "1. çekirdek eşya");
        assert_eq!(Lang::En.fmt(Key::ReasonCore, &[&1]), "core item 1");
    }

    #[test]
    fn a_place_that_shows_twice_gets_the_same_value_twice() {
        // The first-run card names the overlay key twice: to show or hide the overlay, and to move it.
        assert_eq!(
            Lang::En.fmt(Key::HeroStartGame, &[&"F8"]),
            "Start an ARAM: Mayhem game. In game, F8 shows or hides the overlay, and Ctrl+F8 unlocks it so you can drag it."
        );
        assert_eq!(
            Lang::Tr.fmt(Key::HeroStartGame, &[&"F8"]),
            "Bir Şamata maçına gir. Oyunda F8 ile paneli açıp kapatırsın, Ctrl+F8 ile kilidini açıp taşırsın."
        );
        // A repeated place with no argument stays visible both times.
        assert_eq!(
            Lang::En.fmt(Key::HeroStartGame, &[]).matches("{0}").count(),
            2
        );
    }

    #[test]
    fn the_bench_swap_and_the_teammate_trade_keep_their_own_words() {
        // `best_swap_parts` splits the tooltip at {0} and at {1}, so {0} comes first in both languages.
        for lang in [Lang::En, Lang::Tr] {
            let text = lang.t(Key::BenchBestSwap);
            assert!(
                text.find("{0}").unwrap() < text.find("{1}").unwrap(),
                "{lang:?}: {text}"
            );
        }
        assert_eq!(
            Lang::En.fmt(Key::BenchBestSwap, &[&"Yasuo (S)", &"Garen (C)"]),
            "Best swap: Yasuo (S) · yours is Garen (C)"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::BenchBestSwap, &[&"Yasuo (S)", &"Garen (C)"]),
            "En iyi değişim: Yasuo (S) · seninki Garen (C)"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::BenchSwapButton, &[&"Yasuo (S)"]),
            "Yasuo (S) ile değiştir"
        );
        assert_eq!(
            Lang::En.fmt(Key::BenchSwapButton, &[&"Yasuo (S)"]),
            "Swap to Yasuo (S)"
        );
        // A swap with the bench is a "değişim"; "takas" belongs to the trade with a teammate.
        for key in [
            Key::BenchBestSwap,
            Key::BenchSwapButton,
            Key::BenchClickToSwap,
            Key::SwapSent,
            Key::SwapFailed,
        ] {
            assert!(
                !Lang::Tr.t(key).to_lowercase().contains("takas"),
                "{key:?}: {}",
                Lang::Tr.t(key)
            );
        }
        assert_eq!(Lang::Tr.t(Key::SwapSent), "Şampiyon değiştirildi");
        // The trade line says who gets whom without an accusative that a template cannot add.
        assert_eq!(
            Lang::Tr.fmt(Key::TradeGetGive, &[&"Leona", &"Caitlyn"]),
            "Leona sana gelir, Caitlyn takım arkadaşına gider."
        );
        assert_eq!(
            Lang::En.fmt(Key::TradeGetGive, &[&"Leona", &"Caitlyn"]),
            "You get Leona; they get Caitlyn"
        );
    }

    #[test]
    fn the_new_short_texts_read_in_both_languages() {
        assert_eq!(Lang::En.fmt(Key::NextPickAt, &[&7]), "Next pick: level 7");
        assert_eq!(
            Lang::Tr.fmt(Key::NextPickAt, &[&7]),
            "Sonraki seçim: 7. seviye"
        );
        assert_eq!(Lang::En.fmt(Key::OverlayNextPick, &[&7]), "pick at level 7");
        assert_eq!(
            Lang::Tr.fmt(Key::OverlayNextPick, &[&7]),
            "7. seviyede seçim"
        );
        assert_eq!(Lang::En.fmt(Key::GamesLast10Score, &[&7, &3]), "7W 3L");
        assert_eq!(Lang::Tr.fmt(Key::GamesLast10Score, &[&7, &3]), "7G 3M");
        assert_eq!(Lang::Tr.fmt(Key::FooterPatch, &[&"16.21"]), "16.21 yaması");
        assert_eq!(Lang::En.fmt(Key::FooterPatch, &[&"16.21"]), "Patch 16.21");
        // Only a low sample gets a word, and the word says what is missing, not that the card is weak.
        assert_eq!(
            (Lang::En.t(Key::ConfLow), Lang::Tr.t(Key::ConfLow)),
            ("limited", "az veri")
        );
        assert_eq!(
            (Lang::En.t(Key::PairMark), Lang::Tr.t(Key::PairMark)),
            ("pairs", "eşleşir")
        );
        // The names of the three overlay sizes are one word each.
        for key in [Key::SizeSmall, Key::SizeMedium, Key::SizeLarge] {
            assert!(
                !Lang::En.t(key).contains(' ') && !Lang::Tr.t(key).contains(' '),
                "{key:?}"
            );
        }
        // The window stays alive after the X, and the texts say so.
        assert!(
            Lang::En.t(Key::TipClose).contains("keeps running")
                && Lang::Tr.t(Key::TipClose).contains("devam eder")
        );
        assert!(
            Lang::En.t(Key::HiddenToast).contains("keeps running")
                && Lang::Tr.t(Key::HiddenToast).contains("çalışıyor")
        );
    }

    #[test]
    fn turkish_texts_use_the_typographic_apostrophe() {
        for key in KEYS {
            assert!(
                !Lang::Tr.t(*key).contains('\''),
                "{key:?}: {}",
                Lang::Tr.t(*key)
            );
        }
    }

    #[test]
    fn turkish_capitals_keep_the_dot() {
        assert_eq!(Lang::Tr.upper("Şampiyon seçimi"), "ŞAMPİYON SEÇİMİ");
        assert_eq!(Lang::Tr.upper("Prizmatik"), "PRİZMATİK");
        assert_eq!(Lang::Tr.upper("ışık"), "IŞIK");
        assert_eq!(Lang::Tr.upper("Gümüş · Altın"), "GÜMÜŞ · ALTIN");
        // English is not touched by the Turkish rule.
        assert_eq!(Lang::En.upper("Prismatic"), "PRISMATIC");
        assert_eq!(Lang::En.upper("in game"), "IN GAME");
    }

    #[test]
    fn a_failed_swap_names_the_reason_in_the_language_and_never_the_raw_error() {
        let raw = [
            "http status: 404",
            crate::lcu::NOT_CONNECTED,
            "timeout: global",
        ];
        let en: Vec<String> = raw
            .iter()
            .map(|r| Lang::En.fmt(Key::SwapFailed, &[&Lang::En.client_error(r)]))
            .collect();
        assert_eq!(
            en,
            [
                "Swap failed: League didn't allow it",
                "Swap failed: League isn't running",
                "Swap failed: League didn't answer"
            ]
        );
        assert_eq!(
            Lang::Tr.fmt(Key::SwapFailed, &[&Lang::Tr.client_error(raw[0])]),
            "Değişim yapılamadı: League izin vermedi"
        );
        assert_eq!(
            Lang::Tr.fmt(Key::TradeFailed, &[&Lang::Tr.client_error(raw[1])]),
            "Takas yapılamadı: League açık değil"
        );
        for lang in [Lang::En, Lang::Tr] {
            for r in raw {
                let text = lang.client_error(r);
                assert!(
                    !text.contains("http") && !text.contains("404") && !text.contains("client"),
                    "{lang:?} {r}: {text}"
                );
            }
        }
    }

    #[test]
    fn the_reading_language_shows_a_name_and_never_a_bare_known_tag() {
        assert_eq!(Lang::En.language_name("en-US"), "English (United States)");
        assert_eq!(Lang::Tr.language_name("en-US"), "İngilizce (ABD)");
        assert_eq!(Lang::En.language_name("en-GB"), "English (UK)");
        assert_eq!(
            Lang::Tr.language_name("en-GB"),
            "İngilizce (Birleşik Krallık)"
        );
        assert_eq!(Lang::En.language_name("tr-TR"), "Turkish");
        assert_eq!(Lang::Tr.language_name("tr-TR"), "Türkçe");
        assert_eq!(Lang::Tr.language_name("en"), "İngilizce");
        // Case, an underscore and spaces do not matter.
        assert_eq!(Lang::En.language_name(" EN_us "), "English (United States)");
        // Another region keeps its tag. An unknown language shows the tag as written.
        assert_eq!(Lang::En.language_name("en-AU"), "English (en-AU)");
        assert_eq!(Lang::Tr.language_name("tr-CY"), "Türkçe (tr-CY)");
        assert_eq!(Lang::Tr.language_name("de-DE"), "de-DE");
        assert_eq!(Lang::En.language_name("zh-Hans-CN"), "zh-Hans-CN");
    }

    #[test]
    fn a_failed_load_reads_in_both_languages() {
        let main = LoadError::Download(Key::FileAramkitIndex);
        let both = LoadError::Both(
            Box::new(main.clone()),
            Box::new(LoadError::Invalid(Key::FileAramhexIndex)),
        );
        assert_eq!(
            Lang::En.load_error(&main),
            "Couldn't download aramkit patch list"
        );
        assert_eq!(
            Lang::En.load_error(&both),
            "Couldn't download aramkit patch list; backup: Couldn't read aramhex patch info"
        );
        assert_eq!(
            Lang::Tr.load_error(&main),
            "aramkit yama listesi indirilemedi"
        );
        assert_eq!(
            Lang::Tr.load_error(&both),
            "aramkit yama listesi indirilemedi; yedek: aramhex yama bilgisi okunamadı"
        );
        assert_eq!(
            Lang::En.load_error(&LoadError::Names("tr_tr".into())),
            "Couldn't download augment names (tr_tr)"
        );
        assert_eq!(
            Lang::Tr.load_error(&LoadError::Names("tr_tr".into())),
            "eklenti adları indirilemedi (tr_tr)"
        );
    }
}
