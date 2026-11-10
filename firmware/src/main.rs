#![no_std]
#![no_main]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::{
    cell::RefCell,
    fmt::Write as _,
    sync::atomic::{AtomicU32, Ordering},
};
use critical_section::Mutex;
use crossbeam_queue::ArrayQueue;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    timer::timg::TimerGroup,
};
use esp32_cits_core::radio;

// Kanal 180 = 5900 MHz; ausschließlich Empfang, kein TX.
const ITS_CHANNEL: radio::Channel = 180;
const QUEUE_SIZE: usize = 16;
const MAX_FRAME: usize = 2304;

static RX_QUEUE: Mutex<RefCell<Option<ArrayQueue<Vec<u8>>>>> = Mutex::new(RefCell::new(None));
static CALLBACKS: AtomicU32 = AtomicU32::new(0);
static DROPPED: AtomicU32 = AtomicU32::new(0);

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {}", info);
    esp_hal::system::software_reset();
}

// Läuft im WLAN-Callback: Keine USB-Ausgabe, kein Parser und keine blockierende Operation.
fn on_frame(frame: esp_radio::wifi::sniffer::PromiscuousPkt<'_>) {
    CALLBACKS.fetch_add(1, Ordering::Relaxed);
    if frame.data.len() < 24 || frame.data.len() > MAX_FRAME {
        return;
    }
    // Nur 802.11-Datenframes: FCF Type = 2.
    if (frame.data[0] & 0x0c) != 0x08 {
        return;
    }
    critical_section::with(|cs| {
        if let Some(q) = RX_QUEUE.borrow(cs).borrow().as_ref() {
            // Queue ist endlich: Bei Überlast werden Pakete verworfen.
            if q.is_full() {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let mut bytes = Vec::new();
            if bytes.try_reserve_exact(frame.data.len()).is_err() {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                return;
            }
            bytes.extend_from_slice(frame.data);
            if q.push(bytes).is_err() {
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    esp_println::println!("CITS-BOOT waveshare-esp32-c5-zero channel=180 rx-only");

    let cfg = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(cfg);

    // Waveshare ESP32-C5-Zero: GPIO26 LOW=Keramikantenne, HIGH=IPEX1.
    // Für den ersten Test onboard, mit externem Antennenanschluss auf High setzen.
    let _antenna = Output::new(p.GPIO26, Level::Low, OutputConfig::default());

    // Kein PSRAM: Waveshare ESP32-C5-Zero besitzt 4MB Flash und internes RAM.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let tg = TimerGroup::new(p.TIMG0);
    let sw = esp_hal::interrupt::software::SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(tg.timer0, sw.software_interrupt0);

    critical_section::with(|cs| {
        RX_QUEUE
            .borrow(cs)
            .replace(Some(ArrayQueue::new(QUEUE_SIZE)))
    });

    let (mut controller, interfaces) =
        esp_radio::wifi::new(p.WIFI, esp_radio::wifi::ControllerConfig::default())
            .expect("wifi init failed");
    controller
        .set_band_mode(esp_radio::wifi::BandMode::_5G)
        .expect("5 GHz mode failed");
    let mut sniffer = interfaces.sniffer;
    radio::setup_wifi_sniffer(ITS_CHANNEL, &mut sniffer, on_frame)
        .expect("802.11p sniffer setup failed");

    esp_println::println!("CITS-READY channel=180 freq=5900MHz antenna=onboard");

    let mut received: u64 = 0;
    let mut line = String::with_capacity(5 + MAX_FRAME * 2);
    let mut last_stat = embassy_time::Instant::now();
    loop {
        let item = critical_section::with(|cs| {
            RX_QUEUE.borrow(cs).borrow().as_ref().and_then(|q| q.pop())
        });
        if let Some(bytes) = item {
            // ASCII-Line-Protocol: CITS:<hex bytes>\n; Host interpretiert nur diese Zeilen.
            // Im Gegensatz zu Proto-UART kein Adapter erforderlich (USB Serial/JTAG).
            line.clear();
            line.push_str("CITS:");
            for b in bytes {
                let _ = write!(&mut line, "{b:02x}");
            }
            esp_println::println!("{}", line);
            received += 1;
        }
        if last_stat.elapsed() >= embassy_time::Duration::from_secs(5) {
            esp_println::println!(
                "CITS-STAT packets={} callbacks={} dropped={} uptime_ms={}",
                received,
                CALLBACKS.load(Ordering::Relaxed),
                DROPPED.load(Ordering::Relaxed),
                embassy_time::Instant::now().as_millis()
            );
            last_stat = embassy_time::Instant::now();
        }
        // Auch bei voller Queue regelmäßig andere RTOS-/Embassy-Aufgaben ausführen.
        embassy_futures::yield_now().await;
        if critical_section::with(|cs| {
            RX_QUEUE
                .borrow(cs)
                .borrow()
                .as_ref()
                .is_none_or(|q| q.is_empty())
        }) {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(2)).await;
        }
    }
}
