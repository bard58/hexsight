# Privacy

English | [Türkçe](#türkçe)

Version: Hexsight 0.1.0. Last updated: 2026-10-25.

**Short version.** Hexsight keeps your data on your PC. The developer collects nothing. There is no account, no telemetry, no crash report, and no ads.

## 1. Who is behind Hexsight

Hexsight is a free, open source project. The maintainer is listed on the GitHub page of this repository. Contact: drops.grizzly_67@icloud.com. The developer does not collect, receive, or store your data.

## 2. What Hexsight reads on your PC

- **The League Client API on your PC (127.0.0.1).** Hexsight gets the port and the password from the `lockfile` in your League folder. It keeps them in memory and uses them only for local calls.
- **The Live Client Data API on your PC (127.0.0.1, port 2999).** In a game, it gives your champion, level, game time, and death state. For the item tips and the enemy summary, Hexsight also reads the champion, items, and score of all ten players. It uses the names only to find you and does not keep them.
- **Your match history from the League Client.** Hexsight reads your player ID and your finished ARAM Mayhem games. It keeps only your own record of a game: champion, augments, result, kills, deaths, assists. It does not keep the other nine players.
- **One value in `Config\game.cfg` in the League folder:** the window mode. Hexsight does not change this file.
- **A capture of the game area of your screen,** for card reading. See section 4.

## 3. What Hexsight stores on your PC

| What | Where | How to delete |
|---|---|---|
| Settings, data cache | `%LOCALAPPDATA%\Hexsight\settings.json` and `%LOCALAPPDATA%\Hexsight\cache` | Delete the folder `%LOCALAPPDATA%\Hexsight` |
| Your saved games (500 at most) | `%LOCALAPPDATA%\Hexsight\history.json` | Delete the file |
| The last scan, only when you press "Save the last scan" | `%LOCALAPPDATA%\Hexsight\last-scan.png` and `last-scan.txt` | Delete the files |
| A crash note, only when Hexsight crashes | `%LOCALAPPDATA%\Hexsight\crash.log`: one line for each crash with the time, the Hexsight version, the thread, the Windows build, and the error text. It stays on your PC | Delete the file |
| A data log of failed downloads | `%LOCALAPPDATA%\Hexsight\hexsight.log` and `hexsight.log.1`: one line for each data file that did not download or did not parse, with the time, the host and path of the URL, the reason (an HTTP status or an error kind), and the bytes that came. It holds no Riot ID, no player ID, and no folder path of your PC. Each file holds 256 KB at most. It stays on your PC | Delete the files |
| Portable mode: the settings, cache, saved games, last scan, crash note, and data log above | `<Hexsight folder>\Hexsight-data`, only when the file `Hexsight.portable` is next to `Hexsight.exe` (the portable zip). If Hexsight cannot write there, it uses `%LOCALAPPDATA%\Hexsight` | Delete the folder `Hexsight-data` |
| Item sets for the in-game shop | `<League folder>\Config\Global\Recommended\Hexsight-0.json`, `Hexsight-1.json`, and so on | Turn off "Send the build to the in-game shop" in Settings, or delete the files |
| A start-with-Windows entry, only when you turn on "Start with Windows in the background" | The registry value `Hexsight` in `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` | Turn the setting off, or delete the value |

The full uninstall steps are in the README.

## 4. The screen capture

Hexsight captures one part of the game window to read the augment cards. It keeps one image at most in memory. Each new scan replaces it. Hexsight frees it 10 minutes after the game ends. It writes the image to disk only when you press "Save the last scan". It never sends the image anywhere. A saved scan can show a part of your game screen, so check it before you share it.

## 5. What Hexsight sends over the internet

Hexsight sends plain GET requests for public data files. It uses these hosts:

- `data.aramkit.com`: augment stats, builds, and pairs.
- `raw.githubusercontent.com`: fallback stats (aramhex-data).
- `raw.communitydragon.org`: names and icons.
- `api.github.com`: at most once a day, the number of the newest Hexsight release. The request has no data about you.

Each request has the user agent `Hexsight/0.1` with the address of this project. The path of a request shows your client language and the champion IDs that you open. The hosts also see your IP address, as every web server does. Each host keeps its own logs under its own privacy policy. Hexsight does not control them.

## 6. What Hexsight never sends

Your games, your player ID, your screen images, your key presses, and your account data never leave your PC. The League Client password stays in memory and goes only to 127.0.0.1.

## 7. No account, no telemetry, no ads

Hexsight has no account system, no analytics, no crash reporting, and no ads. It has no server of its own.

## 8. Your rights

The developer holds no data about you, so there is nothing to look up, correct, or erase on a server. You control all the data on your PC. Delete the files in section 3 to remove it. If you live in the EU or the UK, the GDPR gives you rights over personal data held by others. If you live in Turkey, the KVKK (Law No. 6698) does. You can contact the data protection authority of your country, for example the KVKK in Turkey (kvkk.gov.tr).

## 9. Children

Hexsight is not aimed at children. The League client sets its own age limits.

## 10. Changes

We show the date of each change at the top of this file. If Hexsight ever adds data collection, we will say so in the release notes before the release, and we will ask for your consent where the law requires it.

---

## Türkçe

Sürüm: Hexsight 0.1.0. Son güncelleme: 2026-10-25.

**Kısaca.** Hexsight verini bilgisayarında tutar. Geliştirici hiçbir şey toplamaz. Hesap, telemetri, çökme raporu ve reklam yok.

### 1. Hexsight'ın arkasında kim var?

Hexsight ücretsiz ve açık kaynaklı bir proje. Geliştiriciyi bu deponun GitHub sayfasında bulabilirsin. İletişim: drops.grizzly_67@icloud.com. Geliştirici verini toplamaz, almaz, saklamaz.

### 2. Hexsight bilgisayarında neyi okur?

- **Bilgisayarındaki League Client API'si (127.0.0.1).** Port ve parolayı League klasöründeki `lockfile` dosyasından alır. İkisini bellekte tutar ve yalnızca yerel çağrılarda kullanır.
- **Bilgisayarındaki Live Client Data API'si (127.0.0.1, port 2999).** Oyundayken şampiyonunu, seviyeni, oyun süresini ve ölü olup olmadığını verir. Eşya önerileri ve karşı takımın özeti için Hexsight on oyuncunun şampiyonunu, eşyalarını ve skorunu da okur. Oyuncu adlarını yalnızca seni bulmak için kullanır, saklamaz.
- **League Client'taki maç geçmişin.** Oyuncu kimliğini ve biten ARAM: Şamata maçlarını okur. Maçla ilgili yalnızca senin kaydını tutar: şampiyon, eklentiler, sonuç, öldürme, ölüm, asist. Diğer dokuz oyuncunun kaydını tutmaz.
- **League klasöründeki `Config\game.cfg` dosyasından tek bir değer:** pencere modu. Hexsight bu dosyayı değiştirmez.
- **Oyun ekranının bir bölümü,** eklenti kartlarını okumak için. 4. bölüme bak.

### 3. Hexsight bilgisayarında neyi saklar?

| Ne | Nerede | Nasıl silinir? |
|---|---|---|
| Ayarlar, veri önbelleği | `%LOCALAPPDATA%\Hexsight\settings.json` ve `%LOCALAPPDATA%\Hexsight\cache` | `%LOCALAPPDATA%\Hexsight` klasörünü sil |
| Kayıtlı maçların (en fazla 500) | `%LOCALAPPDATA%\Hexsight\history.json` | Dosyayı sil |
| Son tarama, yalnızca "Son taramayı kaydet"e basarsan | `%LOCALAPPDATA%\Hexsight\last-scan.png` ve `last-scan.txt` | Dosyaları sil |
| Çökme notu, yalnızca Hexsight çökerse | `%LOCALAPPDATA%\Hexsight\crash.log`: her çökme için saat, Hexsight sürümü, iş parçacığı, Windows derlemesi ve hata metniyle tek satır. Bilgisayarından çıkmaz | Dosyayı sil |
| Veri indirme ve okuma hatalarının kaydı | `%LOCALAPPDATA%\Hexsight\hexsight.log` ve `hexsight.log.1`: indirilemeyen ya da okunamayan her veri dosyası için saat, sunucu adı, dosya yolu, hata nedeni (HTTP durum kodu ya da hata türü) ve gelen bayt sayısıyla tek satır. Riot ID'ni, oyuncu kimliğini ve bilgisayarındaki klasör yollarını içermez. Her dosya en fazla 256 KB olur. Bilgisayarından çıkmaz | Dosyaları sil |
| Taşınabilir modda yukarıdaki ayarlar, önbellek, kayıtlı maçlar, son tarama, çökme notu ve veri kaydı | `<Hexsight klasörü>\Hexsight-data`, yalnızca `Hexsight.exe` ile aynı klasörde `Hexsight.portable` dosyası varsa (taşınabilir zip). Hexsight oraya yazamazsa `%LOCALAPPDATA%\Hexsight` klasörünü kullanır | `Hexsight-data` klasörünü sil |
| Oyun içi mağaza için eşya setleri | `<League klasörü>\Config\Global\Recommended\Hexsight-0.json`, `Hexsight-1.json` ve devamı | Ayarlar'da "Dizilimi oyun içi mağazaya gönder"i kapat ya da dosyaları sil |
| Windows ile başlatma girdisi, yalnızca Ayarlar'da "Windows açılınca arka planda başlat"ı açarsan | `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` altındaki `Hexsight` değeri | Ayarı kapat ya da değeri sil |

Kaldırma adımlarının tamamı README dosyasında.

### 4. Ekran görüntüsü

Hexsight eklenti kartlarını okumak için oyun penceresinin bir bölümünü yakalar. Bellekte en fazla bir görüntü tutar. Her tarama öncekinin yerini alır. Oyun bittikten 10 dakika sonra görüntüyü bellekten siler. Görüntüyü diske yalnızca "Son taramayı kaydet"e basınca yazar. Hiçbir yere göndermez. Kayıtlı bir tarama oyun ekranının bir bölümünü gösterebilir, bu yüzden paylaşmadan önce kontrol et.

### 5. Hexsight internete ne gönderir?

Hexsight herkese açık veri dosyalarını GET istekleriyle indirir. Şu sunucuları kullanır:

- `data.aramkit.com`: eklenti istatistikleri, dizilimler ve eşleşmeler.
- `raw.githubusercontent.com`: yedek istatistikler (aramhex-data).
- `raw.communitydragon.org`: isimler ve simgeler.
- `api.github.com`: en fazla günde bir kez, Hexsight'ın son sürümünün numarası. İstekte seninle ilgili hiçbir bilgi yok.

Her isteğin User-Agent başlığında `Hexsight/0.1` ve bu projenin adresi yer alır. İstek yolu, istemci dilini ve verisi istenen şampiyonların kimliklerini gösterir. Sunucular, her web sunucusu gibi IP adresini de görür. Her sunucu kendi gizlilik politikasına göre kendi kayıtlarını tutar. Hexsight onları denetleyemez.

### 6. Hexsight neyi asla göndermez?

Maçların, oyuncu kimliğin, ekran görüntülerin, bastığın tuşlar ve hesap bilgilerin bilgisayarından çıkmaz. League Client parolası bellekte kalır ve yalnızca 127.0.0.1 adresine gider.

### 7. Hesap yok, telemetri yok, reklam yok

Hexsight'ta hesap sistemi, analiz aracı, çökme raporu ve reklam yok. Kendi sunucusu da yok.

### 8. Hakların

Geliştirici seninle ilgili veri tutmadığı için onun elinde incelenecek, düzeltilecek ya da silinecek bir kayıt yok. Bilgisayarındaki tüm veriyi sen yönetirsin. Silmek için 3. bölümdeki dosyaları sil. AB'de ya da Birleşik Krallık'ta yaşıyorsan GDPR, Türkiye'de yaşıyorsan 6698 sayılı KVKK, başkalarının elindeki kişisel veriler üzerinde sana haklar tanır. Ülkendeki veri koruma kurumuna, Türkiye'deysen Kişisel Verileri Koruma Kurumuna (kvkk.gov.tr) başvurabilirsin.

### 9. Çocuklar

Hexsight çocuklara yönelik değil. League istemcisi kendi yaş sınırlarını uygular.

### 10. Değişiklikler

Her değişikliğin tarihini bu dosyanın başında gösteririz. Hexsight bir gün veri toplamaya başlarsa bunu o sürüm çıkmadan önce sürüm notlarında yazarız ve yasa gerektiriyorsa onayını isteriz.
