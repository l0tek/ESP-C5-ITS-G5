# C-ITS RX – Waveshare ESP32-C5-Zero (Rust)

Rust-Firmware für den Empfang roher ITS-G5-/802.11p-Datenframes auf Kanal 180
(5900 MHz), mit paralleler Ausgabe über Bluetooth LE und USB sowie PCAP-Recorder
für Linux. Die Anwendung ruft
keine Sendefunktion auf und verbindet sich nicht mit einem Access Point.

**Stand 10. November 2026:** Release-Build, Flashen und Start auf dem angeschlossenen
ESP32-C5 Revision 1.0 mit 4 MB Flash erfolgreich geprüft. Die Radioinitialisierung
meldet `CITS-READY`. Im Laufzeittest wurden keine Funkframes gemeldet; tatsächlicher
ITS-G5-Empfang und Betrieb unter Paketlast sind daher noch nicht nachgewiesen.
Die Ergebnisse stehen in [TEST-REPORT.md](TEST-REPORT.md).

## Hardware und Aufbau

- Waveshare ESP32-C5-Zero, 4 MB Flash, kein PSRAM.
- GPIO26 LOW: interne Keramikantenne; HIGH: externe IPEX-1-Antenne.
- Bei einer Bluetooth-Verbindung kann die Android-App GPIO26 zur Laufzeit über
  **Antenne: Intern / Extern** umschalten. Nach jedem Neustart ist intern aktiv.
- USB-C über USB Serial/JTAG; keine UART-Brücke erforderlich.
- `firmware/`: eigenständiges Cargo-Projekt für ESP32-C5.
- `host/src/main.rs`: PCAP-Konverter ohne externe Rust-Crates.
- `flash.sh`: Firmware bauen und flashen.
- `capture.sh`: Host-Konverter bauen, USB-Port konfigurieren und aufnehmen.

Der Radiokern `esp32-cits-core` ist auf Commit
`a6d0ea6ca2da130fde2b84060a34ef45e772a459` festgelegt. PHY-Funktionen werden
über diesen unveränderten Upstream-Kern verwendet. `firmware/Cargo.lock` und
die datierte Rust-Toolchain legen den geprüften Build fest.

## Bauen und flashen

Voraussetzung sind rustup, ein C-Linker sowie `cargo-espflash` mit ESP32-C5-Unterstützung
(hier Version 4.5.0). Beim ersten Build werden Toolchain und Abhängigkeiten geladen.

```bash
cargo install cargo-espflash --version 4.5.0 --locked
./flash.sh
# Alternativ USB-Port explizit angeben:
./flash.sh /dev/ttyACM0
```

Das Skript erkennt genau ein Espressif USB-Serial/JTAG-Gerät anhand seines
stabilen `/dev/serial/by-id/`-Pfads. Linux kann die Nummer von `ttyACM0` nach
einem Reset ändern. Flashen ersetzt die bestehende Firmware auf dem Board.

Nur bauen:

```bash
cd firmware
cargo build --release --locked
```

Ergebnis: `firmware/target/riscv32imac-unknown-none-elf/release/cits-rx`.
Der Build benötigt weder PSRAM noch `protoc`, GNSS oder Display-Abhängigkeiten.

## Laufzeit prüfen

```bash
cd firmware
cargo espflash monitor --chip esp32c5 --non-interactive \
  --port /dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_3C:DC:75:8A:FB:68-if00
```

Der Beispielpfad gehört zum hier geprüften Board. Erwartete Meldungen:

```text
CITS-BOOT waveshare-esp32-c5-zero channel=180 rx-only
CITS-READY channel=180 freq=5900MHz antenna=onboard
CITS-STAT packets=0 callbacks=0 dropped=0 ble_dropped=0 uptime_ms=5783
```

Alle fünf Sekunden erscheint eine Statuszeile. `packets` zählt ausgegebene
Datenframes, `callbacks` alle Aufrufe des Empfangs-Callbacks, `dropped`
Queue-/Speicherverluste akzeptierter Datenframes und `ble_dropped` volle
Bluetooth-Ausgabequeues. Ausgefilterte Frames werden
nicht als Verlust gezählt. Keine `CITS:`-Zeilen bedeuten keinen exportierten
Datenframe; ohne bekannte Funkquelle lässt sich daraus kein Hardwarefehler ableiten.

## LED-Status

Die WS2812-RGB-LED an GPIO27 zeigt den Firmwarezustand:

- Gelb: Start und Funkinitialisierung.
- Schwaches Grün: Empfänger bereit.
- Blau: mindestens 80 ms nach einem exportierten 802.11-Datenframe;
  bei anhaltendem Empfang bleibt sie blau.
- Rot: Fehler beim Initialisieren von WLAN, 5-GHz-Modus oder Sniffer.
  Die Firmware meldet `CITS-ERROR` über USB und wartet bis zum Reset.

