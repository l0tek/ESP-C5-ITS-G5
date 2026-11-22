# Android-App: Testbericht vom 10. November 2026

## Ergebnis

Installierbare ARM64- und x86-64-APKs wurden erfolgreich gebaut. Die x86-64-App
wurde auf einem Android-14-Emulator installiert und über ihren tatsächlichen
WebView bedient. Die ARM64-APK wurde auf Paketinhalt und Signatur geprüft; ein
physisches Android-Telefon war nicht angeschlossen.

## Builds

- Dioxus CLI / Dioxus 0.7.10, Rust stable 1.99.1.
- Android SDK 36, minSdk 26 (Android 8), targetSdk 34, NDK 27.2.12479018.
- Gradle 9.1.0, Android-Studio JBR 25.0.4.
- Rust im Release-Profil; APKs mit Android-Debugschlüssel signiert.
- `scripts/build-android.sh` für ARM64 und x86-64 erfolgreich.
- Desktop-Oberfläche mit `cargo check --locked --features desktop` geprüft.
- `cargo fmt --check`, Shellsyntax und `git diff --check` geprüft.

| APK | Größe | SHA256 |
| --- | ---: | --- |
| `dist/cits-map-0.1.0-android-arm64.apk` | 19.054.972 Bytes | `8db8a5e8942589aff339fead24e36c8ed73ff41cf57e2903aa9027ade9c040b7` |
| `dist/cits-map-0.1.0-android-x86_64.apk` | 20.327.069 Bytes | `e4862537e4a6737140f7459d6f8ffa4df17945b262bd0e0a3b8ef067afb08561` |

Die ARM64-APK enthält ausschließlich `lib/arm64-v8a/libmain.so`; die Emulator-APK
enthält ausschließlich `lib/x86_64/libmain.so`. ZIP-Prüfsummen sind gültig.
`apksigner verify` bestätigt die APK-v2-Signatur. Package-ID:
`org.l0tek.citsmap`; Anzeigename: `C-ITS Map`. Internet-Berechtigung und
USB-Host-Feature sind im fertigen Manifest vorhanden.

## Rust: 19 Tests bestanden

18 Integrationstests prüfen:

- CAM-Positionen aus vollständigen WLAN-/GeoNetworking-/BTP-Paketen.
- Geschwindigkeit und Richtung einschließlich ETSI-„unavailable“-Werten.
- DENM v1, Abmeldung anhand der Action-ID und DENM v2.
- QoS-Header sowie Ablehnung fragmentierter/verschlüsselter Frames.
- Beschädigte und abgeschnittene Frames, ungültiges Hex und Überlängen.
- USB-Chunk-Reassembly und CRLF.
- Logimport ohne letzte Newline; PCAP mit beiden Byte-Reihenfolgen.
- Abgeschnittenes PCAP, falscher Linktyp und PCAPNG-Ablehnung.
- Marker-Aktualisierung, zeitlicher Ablauf und Limit von 512 Objekten.
- MQTT-Topic-Validierung und Demo-/Importverarbeitung.

Ein zusätzlicher MQTT-Integrationstest verwendet ausschließlich einen lokalen
TCP-Testbroker auf `127.0.0.1`. Er prüft den MQTT-Handshake, Topic und rohe
Framebytes. Demo- und Importdaten erzeugen keine MQTT-Publikation; nur das danach
simulierte Livepaket wird gesendet. Es wurden keine Pakete an den öffentlichen
OpenTrafficMap-Broker veröffentlicht.

## Android-14-Emulator: 8 UI-Prüfungen bestanden

- App-Start und Initialisierung des mitgelieferten Leaflet.
- Drei synthetisch codierte CAMs erzeugen drei Marker und ein Stations-Popup.
- Tatsächliches Laden von OpenStreetMap-Kacheln über HTTPS.
- Eingefügtes CITS-Log wird dekodiert und auf der Karte dargestellt.
- Fehlerhafter Import zeigt eine Meldung und erhält die vorherige Karte.
- Android-JNI-USB-Geräteerkennung ohne angeschlossenen ESP liefert eine
  verständliche Meldung; die App bleibt lauffähig.
