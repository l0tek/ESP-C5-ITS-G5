# Build- und Hardwaretest vom 10. November 2026

## Ergebnis

Die Capture-Firmware wurde erstellt, auf das über USB angeschlossene ESP32-C5-Board
geflasht und gestartet. Die Radioinitialisierung kehrt erfolgreich zurück.
Ein Lauf über 60 Sekunden Geräte-Uptime zeigte keinen Panic oder Anwendungsneustart.
Ein positiver Nachweis des ITS-G5-Funkempfangs steht aus: Im Test meldete der
Empfangs-Callback keine Frames. Eine kontrollierte ITS-G5-Sendequelle war nicht
Teil des Tests.

## Geprüfte Hardware und Software

- ESP32-C5, Revision 1.0, 48-MHz-Quarz, 4 MB Flash.
- USB-Seriennummer/MAC: `3C:DC:75:8A:FB:68`.
- Interne Antenne über GPIO26 LOW, Kanal 180 / 5900 MHz.
- Rust `nightly-2026-08-25`, rustc `1.100.0-nightly (e7769602a 2026-08-24)`.
- `cargo-espflash` 4.5.0.
- `esp-hal` 1.1.2, `esp-radio` 0.18.0, `esp-rtos` 0.3.0.
- `esp32-cits-core` Commit `a6d0ea6ca2da130fde2b84060a34ef45e772a459`.
- Eigenständiger Build mit `firmware/Cargo.lock`; kein PSRAM und kein Protobuf-Build.

## Durchgeführte Prüfungen

| Prüfung | Ergebnis |
| --- | --- |
| `cd firmware && cargo build --release --locked` | Erfolgreich |
| `./flash.sh` mit automatischer USB-Erkennung | Erfolgreich, reguläre Flash-Verifikation aktiviert |
| Finale Anwendungsgröße | 520.832 Bytes, 12,61 % der Factory-Partition |
| Start und Radioinitialisierung | `CITS-BOOT` und `CITS-READY` beobachtet |
| Laufzeit | 12 Statusmeldungen bis `uptime_ms=60788`, monoton steigend |
| Funkzähler am Ende dieses Laufs | `packets=0 callbacks=0 dropped=0` |
| Host-Unit-Tests | 4/4 bestanden |
| Host-Integrationstests | 5/5 bestanden |
| USB-Aufnahme über `capture.sh` | PCAP-Header erzeugt, keine Funkdatensätze |
| Shellsyntax | `bash -n` für alle drei Skripte erfolgreich |

Der Laufzeitmitschnitt liegt lokal unter `test-results/board-runtime.log`.
Der letzte Build wurde anschließend über `flash.sh` erneut geflasht; der
USB-Recorder wurde gegen dieses Board ausgeführt. Linux änderte beim Reset
den Gerätenamen von `/dev/ttyACM0` auf `/dev/ttyACM1`. Die Skripte verwenden
daher standardmäßig den stabilen Gerätepfad unter `/dev/serial/by-id/`.

## Behobene Probleme

- Ein eigenständiges Firmware-Manifest ersetzt für den regulären Build den
  vollständigen Companion-Build, der an fehlendem `protoc` scheiterte.
- Firmware meldet alle fünf Sekunden Status und zählt Queue-/Speicherverluste.
- Bei voller Queue wird vor der Paketallokation verworfen; fehlgeschlagene
  Speicherreservierungen werden abgefangen. Der Hex-Ausgabepuffer wird wiederverwendet.
- Der Host schreibt Header und jeden vollständigen Paketdatensatz sofort aus.
  Der Regressionstest prüft den Erhalt eines verarbeiteten Frames nach SIGINT.
- Vorhandene Ausgabedateien werden abgelehnt; ungültige Eingaben erzeugen keine
  Ausgabedatei. Identische Ein-/Ausgabepfade beschädigen die Quelle nicht mehr.

## Wiederholen

```bash
./flash.sh
python3 tests/test_host.py
rustc --edition=2021 --test host/src/main.rs -o /tmp/cits-host-tests
/tmp/cits-host-tests
./capture.sh
```

Die nächste offene Hardwareprüfung ist der Empfang einer bekannten ITS-G5-Quelle
auf Kanal 180 mit Prüfung der exportierten Frames in Wireshark. Durchsatz,
Paketverlustrate unter Last und Antennenreichweite sind noch nicht gemessen.
