use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_futures::{join::join, select::select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_radio::ble::controller::BleConnector;
use heapless::Vec as HeaplessVec;
use trouble_host::prelude::*;

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;
const NOTIFY_CHUNK: usize = 240;

static TX: Channel<CriticalSectionRawMutex, Vec<u8>, 4> = Channel::new();
static CONNECTED: AtomicBool = AtomicBool::new(false);
static BLE_DROPPED: AtomicU32 = AtomicU32::new(0);

#[gatt_server]
struct Server {
    stream: StreamService,
}

#[gatt_service(uuid = "c1750001-6c6f-7465-6b2d-636974736735")]
struct StreamService {
    #[characteristic(uuid = "c1750002-6c6f-7465-6b2d-636974736735", read, notify)]
    data: HeaplessVec<u8, NOTIFY_CHUNK>,
}

pub fn enqueue(line: &[u8]) {
    if !CONNECTED.load(Ordering::Relaxed) {
        return;
    }
    if TX.try_send(line.to_vec()).is_err() {
        BLE_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn dropped() -> u32 {
    BLE_DROPPED.load(Ordering::Relaxed)
}

#[embassy_executor::task]
pub async fn run(controller: ExternalController<BleConnector<'static>, 1>) {
    let address = Address::random([0xc5, 0x17, 0x5a, 0x6b, 0x2d, 0x01]);
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let builder = trouble_host::new(controller, &mut resources);
    let builder = builder.set_random_address(address);
    let host = builder.build();
    let Host {
        mut peripheral,
        mut runner,
        ..
    } = host;
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "CITS-G5",
        appearance: &appearance::sensor::GENERIC_SENSOR,
    }))
    .expect("BLE GATT server");

    let _ = join(runner.run(), async {
        loop {
            let mut adv_data = [0; 31];
            // The 128-bit UUID is advertised so Android can scan without location data.
            let uuid = u128::from_be_bytes([
                0xc1, 0x75, 0x00, 0x01, 0x6c, 0x6f, 0x74, 0x65, 0x6b, 0x2d, 0x63, 0x69, 0x74, 0x73,
                0x67, 0x35,
            ]);
            let len = AdStructure::encode_slice(
                &[
                    AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                    AdStructure::ServiceUuids128(&[uuid.to_le_bytes()]),
                ],
                &mut adv_data,
            )
            .expect("BLE advertisement");
            let advertiser = peripheral
                .advertise(
                    &Default::default(),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &adv_data[..len],
                        scan_data: &[],
                    },
                )
                .await
                .expect("BLE advertise");
            match advertiser
                .accept()
                .await
                .and_then(|a| a.with_attribute_server(&server))
            {
                Ok(conn) => {
                    CONNECTED.store(true, Ordering::Relaxed);
                    esp_println::println!("CITS-BLE connected");
                    let _ = select(gatt_events(&conn), notifications(&server, &conn)).await;
                    CONNECTED.store(false, Ordering::Relaxed);
                    while TX.try_receive().is_ok() {}
                    esp_println::println!("CITS-BLE disconnected");
                }
                Err(error) => esp_println::println!("CITS-BLE accept error: {:?}", error),
            }
        }
    })
    .await;
}

async fn gatt_events<P: PacketPool>(conn: &GattConnection<'_, '_, P>) {
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { .. } => return,
            GattConnectionEvent::Gatt { event } => {
                if let Ok(reply) = event.accept() {
                    reply.send().await;
                }
            }
            _ => {}
        }
    }
}

async fn notifications<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    loop {
        let line = TX.receive().await;
        for bytes in line.chunks(NOTIFY_CHUNK) {
            let mut chunk = HeaplessVec::new();
            if chunk.extend_from_slice(bytes).is_err()
                || server.stream.data.notify(conn, &chunk).await.is_err()
            {
                return;
            }
        }
    }
}
