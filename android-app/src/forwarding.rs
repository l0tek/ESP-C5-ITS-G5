//! OpenTrafficMap wire format: raw IEEE 802.11 bytes on its/<node-id>/packet.
use rumqttc::{AsyncClient, Event, MqttOptions, Outgoing, QoS, TlsConfiguration, Transport};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

pub const BROKER: &str = "cits1.opentrafficmap.org";
pub fn packet_topic(node_id: &str) -> Result<String, String> {
    if node_id.is_empty()
        || node_id.len() > 64
        || !node_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err("Node-ID: 1–64 Zeichen, nur Buchstaben, Ziffern, - _ :".into());
    }
    Ok(format!("its/{node_id}/packet"))
}
#[derive(Clone)]
pub struct Forwarder {
    client: AsyncClient,
    topic: String,
    stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub sent: Arc<AtomicU64>,
    pub dropped: Arc<AtomicU64>,
}
impl Forwarder {
    pub fn start(node_id: &str) -> Result<Self, String> {
        let topic = packet_topic(node_id)?;
        let mut opts = MqttOptions::new(
            format!("citsmap-{node_id}-{}", crate::runtime::now_ms()),
            BROKER,
            8883,
        );
        opts.set_keep_alive(Duration::from_secs(20));
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
        opts.set_transport(Transport::tls_with_config(TlsConfiguration::Rustls(
            Arc::new(config),
        )));
        Self::spawn(topic, opts)
    }
    fn spawn(topic: String, opts: MqttOptions) -> Result<Self, String> {
        let (client, mut events) = AsyncClient::new(opts, 64);
        let this = Self {
            client,
            topic,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new("Verbinde TLS…".into())),
            sent: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
        };
        let worker = this.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    *worker.status.lock().unwrap() = e.to_string();
                    return;
                }
            };
            rt.block_on(async {
                loop {
                    if worker.stop.load(Ordering::Relaxed) { break; }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {},
                        event = events.poll() => match event {
                            Ok(Event::Incoming(rumqttc::Packet::ConnAck(_))) => *worker.status.lock().unwrap() = "OpenTrafficMap verbunden".into(),
                            Ok(Event::Outgoing(Outgoing::Publish(_))) => { worker.sent.fetch_add(1,Ordering::Relaxed); },
                            Ok(_) => {},
                            Err(e) => {
                                *worker.status.lock().unwrap() = format!("MQTT: {e}");
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                // Avoid replaying stale packets after reconnect.
                                break;
                            }
                        }
                    }
                }
                worker.stop.store(true,Ordering::Relaxed);
            });
        });
        Ok(this)
    }
    pub fn publish(&self, frame: &[u8]) {
        if self.stop.load(Ordering::Relaxed)
            || self
                .client
                .try_publish(&self.topic, QoS::AtMostOnce, false, frame)
                .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    fn mqtt_packet(stream: &mut std::net::TcpStream) -> (u8, Vec<u8>) {
        let mut first = [0];
        stream.read_exact(&mut first).unwrap();
        let (mut size, mut shift) = (0usize, 0);
        loop {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            size |= usize::from(b[0] & 127) << shift;
            if b[0] & 128 == 0 {
                break;
            }
            shift += 7;
            assert!(shift <= 21);
        }
        let mut payload = vec![0; size];
        stream.read_exact(&mut payload).unwrap();
        (first[0], payload)
    }
    #[test]
    fn live_usb_is_forwarded_as_raw_bytes_but_demo_and_import_are_not() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let receiver = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            assert_eq!(mqtt_packet(&mut socket).0, 0x10);
            socket.write_all(&[0x20, 2, 0, 0]).unwrap();
            ready_tx.send(()).unwrap();
            let (kind, body) = mqtt_packet(&mut socket);
            assert_eq!(kind, 0x30);
            let n = u16::from_be_bytes([body[0], body[1]]) as usize;
            assert_eq!(&body[2..2 + n], b"its/test-node/packet");
            body[2 + n..].to_vec()
        });
        let opts = MqttOptions::new("local-test", "127.0.0.1", port);
        let f = Forwarder::spawn(packet_topic("test-node").unwrap(), opts).unwrap();
        let runtime = crate::runtime::Runtime::default();
        *runtime.forwarder.lock().unwrap() = Some(f);
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        runtime.demo().unwrap();
        let imported = crate::demo::cam_frame(12, 0, 0);
        let hex: String = imported.iter().map(|b| format!("{b:02x}")).collect();
        runtime
            .import(format!("CITS:{hex}\n").as_bytes(), "test import")
            .unwrap();
        let live = crate::demo::cam_frame(9000, 0, 0);
        runtime.frame(&live, true);
        assert_eq!(receiver.join().unwrap(), live);
        runtime.stop_forwarding();
    }
}
