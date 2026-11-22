use crate::{
    decoder,
    forwarding::Forwarder,
    model::Dashboard,
    stream::{LineDecoder, import_frames},
};
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_RECORDING_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub struct Recording {
    frames: VecDeque<(u64, Vec<u8>)>,
    bytes: usize,
    pub evicted: u64,
}
impl Recording {
    fn push(&mut self, received_ms: u64, frame: &[u8]) {
        if frame.len() > MAX_RECORDING_BYTES {
            self.evicted += 1;
            return;
        }
        while self.bytes + frame.len() > MAX_RECORDING_BYTES {
            if let Some((_, old)) = self.frames.pop_front() {
                self.bytes -= old.len();
                self.evicted += 1;
            }
        }
        self.bytes += frame.len();
        self.frames.push_back((received_ms, frame.to_vec()));
    }
    pub fn pcap(&self) -> Vec<u8> {
        crate::stream::export_pcap(
            self.frames
                .iter()
                .map(|(time, frame)| (*time, frame.as_slice())),
        )
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone, Default)]
pub struct Runtime {
    pub data: Arc<Mutex<Dashboard>>,
    pub cancel: Arc<AtomicBool>,
    pub forwarder: Arc<Mutex<Option<Forwarder>>>,
    pub recording: Arc<Mutex<Recording>>,
}
impl Runtime {
    pub fn snapshot(&self) -> Dashboard {
        self.data.lock().unwrap().clone()
    }
    pub fn frame(&self, frame: &[u8], live: bool) {
        // Only live Bluetooth/USB packets may be published. Imports and demos remain local.
        if live && let Some(f) = self.forwarder.lock().unwrap().as_ref() {
            f.publish(frame);
        }
        let received_ms = now_ms();
        if live {
            self.recording.lock().unwrap().push(received_ms, frame);
        }
        let parsed = decoder::decode_frame(frame, received_ms);
        let mut data = self.data.lock().unwrap();
        data.frames += 1;
        match parsed {
            Ok(item) => data.insert(item),
            Err(e) => {
                data.rejected += 1;
                data.last_error = e;
            }
        }
    }
    pub fn recording_stats(&self) -> (usize, usize, u64) {
        let recording = self.recording.lock().unwrap();
        (recording.frames.len(), recording.bytes, recording.evicted)
    }
    pub fn clear_recording(&self) {
        *self.recording.lock().unwrap() = Recording::default();
    }
    pub fn export_recording(&self) -> Result<String, String> {
        let pcap = {
            let recording = self.recording.lock().unwrap();
            if recording.frames.is_empty() {
                return Err("Noch keine Livepakete aufgezeichnet".into());
            }
            recording.pcap()
        };
        crate::export::save_pcap(&pcap, now_ms())
    }
    pub fn import(&self, bytes: &[u8], source: &str) -> Result<(), String> {
        if self.data.lock().unwrap().running {
            return Err("Live-Empfang zuerst trennen".into());
        }
        let (frames, errors) = import_frames(bytes)?;
        if frames.is_empty() {
            return Err(format!(
                "Keine Frames gefunden ({errors} fehlerhafte Zeilen)"
            ));
        }
        *self.data.lock().unwrap() = Dashboard {
            source: source.into(),
            status: "Import verarbeitet".into(),
            framing_errors: errors,
            ..Dashboard::default()
        };
        for f in frames {
            self.frame(&f, false);
        }
        Ok(())
    }
    pub fn demo(&self) -> Result<(), String> {
        if self.data.lock().unwrap().running {
            return Err("Live-Empfang zuerst trennen".into());
        }
        *self.data.lock().unwrap() = Dashboard {
            source: "DEMO · synthetische Pakete".into(),
            status: "Drei synthetische CAMs · keine echten Verkehrsdaten".into(),
            ..Dashboard::default()
        };
        for frame in crate::demo::frames() {
            self.frame(&frame, false);
        }
        Ok(())
    }
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.stop_forwarding();
    }
    pub fn stop_forwarding(&self) {
        if let Some(f) = self.forwarder.lock().unwrap().take() {
            f.stop();
        }
    }
    pub fn connect(&self) {
        self.connect_usb();
    }
    pub fn connect_usb(&self) {
        let mut data = self.data.lock().unwrap();
        if data.running {
            return;
        }
        #[cfg(not(target_os = "android"))]
        {
            data.status =
                "USB-OTG ist in der Android-App verfügbar; hier Log/PCAP importieren.".into();
        }
        #[cfg(target_os = "android")]
        {
            *data = Dashboard {
                running: true,
                source: "USB · ESP32-C5".into(),
                status: "Suche ESP32-C5…".into(),
                ..Dashboard::default()
            };
            self.cancel.store(false, Ordering::Relaxed);
            let this = self.clone();
            std::thread::spawn(move || {
                let result = crate::usb::receive(&this);
                this.stop_forwarding();
                let mut data = this.data.lock().unwrap();
                data.running = false;
                data.status = match result {
                    Ok(()) => "USB getrennt".into(),
                    Err(e) => format!("USB: {e}"),
                };
            });
        }
    }
    pub fn connect_ble(&self) {
        let mut data = self.data.lock().unwrap();
        if data.running {
            return;
        }
        #[cfg(not(target_os = "android"))]
        {
            data.status = "Bluetooth LE ist in der Android-App verfügbar.".into();
        }
        #[cfg(target_os = "android")]
        {
            *data = Dashboard {
                running: true,
                source: "Bluetooth LE · ESP32-C5".into(),
                status: "Suche CITS-G5 über Bluetooth…".into(),
                ..Dashboard::default()
            };
            self.cancel.store(false, Ordering::Relaxed);
            let this = self.clone();
            std::thread::spawn(move || {
                let result = crate::ble::receive(&this);
                this.stop_forwarding();
                let mut data = this.data.lock().unwrap();
                data.running = false;
                data.status = match result {
                    Ok(()) => "Bluetooth getrennt".into(),
                    Err(e) => format!("Bluetooth: {e}"),
                };
            });
        }
    }
    pub fn set_external_antenna(&self, external: bool) -> Result<(), String> {
        let data = self.data.lock().unwrap();
        if !data.running || !data.source.starts_with("Bluetooth LE") {
            return Err("Antennenumschaltung benötigt eine Bluetooth-Verbindung".into());
        }
        drop(data);
        #[cfg(target_os = "android")]
        {
            crate::ble::set_external_antenna(external)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = external;
            Err("Antennenumschaltung ist nur in der Android-App verfügbar".into())
        }
    }
    pub fn feed(&self, decoder: &mut LineDecoder, bytes: &[u8]) {
        let before = decoder.errors;
        for frame in decoder.feed(bytes) {
            self.frame(&frame, true);
        }
        self.data.lock().unwrap().framing_errors += decoder.errors - before;
    }
}
