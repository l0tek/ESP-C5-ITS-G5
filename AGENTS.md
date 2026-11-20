# Projektregeln

## Android-Builds

- Bei jedem neuen APK-Build müssen Android `versionCode` und `versionName`
  steigen, auch bei reinen Fehlerkorrekturen oder erneutem Bauen.
- Ausschließlich `android-app/scripts/build-android.sh` für installierbare APKs
  verwenden. Das Skript erhöht vor jedem Build die Patchversion und den
  `versionCode` in `android-app/android-version.json` automatisch. Verbrauchte
  Versionsnummern bei fehlgeschlagenen Builds nicht zurücksetzen.
- Den aktualisierten Versionsstand mit den Änderungen committen. Bei parallelen
  Branches vor einem Build den neuesten Versionsstand übernehmen und Konflikte
  mit einer höheren Nummer auflösen.
- Paket-ID `org.l0tek.citsmap` und bisherigen Signierschlüssel beibehalten, damit
  ein Update über die vorhandene Installation möglich bleibt.
- APK-Dateiname und Versionsanzeige müssen die tatsächlich verpackte Version
  enthalten. Nach dem Build Version und Signatur der fertigen APK prüfen.

## Android-Dateiimport

- Der Import muss über den sichtbaren Button „PCAP / Log auswählen …“ den
  nativen Android-Dokumentdialog öffnen. Eine Änderung des `accept`-Filters
  eines HTML-Dateifelds behebt keinen fehlenden Android-Dateidialog.
- Die Activity-Vorlage liegt in `android-app/android/MainActivity.kt` und wird
  vom Buildskript in das generierte Projekt kopiert. Änderungen an generierten
  Dateien allein gehen beim nächsten Build verloren.
