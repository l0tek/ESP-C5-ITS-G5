#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ndk_version=$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort -V | tail -1)
  export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$ndk_version"
fi
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
if [[ -z "${JAVA_HOME:-}" ]]; then
  for candidate in /snap/android-studio/current/jbr /opt/android-studio/jbr /usr/lib/jvm/java-17-openjdk-amd64; do
    if [[ -x "$candidate/bin/java" ]]; then export JAVA_HOME="$candidate"; break; fi
  done
fi
: "${JAVA_HOME:?Bitte JAVA_HOME auf ein JDK 17 oder neuer setzen}"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"
android_target="${1:-aarch64-linux-android}"
case "$android_target" in
  aarch64-linux-android) abi=arm64; native_abi=arm64-v8a ;;
  x86_64-linux-android) abi=x86_64; native_abi=x86_64 ;;
  *) echo "Unbekanntes Ziel: $android_target" >&2; exit 1 ;;
esac
dx build --android --release --locked --no-default-features --features mobile --target "$android_target"
project="target/dx/cits-map/release/android/app"
python3 - "$project" "$native_abi" <<'PY'
import pathlib,re,sys,xml.etree.ElementTree as ET
project=pathlib.Path(sys.argv[1])
path=project/'app/src/main/AndroidManifest.xml'
ns='http://schemas.android.com/apk/res/android'; ET.register_namespace('android',ns)
tree=ET.parse(path);root=tree.getroot()
if not any(v.get('{'+ns+'}name')=='android.hardware.usb.host' for v in root.findall('uses-feature')):
 ET.SubElement(root,'uses-feature',{'{'+ns+'}name':'android.hardware.usb.host','{'+ns+'}required':'true'})
if not any(v.get('{'+ns+'}name')=='android.permission.INTERNET' for v in root.findall('uses-permission')):
 ET.SubElement(root,'uses-permission',{'{'+ns+'}name':'android.permission.INTERNET'})
app=root.find('application');app.set('{'+ns+'}label','C-ITS Map');app.set('{'+ns+'}usesCleartextTraffic','false')
tree.write(path,encoding='utf-8',xml_declaration=True)
# dx 0.7.10 does not expose WebView User-Agent configuration. Identify tile requests
# in the generated Wry WebView before rebuilding the final APK.
webview=project/'app/src/main/kotlin/dev/dioxus/main/RustWebView.kt'
s=webview.read_text()
agent=' CITSMap/0.1 (+https://github.com/l0tek/ESP-C5-ITS-G5)'
if agent not in s:
 anchor='settings.javaScriptEnabled = true'
 if anchor not in s: raise SystemExit('Wry WebView template changed; User-Agent patch must be updated')
 s=s.replace(anchor,anchor+'\n        settings.userAgentString = settings.userAgentString + "'+agent+'"',1)
 webview.write_text(s)
gradle=project/'app/build.gradle.kts'
s=re.sub(r'// CITS-ABI-BEGIN.*?// CITS-ABI-END\n?', '', gradle.read_text(), flags=re.S)
s+='\n// CITS-ABI-BEGIN\nandroid { defaultConfig { ndk { abiFilters.clear(); abiFilters.add("'+sys.argv[2]+'") } } }\n// CITS-ABI-END\n'
gradle.write_text(s)
PY
(cd "$project" && ./gradlew --no-daemon assembleDebug)
mkdir -p dist
output="cits-map-0.1.0-android-$abi.apk"
cp "$project/app/build/outputs/apk/debug/app-debug.apk" "dist/$output"
(cd dist && sha256sum "$output" > "$output.sha256")
printf 'APK: %s/dist/%s\n' "$PWD" "$output"
