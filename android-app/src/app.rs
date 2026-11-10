use crate::{forwarding::Forwarder, model::Dashboard, runtime::Runtime};
use dioxus::prelude::*;
use std::{sync::atomic::Ordering, time::Duration};

const CSS: Asset = asset!("/assets/app.css");
const LEAFLET_CSS: Asset = asset!("/assets/vendor/leaflet.css");
const LEAFLET_JS: Asset = asset!("/assets/vendor/leaflet.js");

#[component]
pub fn App() -> Element {
    let runtime = use_hook(Runtime::default);
    let mut snapshot = use_signal(Dashboard::default);
    let mut modal = use_signal(|| "");
    let mut pasted = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut node_id = use_signal(String::new);
    let mut forwarding = use_signal(|| (false, String::new(), 0u64, 0u64));
    let poll_runtime = runtime.clone();
    use_future(move || {
        let rt = poll_runtime.clone();
        async move {
            loop {
                let data = rt.snapshot();
                if let Ok(json) = serde_json::to_string(&data) {
                    let _ = document::eval(&format!("window.__citsData={json};if(window.citsRender)window.citsRender(window.__citsData);")).await;
                }
                snapshot.set(data);
                let next = rt
                    .forwarder
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|f| {
                        (
                            true,
                            f.status.lock().unwrap().clone(),
                            f.sent.load(Ordering::Relaxed),
                            f.dropped.load(Ordering::Relaxed),
                        )
                    })
                    .unwrap_or_default();
                forwarding.set(next);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    });
    use_effect(move || {
        spawn(async {
            let _ = document::eval(include_str!("../assets/map.js")).await;
        });
    });
    let cleanup = runtime.clone();
    use_drop(move || cleanup.stop());
    let data = snapshot.read().clone();
    let objects: Vec<_> = data.objects.values().cloned().collect();
    let vehicles = objects.iter().filter(|o| o.kind == "CAM").count();
    let events = objects.iter().filter(|o| o.kind == "DENM").count();
    let connect = runtime.clone();
    let demo = runtime.clone();
    let import_file = runtime.clone();
    let import_text = runtime.clone();
    let share = runtime.clone();
    let source = data.source.clone();
    rsx! {
        document::Title { "C-ITS Map" }
        document::Stylesheet { href: LEAFLET_CSS }
        document::Stylesheet { href: CSS }
        document::Script { src: LEAFLET_JS }
        main { class: "shell",
            header {
                div { class: "brand",
                    div { class: "brand-icon", aria_hidden: "true", "⌁" }
                    div { div { class: "eyebrow", "ESP32-C5 / ITS-G5" } h1 { "C-ITS Map" } }
                }
                span { class: "badge", if source.starts_with("DEMO") { "DEMO" } else { "RX ONLY" } }
            }
            div { class: "status-bar", role: "status",
                span { class: if data.running { "led active" } else { "led" } }
                span { "{data.status}" }
            }
            section { class: "map-wrap", aria_label: "Karte empfangener ITS-G5-Positionen",
                div { id: "traffic-map" }
                div { class: "map-overlay",
                    span { class: "map-pill", "CH 180 · 5900 MHz" }
                    if forwarding.read().0 { span { class: "map-pill", "OTM ↗" } }
                }
                div { class: "map-bottom",
                    span { id: "map-status", class: "map-status", role: "status" }
                    button { class: "quiet", title: "Alle Positionen anzeigen", onclick: move |_| { spawn(async { let _ = document::eval("if(window.citsFit)window.citsFit();").await; }); }, "⊙ Übersicht" }
                }
            }
            section { class: "panel",
                div { class: "metrics",
                    div { class: "metric", strong { "{data.frames}" } span { "Frames" } }
                    div { class: "metric", strong { "{vehicles}" } span { "CAM-Stationen" } }
                    div { class: "metric", strong { "{events}" } span { "DENM-Ereignisse" } }
                    div { class: "metric", strong { "{data.rejected + data.framing_errors}" } span { "Nicht darstellbar" } }
                }
                div { class: "toolbar",
                    button { class: "primary", onclick: move |_| { notice.set(String::new()); if connect.snapshot().running { connect.stop(); } else { connect.connect(); } },
                        if data.running { "USB trennen" } else { "USB verbinden" }
                    }
                    button { disabled: data.running, onclick: move |_| {notice.set(String::new());modal.set("import");}, "Import" }
                    button { class: "quiet", disabled: data.running, onclick: move |_| { if let Err(e)=demo.demo(){notice.set(e);} }, "Demo" }
                    button { class: "quiet", onclick: move |_| {notice.set(String::new());modal.set("share");}, "OpenTrafficMap ↗" }
                }
                div { class: "source", span { "{source}" } button { class: "quiet", style: "padding:0;border:0;font-size:11px", onclick: move |_| modal.set("about"), "Info" } }
                if !notice.read().is_empty() { div { class: "notice", role: "alert", "{notice}" } }
                if objects.is_empty() {
                    p { class: "empty", "Noch keine Positionen. ESP über USB-OTG verbinden oder eine Aufnahme importieren. Mit „Demo“ kannst du die Karte ausprobieren." }
                } else {
                    ul { class: "object-list",
                        for object in objects {
                            li { key: "{object.key}",
                                button { onclick: move |_| {
                                    let key=serde_json::to_string(&object.key).unwrap_or_default();
                                    spawn(async move { let _=document::eval(&format!("if(window.citsFocus)window.citsFocus({key});")).await; });
                                },
                                    span { span { class: if object.kind=="DENM" {"tag event"} else {"tag"}, "{object.kind}" } "{object.station_id}" }
                                    small { if let Some(speed)=object.speed_kmh { "{speed:.0} km/h" } else { "{object.latitude:.4}, {object.longitude:.4}" } }
                                }
                            }
                        }
                    }
                }
                if data.rejected > 0 { details { class: "empty", summary { "Letzter Decoderhinweis" } "{data.last_error}" } }
                footer { "Unabhängige Umsetzung · " a { href: "https://opentrafficmap.org", target: "_blank", rel: "noopener", "Danke, OpenTrafficMap" } }
            }
        }
        if !modal.read().is_empty() {
            div { class: "dialog-backdrop",
                section { class: "dialog-card", role: "dialog", aria_modal: "true", aria_label: "App-Einstellungen und Import",
                    if *modal.read()=="import" {
                        h2 { "Aufnahme öffnen" }
                        p { "CITS-Log oder klassisches WLAN-PCAP (Linktyp 105), maximal 8 MiB. Importierte Pakete werden ausschließlich lokal verarbeitet." }
                        input { r#type: "file", accept: ".txt,.log,.pcap", onchange: move |event: Event<FormData>| {
                            let rt=import_file.clone();
                            async move {
                                if let Some(file)=event.files().first() {
                                    if file.size()>crate::stream::MAX_IMPORT as u64 {notice.set("Datei größer als 8 MiB".into());return;}
                                    match file.read_bytes().await {
                                        Ok(bytes)=> match rt.import(&bytes,"Dateiimport") {Ok(())=>modal.set(""),Err(e)=>notice.set(e)},
                                        Err(e)=>notice.set(format!("Datei: {e}")),
                                    }
                                }
                            }
                        } }
                        label { r#for: "packet-log", "Oder CITS-Zeilen einfügen" }
                        textarea { id: "packet-log", placeholder: "CITS:08…", value: "{pasted}", oninput: move |e| pasted.set(e.value().chars().take(100_000).collect()) }
                        if !notice.read().is_empty() { p { role: "alert", "{notice}" } }
                        div { class: "dialog-actions",
                            button { onclick: move |_| modal.set(""), "Schließen" }
                            button { class: "primary", onclick: move |_| {
                                match import_text.import(pasted.read().as_bytes(),"Eingefügtes Log") {Ok(())=>modal.set(""),Err(e)=>notice.set(e)}
                            }, "Verarbeiten" }
                        }
                    } else if *modal.read()=="share" {
                        h2 { "Auf OpenTrafficMap teilen" }
                        p { "Mit aktivierter Weiterleitung sendet die App neu empfangene USB-Livepakete an den OpenTrafficMap-Server. Die enthaltenen Positionen können öffentlich auf der Karte erscheinen. Demo- und Importdaten werden nie weitergeleitet." }
                        label { r#for: "node-id", "Deine OpenTrafficMap-Node-ID" }
                        input { id: "node-id", r#type: "text", placeholder: "z. B. MAC-Adresse deines Empfängers", value: "{node_id}", disabled: forwarding.read().0, oninput: move |e| node_id.set(e.value().chars().take(64).collect()) }
                        p { "Verwende deine eigene registrierte Node-ID. Kartenanzeige und Freischaltung hängen vom OpenTrafficMap-Server ab." }
                        a { class: "help-link", href: "https://wiki.opentrafficmap.org/en:node_registration", target: "_blank", rel: "noopener", "Node-Registrierung ↗" }
                        if forwarding.read().0 {
                            p { role: "status", "{forwarding.read().1} · {forwarding.read().2} gesendet · {forwarding.read().3} verworfen" }
                        }
                        if !notice.read().is_empty() { p { role: "alert", "{notice}" } }
                        div { class: "dialog-actions",
                            button { onclick: move |_| modal.set(""), "Schließen" }
                            button { class: "primary", disabled: !data.running && !forwarding.read().0, onclick: move |_| {
                                if share.forwarder.lock().unwrap().is_some() {share.stop_forwarding();}
                                else { match Forwarder::start(&node_id.read()) {
                                    Ok(f)=>*share.forwarder.lock().unwrap()=Some(f), Err(e)=>notice.set(e),
                                }}
                            }, if forwarding.read().0 {"Weiterleitung stoppen"} else {"Livepakete teilen"} }
                        }
                        p { a { class: "help-link", href: "https://opentrafficmap.org", target: "_blank", rel: "noopener", "Öffentliche OpenTrafficMap öffnen ↗" } }
                    } else {
                        h2 { "Offene Verkehrsforschung" }
                        p { "OpenTrafficMap hat wichtige Grundlagen für günstigen C-ITS-Empfang, ESP32-C5-Firmware, Paketweiterleitung und öffentliche Verkehrskarten geschaffen. Vielen Dank an alle Mitwirkenden." }
                        p { "Dieses Projekt ist keine offizielle OpenTrafficMap-Veröffentlichung." }
                        p { "Die App dekodiert CAM und DENM. Positionen werden nach 60 s (CAM) bzw. 5 min (DENM) ausgeblendet. Kryptografische Signaturen werden nicht geprüft; die Anzeige ist keine verlässliche Verkehrs- oder Gefahrenwarnung." }
                        p { "Pakete bleiben lokal, solange die Weiterleitung ausgeschaltet ist. Kartenkacheln werden von OpenStreetMap geladen; dafür wird eine Internetverbindung benötigt." }
                        a { class: "help-link", href: "https://codeberg.org/opentrafficmap", target: "_blank", rel: "noopener", "OpenTrafficMap auf Codeberg ↗" }
                        div { class: "dialog-actions", button { class: "primary", onclick: move |_|modal.set(""), "Schließen" } }
                    }
                }
            }
        }
    }
}
