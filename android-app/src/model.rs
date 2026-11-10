use serde::Serialize;
use std::collections::BTreeMap;

pub const MAX_OBJECTS: usize = 512;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Observation {
    pub key: String,
    pub station_id: u32,
    pub kind: String,
    pub latitude: f64,
    pub longitude: f64,
    pub speed_kmh: Option<f64>,
    pub heading: Option<f64>,
    pub station_type: u8,
    pub terminated: bool,
    pub received_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Dashboard {
    pub objects: BTreeMap<String, Observation>,
    pub frames: u64,
    pub decoded: u64,
    pub rejected: u64,
    pub framing_errors: u64,
    pub source: String,
    pub status: String,
    pub last_error: String,
    pub running: bool,
    pub revision: u64,
}
impl Default for Dashboard {
    fn default() -> Self {
        Self {
            objects: BTreeMap::new(),
            frames: 0,
            decoded: 0,
            rejected: 0,
            framing_errors: 0,
            source: "Keine Quelle".into(),
            status: "ESP32-C5 über USB-OTG verbinden".into(),
            last_error: String::new(),
            running: false,
            revision: 0,
        }
    }
}
impl Dashboard {
    pub fn insert(&mut self, item: Observation) {
        self.decoded += 1;
        if item.terminated {
            self.objects.remove(&item.key);
        } else {
            if self.objects.len() >= MAX_OBJECTS && !self.objects.contains_key(&item.key) {
                let oldest = self
                    .objects
                    .iter()
                    .min_by_key(|(_, o)| o.received_ms)
                    .map(|(k, _)| k.clone());
                if let Some(k) = oldest {
                    self.objects.remove(&k);
                }
            }
            self.objects.insert(item.key.clone(), item);
        }
        self.revision += 1;
    }
    pub fn expire(&mut self, now_ms: u64) {
        let before = self.objects.len();
        self.objects.retain(|_, o| {
            now_ms.saturating_sub(o.received_ms) < if o.kind == "CAM" { 60_000 } else { 300_000 }
        });
        if before != self.objects.len() {
            self.revision += 1;
        }
    }
}
