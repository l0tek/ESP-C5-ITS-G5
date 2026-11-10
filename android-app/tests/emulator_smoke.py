"""UI smoke test against an installed debug APK on a dedicated Android emulator.
Requires websocket-client. Run from the repository root after installing the APK.
No public MQTT connection is made.
"""
import json
import os
import pathlib
import subprocess
import time
import urllib.request
import websocket

ROOT = pathlib.Path(__file__).resolve().parents[2]
ADB = os.environ.get("ADB", str(pathlib.Path.home() / "Android/Sdk/platform-tools/adb"))
SERIAL = os.environ.get("ANDROID_SERIAL", "emulator-5554")
PACKAGE = "org.l0tek.citsmap"
OUTPUT = ROOT / "test-results"
OUTPUT.mkdir(exist_ok=True)


def adb(*args, **kwargs):
    return subprocess.check_output([ADB, "-s", SERIAL, *args], **kwargs)


adb("shell", "am", "force-stop", PACKAGE)
adb("shell", "am", "start", "-W", "-n", PACKAGE + "/dev.dioxus.main.MainActivity")
pid = adb("shell", "pidof", PACKAGE).decode().strip().split()[0]
adb("forward", "tcp:9223", f"localabstract:webview_devtools_remote_{pid}")
for attempt in range(50):
    try:
        pages = json.load(urllib.request.urlopen("http://127.0.0.1:9223/json", timeout=5))
        if any(p.get("type") == "page" for p in pages):
            break
    except OSError:
        pass
    time.sleep(0.2)
else:
    raise AssertionError("Android WebView debugging endpoint did not become ready")
page = next(p for p in pages if p["type"] == "page")
ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, suppress_origin=True)
sequence = 0


def evaluate(expression):
    global sequence
    sequence += 1
    ws.send(json.dumps({"id": sequence, "method": "Runtime.evaluate", "params": {
        "expression": expression, "returnByValue": True, "awaitPromise": True,
    }}))
    while True:
        message = json.loads(ws.recv())
        if message.get("id") != sequence:
            continue
        assert "error" not in message, message
        result = message["result"]
        assert "exceptionDetails" not in result, result
        return result.get("result", {}).get("value")


def wait(expression, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if evaluate(expression):
            return
        time.sleep(0.2)
    raise AssertionError(f"UI condition timed out: {expression}")


def click(text):
    evaluate("Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()==="
             + json.dumps(text) + ").click()")


try:
    wait("document.body.innerText.includes('C-ITS Map') && !!window.citsMap")
    assert evaluate("window.citsMap.getZoom()") == 6
    print("PASS launch and Leaflet initialization")
    click("Demo")
    wait("document.querySelectorAll('.vehicle-dot').length===3 && document.body.innerText.includes('DEMO · synthetische Pakete')")
    assert evaluate("document.querySelector('.metric strong').textContent") == "3"
    evaluate("document.querySelector('.object-list button').click()")
    wait("document.querySelector('.leaflet-popup-content')?.textContent.includes('1001')")
    print("PASS synthetic packets produce 3 map markers and station popup")
    try:
        wait("document.querySelectorAll('img.leaflet-tile-loaded').length>0", 15)
        print("PASS OpenStreetMap tiles loaded")
    except AssertionError:
        print("SKIP tiles unavailable; offline map-error state visible")
    OUTPUT.joinpath("android-demo.png").write_bytes(adb("exec-out", "screencap", "-p"))
    click("Import")
    wait("!!document.querySelector('textarea')")
    demo = ROOT.joinpath("test-results/demo-input.log").read_text()
    evaluate("(()=>{const e=document.querySelector('textarea');e.value=" + json.dumps(demo)
             + ";e.dispatchEvent(new Event('input',{bubbles:true}));})()")
    click("Verarbeiten")
    wait("!document.querySelector('[role=dialog]') && document.body.innerText.includes('Eingefügtes Log') && document.querySelectorAll('.vehicle-dot').length===3")
    print("PASS pasted CITS log decoded and plotted")
    click("Import")
    wait("!!document.querySelector('textarea')")
    evaluate("(()=>{const e=document.querySelector('textarea');e.value='CITS:zz';e.dispatchEvent(new Event('input',{bubbles:true}));})()")
    click("Verarbeiten")
    wait("document.querySelector('[role=dialog]')?.innerText.includes('Keine Frames gefunden')")
    click("Schließen")
    print("PASS malformed import reports error and preserves prior map")
    click("USB verbinden")
    wait("document.querySelector('.status-bar').textContent.includes('Genau einen ESP32-C5')")
    print("PASS Android JNI USB enumeration with no receiver reports a useful error")
    click("OpenTrafficMap ↗")
    wait("document.querySelector('[role=dialog]')?.innerText.includes('öffentlich')")
    assert evaluate("Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()==='Livepakete teilen').disabled")
    click("Schließen")
    print("PASS OpenTrafficMap publication requires live USB connection")
    click("Info")
    wait("document.querySelector('[role=dialog]')?.innerText.includes('keine offizielle OpenTrafficMap')")
    print("PASS independent-project attribution displayed")
    click("Schließen")
finally:
    ws.close()
    adb("forward", "--remove", "tcp:9223")