- Öffentliche Weiterleitung bleibt ohne USB-Liveverbindung gesperrt.
- Danksagung und Hinweis auf die unabhängige Umsetzung werden angezeigt.

Im Emulator wurde außerdem der angepasste User-Agent `CITSMap/0.1` geprüft.
Ein Screenshot der laufenden App liegt unter
[`docs/android-demo.png`](docs/android-demo.png). Der ausschließlich für diesen
Test gestartete Emulator wurde anschließend beendet.

## Noch nicht nachgewiesen

- USB-Gerätefreigabe und tatsächlicher Bulk-Datenempfang an einem physischen
  Android-Gerät mit ESP32-C5 über OTG; der Emulator-Test deckt die Geräteerkennung
  ohne angeschlossenen Empfänger ab.
- Positiver ITS-G5-Funkempfang. Der vorherige Firmwaretest meldete keine Frames.
- Öffentliche Annahme/Anzeige beim OpenTrafficMap-Server und Zuordnung einer
  registrierten Node-ID. Der Transport entspricht der am Testdatum eingesehenen
  Originalfirmware; eine Ende-zu-Ende-Veröffentlichung wurde nicht ausgeführt.
- Kryptografische Signaturprüfung, Last-/Dauerbetrieb, Hintergrundbetrieb und
  unterschiedliche Hersteller-WebViews.

Lokale ausführliche Logs liegen unter `../test-results/`; sie und die APKs sind
von Git ausgeschlossen. Die Quellen und der Bericht sind im Projekt enthalten.

## Import und Versionierung vom 20. November 2026

- Der Android-Import verwendet einen eigenen Button und den nativen
  `ACTION_OPEN_DOCUMENT`-Dialog in `android/MainActivity.kt`. Der ContentResolver
  liest die Auswahl in einem Hintergrundthread mit einer Grenze von 8 MiB.
  Der Rust-Decoder erhält die Bytes über JNI. Abbruch und Lesefehler werden
  angezeigt. HTML-Dateieingaben werden nur auf Desktop verwendet.
- Jeder Aufruf von `scripts/build-android.sh` erhöht Patchversion und Android-
  `versionCode` in `android-version.json`. Version und Buildnummer stehen im
  Info-Dialog; APK-Dateinamen enthalten die Version. Die Regel steht in
  `../AGENTS.md`.
- ARM64-APK 0.1.5, versionCode 6 erfolgreich gebaut. Paket-ID und Signierschlüssel
  entsprechen der vorherigen ARM64-APK. Manifestversion mit `aapt`, Signatur
  mit `apksigner` und SHA256-Datei geprüft.
- `cargo test --locked`: 1 Unit-Test und 19 Integrationstests erfolgreich;
  `cargo fmt --check`, `bash -n scripts/build-android.sh` und
  `git diff --check` erfolgreich.
- Android-14-Emulator neu eingerichtet; x86-64-APK 0.1.4 (versionCode 5)
  installiert. Der sichtbare Import-Button öffnet den nativen Systemdialog.
  Abbruch kehrt zur App zurück und zeigt „Dateiauswahl abgebrochen“.
- Die vom Benutzer bereitgestellte PCAP wurde über den echten Dokumentdialog
  aus Downloads ausgewählt: 1.118 Frames, 7 dekodierte CAM-Nachrichten und
  2 sichtbare Kartenmarker. Screenshot lokal:
  `../test-results/android-native-pcap-import.png`.
- Die Infrastruktur-Aufzeichnung `cits-1795365581771.pcap` mit 292 Frames wurde
  direkt gegen den Runtime-Import geprüft: 284 MAPEM-, SPATEM- und
  SREM-Nachrichten werden dekodiert und als sieben Infrastruktur-Einträge an
  ihren GeoNetworking-Quellpositionen dargestellt; acht reine Beacons bleiben
  als nicht darstellbar gekennzeichnet.
- Die automatische Prüfung des Abbruchhinweises überschritt zunächst das
  20-Sekunden-Zeitlimit des frisch gestarteten Emulators. Der Hinweis und der
  wieder aktivierte Auswahlbutton wurden danach im WebView bestätigt.
- Kein physisches Telefon angeschlossen; das Update dort ist noch zu bestätigen.
