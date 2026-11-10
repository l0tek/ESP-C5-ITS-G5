use crate::{
    decoder,
    forwarding::Forwarder,
    model::Dashboard,
    stream::{LineDecoder, import_frames},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

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
}
impl Runtime {
    pub fn snapshot(&self) -> Dashboard {
        let mut data = self.data.lock().unwrap();
        data.expire(now_ms());
        data.clone()
    }
    pub fn frame(&self, frame: &[u8], live: bool) {
        // Only live USB packets may be published. Imports and demos remain local.
        if live && let Some(f) = self.forwarder.lock().unwrap().as_ref() {
            f.publish(frame);
        }
        let parsed = decoder::decode_frame(frame, now_ms());
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
    pub fn import(&self, bytes: &[u8], source: &str) -> Result<(), String> {
        if self.data.lock().unwrap().running {
            return Err("USB-Empfang zuerst trennen".into());
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
            return Err("USB-Empfang zuerst trennen".into());
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
    pub fn feed(&self, decoder: &mut LineDecoder, bytes: &[u8]) {
        let before = decoder.errors;
        for frame in decoder.feed(bytes) {
            self.frame(&frame, true);
        }
        self.data.lock().unwrap().framing_errors += decoder.errors - before;
    }
}
