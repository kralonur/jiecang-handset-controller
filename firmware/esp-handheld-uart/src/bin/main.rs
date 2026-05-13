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
use jiecang_handset_controller::{
    Button, ButtonEvent, ButtonLines, ButtonReading, Controller, DisplayCommand, HandsetError,
    ProgramCommand,
};

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

    let mut controller = Controller::default();
    let mut previous_button_reading = controller.button_reading(button_inputs.lines());
    log_button_reading(previous_button_reading);

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
    let mut current_command = DisplayCommand::Reset;
    log_display_command(current_command);

    loop {
        let current_packet = controller.command(current_command);
        let mut written = 0;
        while written < current_packet.len() {
            written += handset_tx.write(&current_packet[written..]).unwrap_or(0);
        }
        handset_tx.flush().ok();

        let button_lines = button_inputs.lines();
        let button_reading = controller.button_reading(button_lines);
        if button_reading != previous_button_reading {
            log_button_reading(button_reading);
        }

        if reset_refreshes_remaining > 0 {
            reset_refreshes_remaining -= 1;

            if reset_refreshes_remaining == 0 {
                current_command = DisplayCommand::Height(current_height_mm);
                log_display_command(current_command);
            }
        } else if let Some(button_event) = controller.update_buttons(button_lines) {
            if let ButtonEvent::Pressed(button) = button_event {
                held_adjust_ticks = 0;
                handle_button_press(
                    button,
                    &mut current_height_mm,
                    &mut memories,
                    &mut save_armed,
                    &mut current_command,
                );
            }
        } else if matches!(
            button_reading,
            ButtonReading::Button(Button::Up | Button::Down)
        ) {
            held_adjust_ticks += 1;

            if held_adjust_ticks >= HELD_ADJUST_TICKS {
                held_adjust_ticks = 0;
                adjust_height(button_reading, &mut current_height_mm, &mut current_command);
            }
        } else {
            held_adjust_ticks = 0;
        }

        previous_button_reading = button_reading;

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
    fn lines(&self) -> ButtonLines {
        ButtonLines {
            pin2_low: self.pin2.is_low(),
            pin3_low: self.pin3.is_low(),
            pin4_low: self.pin4.is_low(),
            pin5_low: self.pin5.is_low(),
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
    fn from_button(button: Button) -> Option<Self> {
        match button {
            Button::M1 => Some(Self::M1),
            Button::M2 => Some(Self::M2),
            Button::M3 => Some(Self::M3),
            Button::M4 => Some(Self::M4),
            Button::Up | Button::Down | Button::Rec => None,
        }
    }

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

fn handle_button_press(
    button: Button,
    current_height_mm: &mut u16,
    memories: &mut [Option<u16>; MEMORY_SLOT_COUNT],
    save_armed: &mut bool,
    current_command: &mut DisplayCommand,
) {
    match button {
        Button::Up | Button::Down => {
            adjust_height(
                ButtonReading::Button(button),
                current_height_mm,
                current_command,
            );
        }
        Button::Rec => {
            *save_armed = !*save_armed;
            if *save_armed {
                *current_command = DisplayCommand::Program(ProgramCommand::Pending);
            } else {
                *current_command = DisplayCommand::Height(*current_height_mm);
            }
            log_save_armed(*save_armed);
            log_display_command(*current_command);
        }
        Button::M1 | Button::M2 | Button::M3 | Button::M4 => {
            if let Some(slot) = MemorySlot::from_button(button) {
                let slot_index = slot.index();

                if *save_armed {
                    memories[slot_index] = Some(*current_height_mm);
                    *save_armed = false;
                    *current_command = DisplayCommand::Program(slot.program_command());
                    log_memory_saved(slot, *current_height_mm);
                    log_display_command(*current_command);
                } else if let Some(saved_height_mm) = memories[slot_index] {
                    *current_height_mm = saved_height_mm;
                    *current_command = DisplayCommand::Height(*current_height_mm);
                    log_memory_recalled(slot, *current_height_mm);
                    log_display_command(*current_command);
                } else {
                    *current_command = DisplayCommand::Error(slot.empty_error());
                    log_memory_empty(slot);
                    log_display_command(*current_command);
                }
            }
        }
    }
}

fn adjust_height(
    button_reading: ButtonReading,
    current_height_mm: &mut u16,
    current_command: &mut DisplayCommand,
) {
    let next_height_mm = match button_reading {
        ButtonReading::Button(Button::Up) => increase_height(*current_height_mm),
        ButtonReading::Button(Button::Down) => decrease_height(*current_height_mm),
        ButtonReading::None
        | ButtonReading::Button(Button::M1 | Button::M2 | Button::M3 | Button::M4 | Button::Rec)
        | ButtonReading::Unknown(_) => *current_height_mm,
    };

    if next_height_mm != *current_height_mm {
        *current_height_mm = next_height_mm;
        *current_command = DisplayCommand::Height(*current_height_mm);
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

fn log_display_command(command: DisplayCommand) {
    let packet = command.packet();

    match command {
        DisplayCommand::Reset => {
            info!(
                "display reset packet={:02x} {:02x} {:02x} {:02x}",
                packet[0], packet[1], packet[2], packet[3]
            );
        }
        DisplayCommand::Height(height_mm) => {
            info!(
                "display height={} packet={:02x} {:02x} {:02x} {:02x}",
                height_mm, packet[0], packet[1], packet[2], packet[3]
            );
        }
        DisplayCommand::Error(error) => {
            info!(
                "display error=E{} packet={:02x} {:02x} {:02x} {:02x}",
                error.code(),
                packet[0],
                packet[1],
                packet[2],
                packet[3]
            );
        }
        DisplayCommand::Program(program) => {
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

fn log_button_reading(reading: ButtonReading) {
    match reading {
        ButtonReading::None => {
            info!("buttons none");
        }
        ButtonReading::Button(Button::Up) => {
            info!("button UP");
        }
        ButtonReading::Button(Button::Down) => {
            info!("button DOWN");
        }
        ButtonReading::Button(Button::M1) => {
            info!("button M1");
        }
        ButtonReading::Button(Button::M2) => {
            info!("button M2");
        }
        ButtonReading::Button(Button::M3) => {
            info!("button M3");
        }
        ButtonReading::Button(Button::M4) => {
            info!("button M4");
        }
        ButtonReading::Button(Button::Rec) => {
            info!("button REC");
        }
        ButtonReading::Unknown(raw_low_mask) => {
            info!("button unknown raw_low_mask=0b{:04b}", raw_low_mask);
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
