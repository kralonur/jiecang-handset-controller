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

    let mut current_height_mm = START_HEIGHT_MM;
    let mut memories = [None; MEMORY_SLOT_COUNT];
    let mut save_armed = false;
    let mut reset_refreshes_remaining = RESET_REFRESH_TICKS;
    let mut held_adjust_ticks = 0u8;
    let mut current_command = HandsetCommand::Reset;
    log_display_command(current_command);

    loop {
        let current_packet = current_command.packet();
        let mut written = 0;
        while written < current_packet.len() {
            written += handset_tx.write(&current_packet[written..]).unwrap_or(0);
        }
        handset_tx.flush().ok();

        let button_state = button_inputs.read();
        if button_state != previous_button_state {
            log_button_state(button_state);
        }

        if reset_refreshes_remaining > 0 {
            reset_refreshes_remaining -= 1;

            if reset_refreshes_remaining == 0 {
                current_command = HandsetCommand::Height(current_height_mm);
                log_display_command(current_command);
            }
        } else if button_state.button != previous_button_state.button {
            held_adjust_ticks = 0;

            if button_state.button != HandsetButton::None {
                handle_button_press(
                    button_state.button,
                    &mut current_height_mm,
                    &mut memories,
                    &mut save_armed,
                    &mut current_command,
                );
            }
        } else if matches!(button_state.button, HandsetButton::Up | HandsetButton::Down) {
            held_adjust_ticks += 1;

            if held_adjust_ticks >= HELD_ADJUST_TICKS {
                held_adjust_ticks = 0;
                adjust_height(
                    button_state.button,
                    &mut current_height_mm,
                    &mut current_command,
                );
            }
        } else {
            held_adjust_ticks = 0;
        }

        previous_button_state = button_state;

        Timer::after(Duration::from_millis(50)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

const START_HEIGHT_MM: u16 = 700;
const MIN_HEIGHT_MM: u16 = 600;
const MAX_HEIGHT_MM: u16 = 1300;
const HEIGHT_STEP_MM: u16 = 10;
const MEMORY_SLOT_COUNT: usize = 4;
const RESET_REFRESH_TICKS: u16 = 20;
const HELD_ADJUST_TICKS: u8 = 4;

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

    fn memory_slot(self) -> Option<MemorySlot> {
        match self {
            Self::M1 => Some(MemorySlot::M1),
            Self::M2 => Some(MemorySlot::M2),
            Self::M3 => Some(MemorySlot::M3),
            Self::M4 => Some(MemorySlot::M4),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum MemorySlot {
    M1,
    M2,
    M3,
    M4,
}

impl MemorySlot {
    fn index(self) -> usize {
        match self {
            Self::M1 => 0,
            Self::M2 => 1,
            Self::M3 => 2,
            Self::M4 => 3,
        }
    }

    fn program_command(self) -> ProgramCommand {
        match self {
            Self::M1 => ProgramCommand::Preset1,
            Self::M2 => ProgramCommand::Preset2,
            Self::M3 => ProgramCommand::Preset3,
            Self::M4 => ProgramCommand::Preset4,
        }
    }

    fn empty_error(self) -> HandsetError {
        match self {
            Self::M1 => HandsetError::E01,
            Self::M2 => HandsetError::E02,
            Self::M3 => HandsetError::E03,
            Self::M4 => HandsetError::E04,
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
enum HandsetError {
    E01,
    E02,
    E03,
    E04,
}

impl HandsetError {
    fn arg0(self) -> u8 {
        match self {
            Self::E01 => 0x01,
            Self::E02 => 0x02,
            Self::E03 => 0x04,
            Self::E04 => 0x08,
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::E01 => 1,
            Self::E02 => 2,
            Self::E03 => 3,
            Self::E04 => 4,
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
            Self::Pending => 0x00,
            Self::Preset1 => 0x01,
            Self::Preset2 => 0x02,
            Self::Preset3 => 0x04,
            Self::Preset4 => 0x08,
        }
    }
}

fn handle_button_press(
    button: HandsetButton,
    current_height_mm: &mut u16,
    memories: &mut [Option<u16>; MEMORY_SLOT_COUNT],
    save_armed: &mut bool,
    current_command: &mut HandsetCommand,
) {
    match button {
        HandsetButton::Up | HandsetButton::Down => {
            adjust_height(button, current_height_mm, current_command);
        }
        HandsetButton::Rec => {
            *save_armed = !*save_armed;
            if *save_armed {
                *current_command = HandsetCommand::Program(ProgramCommand::Pending);
            } else {
                *current_command = HandsetCommand::Height(*current_height_mm);
            }
            log_save_armed(*save_armed);
            log_display_command(*current_command);
        }
        _ => {
            if let Some(slot) = button.memory_slot() {
                let slot_index = slot.index();

                if *save_armed {
                    memories[slot_index] = Some(*current_height_mm);
                    *save_armed = false;
                    *current_command = HandsetCommand::Program(slot.program_command());
                    log_memory_saved(slot, *current_height_mm);
                    log_display_command(*current_command);
                } else if let Some(saved_height_mm) = memories[slot_index] {
                    *current_height_mm = saved_height_mm;
                    *current_command = HandsetCommand::Height(*current_height_mm);
                    log_memory_recalled(slot, *current_height_mm);
                    log_display_command(*current_command);
                } else {
                    *current_command = HandsetCommand::Error(slot.empty_error());
                    log_memory_empty(slot);
                    log_display_command(*current_command);
                }
            }
        }
    }
}

fn adjust_height(
    button: HandsetButton,
    current_height_mm: &mut u16,
    current_command: &mut HandsetCommand,
) {
    let next_height_mm = match button {
        HandsetButton::Up => increase_height(*current_height_mm),
        HandsetButton::Down => decrease_height(*current_height_mm),
        _ => *current_height_mm,
    };

    if next_height_mm != *current_height_mm {
        *current_height_mm = next_height_mm;
        *current_command = HandsetCommand::Height(*current_height_mm);
        log_display_command(*current_command);
    }
}

fn increase_height(height_mm: u16) -> u16 {
    if height_mm > MAX_HEIGHT_MM - HEIGHT_STEP_MM {
        MAX_HEIGHT_MM
    } else {
        height_mm + HEIGHT_STEP_MM
    }
}

fn decrease_height(height_mm: u16) -> u16 {
    if height_mm < MIN_HEIGHT_MM + HEIGHT_STEP_MM {
        MIN_HEIGHT_MM
    } else {
        height_mm - HEIGHT_STEP_MM
    }
}

fn log_display_command(command: HandsetCommand) {
    let packet = command.packet();

    match command {
        HandsetCommand::Reset => {
            info!(
                "display reset packet={:02x} {:02x} {:02x} {:02x}",
                packet[0], packet[1], packet[2], packet[3]
            );
        }
        HandsetCommand::Height(height_mm) => {
            info!(
                "display height={} packet={:02x} {:02x} {:02x} {:02x}",
                height_mm, packet[0], packet[1], packet[2], packet[3]
            );
        }
        HandsetCommand::Error(error) => {
            info!(
                "display error=E{} packet={:02x} {:02x} {:02x} {:02x}",
                error.code(),
                packet[0],
                packet[1],
                packet[2],
                packet[3]
            );
        }
        HandsetCommand::Program(program) => {
            info!(
                "display program arg0=0x{:02x} packet={:02x} {:02x} {:02x} {:02x}",
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

fn log_save_armed(save_armed: bool) {
    if save_armed {
        info!("memory save armed");
    } else {
        info!("memory save cancelled");
    }
}

fn log_memory_saved(slot: MemorySlot, height_mm: u16) {
    match slot {
        MemorySlot::M1 => info!("saved M1 height={}", height_mm),
        MemorySlot::M2 => info!("saved M2 height={}", height_mm),
        MemorySlot::M3 => info!("saved M3 height={}", height_mm),
        MemorySlot::M4 => info!("saved M4 height={}", height_mm),
    }
}

fn log_memory_recalled(slot: MemorySlot, height_mm: u16) {
    match slot {
        MemorySlot::M1 => info!("recalled M1 height={}", height_mm),
        MemorySlot::M2 => info!("recalled M2 height={}", height_mm),
        MemorySlot::M3 => info!("recalled M3 height={}", height_mm),
        MemorySlot::M4 => info!("recalled M4 height={}", height_mm),
    }
}

fn log_memory_empty(slot: MemorySlot) {
    match slot {
        MemorySlot::M1 => info!("M1 memory empty"),
        MemorySlot::M2 => info!("M2 memory empty"),
        MemorySlot::M3 => info!("M3 memory empty"),
        MemorySlot::M4 => info!("M4 memory empty"),
    }
}
