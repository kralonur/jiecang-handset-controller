#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config as UartConfig, UartTx};
use esp_println as _;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32c6 -o esp32c6-mini-1 -o unstable-hal -o embassy -o defmt -o esp-backtrace -o vscode

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO4
    // - GPIO5
    // - GPIO8
    // - GPIO9
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO24;
    let _ = peripherals.GPIO25;
    let _ = peripherals.GPIO26;
    let _ = peripherals.GPIO27;
    let _ = peripherals.GPIO28;
    let _ = peripherals.GPIO29;
    let _ = peripherals.GPIO30;

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut handset_tx = UartTx::new(peripherals.UART1, UartConfig::default().with_baudrate(9_600))
        .expect("failed to configure UART1")
        .with_tx(peripherals.GPIO16);

    let mut test_step = 0u16;
    let mut refresh_count = 0u32;
    let mut current_packet = test_packet(test_step);
    log_test_step(test_step, current_packet);

    loop {
        let mut written = 0;
        while written < current_packet.len() {
            written += handset_tx.write(&current_packet[written..]).unwrap_or(0);
        }
        handset_tx.flush().ok();

        refresh_count += 1;
        if refresh_count >= 20 {
            refresh_count = 0;
            test_step = (test_step + 1) % TEST_STEP_COUNT;
            current_packet = test_packet(test_step);
            log_test_step(test_step, current_packet);
        }

        Timer::after(Duration::from_millis(50)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

fn height_packet(height_mm: u16) -> [u8; 4] {
    let [hi, lo] = height_mm.to_be_bytes();
    [0x01, 0x01, hi, lo]
}

const ERROR_STEP_COUNT: u16 = 16;
const HEIGHT_STEP_COUNT: u16 = 30;
const TEST_STEP_COUNT: u16 = 1 + ERROR_STEP_COUNT + HEIGHT_STEP_COUNT;

fn test_packet(step: u16) -> [u8; 4] {
    if step == 0 {
        return [0x01, 0x04, 0x01, 0xaa];
    }

    let error_step = step - 1;
    if error_step < ERROR_STEP_COUNT {
        return [0x01, 0x02, (error_step as u8) << 4, 0x00];
    }

    let height_step = error_step - ERROR_STEP_COUNT + 1;
    height_packet(height_step * 10)
}

fn log_test_step(step: u16, packet: [u8; 4]) {
    if step == 0 {
        info!(
            "test reset packet={:02x} {:02x} {:02x} {:02x}",
            packet[0], packet[1], packet[2], packet[3]
        );
    } else if step <= ERROR_STEP_COUNT {
        info!(
            "test error arg0=0x{:02x} packet={:02x} {:02x} {:02x} {:02x}",
            packet[2], packet[0], packet[1], packet[2], packet[3]
        );
    } else {
        let height = (step - ERROR_STEP_COUNT) * 10;
        info!(
            "test height={} packet={:02x} {:02x} {:02x} {:02x}",
            height, packet[0], packet[1], packet[2], packet[3]
        );
    }
}
