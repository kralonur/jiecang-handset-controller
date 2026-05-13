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
use esp_hal::gpio::{Input, InputConfig, Pull};
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

    let button_input_config = InputConfig::default().with_pull(Pull::Up);
    let handset_pin2 = Input::new(peripherals.GPIO2, button_input_config);
    let handset_pin3 = Input::new(peripherals.GPIO3, button_input_config);
    let handset_pin4 = Input::new(peripherals.GPIO4, button_input_config);
    let handset_pin5 = Input::new(peripherals.GPIO5, button_input_config);
    let button_inputs = HandsetButtonInputs {
        pin2: handset_pin2,
        pin3: handset_pin3,
        pin4: handset_pin4,
        pin5: handset_pin5,
    };
    let mut previous_button_state = button_inputs.read();
    log_button_state(previous_button_state);

    let mut handset_tx = UartTx::new(
        peripherals.UART1,
        UartConfig::default().with_baudrate(9_600),
    )
    .expect("failed to configure UART1")
    .with_tx(peripherals.GPIO16);

    let mut test_step = 0u16;
    let mut refresh_count = 0u32;
    let mut current_command = test_command(test_step);
    let mut current_packet = current_command.packet();
    log_test_step(current_command, current_packet);

    loop {
        let mut written = 0;
        while written < current_packet.len() {
            written += handset_tx.write(&current_packet[written..]).unwrap_or(0);
        }
        handset_tx.flush().ok();

        let button_state = button_inputs.read();
        if button_state != previous_button_state {
            log_button_state(button_state);
            previous_button_state = button_state;
        }

        refresh_count += 1;
        if refresh_count >= 20 {
            refresh_count = 0;
            test_step = (test_step + 1) % TEST_STEP_COUNT;
            current_command = test_command(test_step);
            current_packet = current_command.packet();
            log_test_step(current_command, current_packet);
        }

        Timer::after(Duration::from_millis(50)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

const ERROR_STEP_COUNT: u16 = 8;
const PROGRAM_STEP_COUNT: u16 = 5;
const HEIGHT_STEP_COUNT: u16 = 30;
const TEST_STEP_COUNT: u16 = 1 + ERROR_STEP_COUNT + PROGRAM_STEP_COUNT + HEIGHT_STEP_COUNT;

struct HandsetButtonInputs<'d> {
    pin2: Input<'d>,
    pin3: Input<'d>,
    pin4: Input<'d>,
    pin5: Input<'d>,
}

impl HandsetButtonInputs<'_> {
    fn read(&self) -> HandsetButtonState {
        let mut low_mask = 0u8;

        if self.pin2.is_low() {
            low_mask |= 0b0001;
        }
        if self.pin3.is_low() {
            low_mask |= 0b0010;
        }
        if self.pin4.is_low() {
            low_mask |= 0b0100;
        }
        if self.pin5.is_low() {
            low_mask |= 0b1000;
        }

        HandsetButtonState {
            low_mask,
            button: HandsetButton::decode(low_mask),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HandsetButtonState {
    low_mask: u8,
    button: HandsetButton,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandsetButton {
    None,
    Up,
    Down,
    M1,
    M2,
    M3,
    M4,
    Rec,
    Unknown,
}

impl HandsetButton {
    fn decode(low_mask: u8) -> Self {
        match low_mask {
            0b0000 => Self::None,
            0b1000 => Self::Up,
            0b0100 => Self::Down,
            0b1100 => Self::M1,
            0b0010 => Self::M2,
            0b0110 => Self::M3,
            0b1010 => Self::M4,
            0b0001 => Self::Rec,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy)]
enum HandsetCommand {
    Reset,
    Height(u16),
    Error(HandsetError),
    Program(ProgramCommand),
}

impl HandsetCommand {
    fn packet(self) -> [u8; 4] {
        match self {
            Self::Reset => [0x01, 0x04, 0x01, 0xaa],
            Self::Height(height_mm) => {
                let [hi, lo] = height_mm.to_be_bytes();
                [0x01, 0x01, hi, lo]
            }
            Self::Error(error) => [0x01, 0x02, error.arg0(), 0x00],
            Self::Program(program) => [0x01, 0x06, program.arg0(), 0x00],
        }
    }
}

#[derive(Clone, Copy)]
enum ProgramCommand {
    Pending,
    Preset1,
    Preset2,
    Preset3,
    Preset4,
}

impl ProgramCommand {
    fn arg0(self) -> u8 {
        match self {
            Self::Pending => 0x00, // 01 06 00 00
            Self::Preset1 => 0x01, // 01 06 01 00
            Self::Preset2 => 0x02, // 01 06 02 00
            Self::Preset3 => 0x04, // 01 06 04 00
            Self::Preset4 => 0x08, // 01 06 08 00
        }
    }
}

#[derive(Clone, Copy)]
enum HandsetError {
    E01,
    E02,
    E03,
    E04,
    E05,
    E06,
    E07,
    E08,
}

impl HandsetError {
    fn arg0(self) -> u8 {
        match self {
            Self::E01 => 0x01,
            Self::E02 => 0x02,
            Self::E03 => 0x04,
            Self::E04 => 0x08,
            Self::E05 => 0x10,
            Self::E06 => 0x20,
            Self::E07 => 0x40,
            Self::E08 => 0x80,
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::E01 => 1,
            Self::E02 => 2,
            Self::E03 => 3,
            Self::E04 => 4,
            Self::E05 => 5,
            Self::E06 => 6,
            Self::E07 => 7,
            Self::E08 => 8,
        }
    }
}

fn test_command(step: u16) -> HandsetCommand {
    if step == 0 {
        return HandsetCommand::Reset;
    }

    let error_step = step - 1;
    if error_step < ERROR_STEP_COUNT {
        return HandsetCommand::Error(match error_step {
            0 => HandsetError::E01,
            1 => HandsetError::E02,
            2 => HandsetError::E03,
            3 => HandsetError::E04,
            4 => HandsetError::E05,
            5 => HandsetError::E06,
            6 => HandsetError::E07,
            _ => HandsetError::E08,
        });
    }

    let program_step = error_step - ERROR_STEP_COUNT;
    if program_step < PROGRAM_STEP_COUNT {
        return HandsetCommand::Program(match program_step {
            0 => ProgramCommand::Pending,
            1 => ProgramCommand::Preset1,
            2 => ProgramCommand::Preset2,
            3 => ProgramCommand::Preset3,
            _ => ProgramCommand::Preset4,
        });
    }

    let height_step = program_step - PROGRAM_STEP_COUNT + 1;
    HandsetCommand::Height(height_step * 10)
}

fn log_test_step(command: HandsetCommand, packet: [u8; 4]) {
    match command {
        HandsetCommand::Reset => {
            info!(
                "test reset packet={:02x} {:02x} {:02x} {:02x}",
                packet[0], packet[1], packet[2], packet[3]
            );
        }
        HandsetCommand::Height(height_mm) => {
            info!(
                "test height={} packet={:02x} {:02x} {:02x} {:02x}",
                height_mm, packet[0], packet[1], packet[2], packet[3]
            );
        }
        HandsetCommand::Error(error) => {
            info!(
                "test error=E{} packet={:02x} {:02x} {:02x} {:02x}",
                error.code(),
                packet[0],
                packet[1],
                packet[2],
                packet[3]
            );
        }
        HandsetCommand::Program(program) => {
            info!(
                "test program arg0=0x{:02x} packet={:02x} {:02x} {:02x} {:02x}",
                program.arg0(),
                packet[0],
                packet[1],
                packet[2],
                packet[3]
            );
        }
    }
}

fn log_button_state(state: HandsetButtonState) {
    match state.button {
        HandsetButton::None => {
            info!("buttons none raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::Up => {
            info!("button UP raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::Down => {
            info!("button DOWN raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::M1 => {
            info!("button M1 raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::M2 => {
            info!("button M2 raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::M3 => {
            info!("button M3 raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::M4 => {
            info!("button M4 raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::Rec => {
            info!("button REC raw_low_mask=0b{:04b}", state.low_mask);
        }
        HandsetButton::Unknown => {
            info!("button unknown raw_low_mask=0b{:04b}", state.low_mask);
        }
    }
}