Die LED wird direkt von der ESP-Firmware gesteuert; dafür muss diese auf dem
Board aktualisiert werden. Eine APK-Installation allein ändert die LED nicht.
Grün bestätigt die Initialisierung, nicht den tatsächlichen ITS-G5-Empfang.

## PCAP aufnehmen

Den seriellen Monitor vorher schließen; nur ein Prozess sollte den USB-Datenstrom lesen.

```bash
./capture.sh
# Oder Eingabe und Ausgabedatei explizit wählen:
./capture.sh /dev/ttyACM0 neue-aufnahme.pcap
./capture.sh bootlog.txt aus-log.pcap
wireshark neue-aufnahme.pcap
```

Der Recorder akzeptiert `CITS:<hex>` und ignoriert Status-/Bootmeldungen.
Er erzeugt klassisches PCAP mit `LINKTYPE_IEEE802_11 = 105`. Jede vollständig
verarbeitete Paketzeile wird sofort aus dem Benutzerpuffer geschrieben, ebenso
der Dateiheader. Bereits existierende Ausgabedateien werden abgelehnt.
Strg+C beendet die Aufnahme; die zuletzt gerade verarbeitete Zeile kann fehlen.
Dies ist keine Zusicherung gegen Stromausfall.

Für den Host-Konverter genügt auch:

```bash
rustc --edition=2021 -O host/src/main.rs -o cits-pcap
./cits-pcap bootlog.txt aus-log.pcap
rustc --edition=2021 --test host/src/main.rs -o /tmp/cits-host-tests
/tmp/cits-host-tests
python3 tests/test_host.py
```

Bei fehlenden USB-Rechten unter Ubuntu den Benutzer zur Gruppe `dialout`
hinzufügen und neu anmelden.

## Grenzen

- Zeitstempel sind Host-Verarbeitungszeiten, keine Funk-Empfangszeitstempel.
  Bei Logdateien wird die Konvertierungszeit verwendet.
- Kein CAM-/DENM-Decoder und keine Radiotap-Metadaten wie RSSI.
- 16 Empfangs- und vier Bluetooth-Ausgabeeinträge; Hex-Ausgabe verdoppelt
  ungefähr das Transportvolumen. Hohe Paketlast und Transportunterbrechungen
  können Frames kosten.
- Kein positiver Funkempfangstest ohne passende ITS-G5-Quelle; `CITS-READY`
  bestätigt die erfolgreiche Rückkehr der Initialisierungsfunktionen.
- Die Antennenumschaltung setzt nur den RF-Schalter GPIO26. Eine externe
  Antenne muss vor Auswahl von „Extern“ tatsächlich am IPEX-1-Anschluss sitzen.

## Upstream

- https://git.hamburg.ccc.de/jtbx/esp32-c_its-companion
- https://git.hamburg.ccc.de/jtbx/esp32-cits-core
- https://docs.waveshare.com/ESP32-C5-Zero

`setup-upstream.sh` bleibt als alternativer Integrationsweg für das vollständige
Originalprojekt erhalten. Es wird für den eigenständigen Build nicht benötigt.
Dieser ältere Integrationsweg benötigt zusätzliche Build-Werkzeuge (unter anderem
`protoc`) und wurde hier nicht bis zum fertigen Firmware-Build validiert.
Die jeweiligen Upstream-Lizenzen gelten für die eingebundenen Abhängigkeiten.

## Android-App mit Rust und Dioxus

Die eigenständige App unter [`android-app/`](android-app/) empfängt die `CITS:`-Zeilen
des ESP32-C5 per Bluetooth LE oder USB-OTG, dekodiert CAM-/DENM-Pakete in Rust und zeigt Positionen
auf einer OpenStreetMap-Karte an. Aufnahmen lassen sich als Log oder klassisches
PCAP importieren. Eine ausdrücklich aktivierbare TLS/MQTT-Weiterleitung sendet
neue Livepakete im OpenTrafficMap-Format an dessen Server; Demo- und
Importdaten werden nicht veröffentlicht.

Build, Installation, getestete Funktionen und Grenzen:
[`android-app/README.md`](android-app/README.md).

## Acknowledgements: OpenTrafficMap

OpenTrafficMap has done the important groundwork of exploring low-cost C-ITS
reception, ESP32-C5 receiver firmware, packet forwarding, and public traffic-map
tooling.

Huge thanks and praise to the OpenTrafficMap contributors for making their work
available and for pushing open, accessible traffic infrastructure research forward.

Links:

- OpenTrafficMap website: [https://opentrafficmap.org](https://opentrafficmap.org)
- OpenTrafficMap Codeberg organization: [https://codeberg.org/opentrafficmap](https://codeberg.org/opentrafficmap)
- Original repository family: [https://codeberg.org/opentrafficmap](https://codeberg.org/opentrafficmap)

This project is not an official OpenTrafficMap release.
