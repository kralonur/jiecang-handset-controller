#![no_std]

use core::marker::PhantomData;

pub type FullController = Controller<WithDisplay, WithButtons>;
pub type DisplayController = Controller<WithDisplay, WithoutButtons>;
pub type ButtonsController = Controller<WithoutDisplay, WithButtons>;

#[derive(Debug, Default, Clone, Copy)]
pub struct WithDisplay;

#[derive(Debug, Default, Clone, Copy)]
pub struct WithoutDisplay;

#[derive(Debug, Default, Clone, Copy)]
pub struct WithButtons;

#[derive(Debug, Default, Clone, Copy)]
pub struct WithoutButtons;

#[derive(Debug, Clone, Copy)]
pub struct Controller<Display = WithDisplay, Buttons = WithButtons> {
    button_tracker: ButtonTracker,
    _display: PhantomData<Display>,
    _buttons: PhantomData<Buttons>,
}

impl Controller<WithDisplay, WithButtons> {
    pub fn builder() -> ControllerBuilder<WithoutDisplay, WithoutButtons> {
        ControllerBuilder::default()
    }

    pub fn display_only() -> DisplayController {
        Controller::builder().with_display().build()
    }

    pub fn buttons_only() -> ButtonsController {
        Controller::builder().with_buttons().build()
    }
}

impl Default for Controller<WithDisplay, WithButtons> {
    fn default() -> Self {
        Controller::builder().with_display().with_buttons().build()
    }
}

impl<D, B> Controller<D, B> {
    fn new(button_tracker: ButtonTracker) -> Self {
        Self {
            button_tracker,
            _display: PhantomData,
            _buttons: PhantomData,
        }
    }
}

impl<B> Controller<WithDisplay, B> {
    pub fn command(&self, command: DisplayCommand) -> [u8; 4] {
        command.packet()
    }

    pub fn reset(&self) -> [u8; 4] {
        DisplayCommand::Reset.packet()
    }

    pub fn height(&self, height_mm: u16) -> [u8; 4] {
        DisplayCommand::Height(height_mm).packet()
    }

    pub fn error(&self, error: HandsetError) -> [u8; 4] {
        DisplayCommand::Error(error).packet()
    }

    pub fn program(&self, program: ProgramCommand) -> [u8; 4] {
        DisplayCommand::Program(program).packet()
    }
}

impl<D> Controller<D, WithButtons> {
    pub fn update_buttons(&mut self, lines: ButtonLines) -> Option<ButtonEvent> {
        self.button_tracker.update(lines)
    }

    pub fn button_reading(&self, lines: ButtonLines) -> ButtonReading {
        decode_button(lines)
    }

    pub fn reset_button_tracker(&mut self) {
        self.button_tracker = ButtonTracker::default();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ControllerBuilder<Display = WithoutDisplay, Buttons = WithoutButtons> {
    button_tracker: ButtonTracker,
    _display: PhantomData<Display>,
    _buttons: PhantomData<Buttons>,
}

impl Default for ControllerBuilder<WithoutDisplay, WithoutButtons> {
    fn default() -> Self {
        Self {
            button_tracker: ButtonTracker::default(),
            _display: PhantomData,
            _buttons: PhantomData,
        }
    }
}

impl<D, B> ControllerBuilder<D, B> {
    pub fn with_display(self) -> ControllerBuilder<WithDisplay, B> {
        ControllerBuilder {
            button_tracker: self.button_tracker,
            _display: PhantomData,
            _buttons: PhantomData,
        }
    }

    pub fn with_buttons(self) -> ControllerBuilder<D, WithButtons> {
        ControllerBuilder {
            button_tracker: self.button_tracker,
            _display: PhantomData,
            _buttons: PhantomData,
        }
    }

    pub fn build(self) -> Controller<D, B> {
        Controller::new(self.button_tracker)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayCommand {
    Reset,
    Height(u16),
    Error(HandsetError),
    Program(ProgramCommand),
}

impl DisplayCommand {
    pub fn packet(self) -> [u8; 4] {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsetError {
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
    pub fn arg0(self) -> u8 {
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

    pub fn code(self) -> u8 {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramCommand {
    Pending,
    Preset1,
    Preset2,
    Preset3,
    Preset4,
}

impl ProgramCommand {
    pub fn arg0(self) -> u8 {
        match self {
            Self::Pending => 0x00,
            Self::Preset1 => 0x01,
            Self::Preset2 => 0x02,
            Self::Preset3 => 0x04,
            Self::Preset4 => 0x08,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Up,
    Down,
    M1,
    M2,
    M3,
    M4,
    Rec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonReading {
    None,
    Button(Button),
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    Pressed(Button),
    Released(Button),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ButtonLines {
    pub pin2_low: bool,
    pub pin3_low: bool,
    pub pin4_low: bool,
    pub pin5_low: bool,
}

impl ButtonLines {
    pub const fn new(pin2_low: bool, pin3_low: bool, pin4_low: bool, pin5_low: bool) -> Self {
        Self {
            pin2_low,
            pin3_low,
            pin4_low,
            pin5_low,
        }
    }

    pub const fn raw_low_mask(self) -> u8 {
        let mut mask = 0;

        if self.pin2_low {
            mask |= 0b0001;
        }
        if self.pin3_low {
            mask |= 0b0010;
        }
        if self.pin4_low {
            mask |= 0b0100;
        }
        if self.pin5_low {
            mask |= 0b1000;
        }

        mask
    }

    pub const fn from_raw_low_mask(raw_low_mask: u8) -> Self {
        Self {
            pin2_low: raw_low_mask & 0b0001 != 0,
            pin3_low: raw_low_mask & 0b0010 != 0,
            pin4_low: raw_low_mask & 0b0100 != 0,
            pin5_low: raw_low_mask & 0b1000 != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonTracker {
    last_reading: ButtonReading,
}

impl Default for ButtonTracker {
    fn default() -> Self {
        Self {
            last_reading: ButtonReading::None,
        }
    }
}

impl ButtonTracker {
    pub fn update(&mut self, lines: ButtonLines) -> Option<ButtonEvent> {
        let reading = decode_button(lines);
        let previous = self.last_reading;

        if reading == previous {
            return None;
        }

        self.last_reading = reading;

        match (previous, reading) {
            (_, ButtonReading::Button(button)) => Some(ButtonEvent::Pressed(button)),
            (ButtonReading::Button(button), ButtonReading::None | ButtonReading::Unknown(_)) => {
                Some(ButtonEvent::Released(button))
            }
            (
                ButtonReading::None | ButtonReading::Unknown(_),
                ButtonReading::None | ButtonReading::Unknown(_),
            ) => None,
        }
    }
}

pub fn decode_button(lines: ButtonLines) -> ButtonReading {
    decode_button_mask(lines.raw_low_mask())
}

pub fn decode_button_mask(raw_low_mask: u8) -> ButtonReading {
    match raw_low_mask & 0x0f {
        0b0000 => ButtonReading::None,
        0b1000 => ButtonReading::Button(Button::Up),
        0b0100 => ButtonReading::Button(Button::Down),
        0b1100 => ButtonReading::Button(Button::M1),
        0b0010 => ButtonReading::Button(Button::M2),
        0b0110 => ButtonReading::Button(Button::M3),
        0b1010 => ButtonReading::Button(Button::M4),
        0b0001 => ButtonReading::Button(Button::Rec),
        unknown => ButtonReading::Unknown(unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_packets_match_protocol() {
        assert_eq!(DisplayCommand::Reset.packet(), [0x01, 0x04, 0x01, 0xaa]);
        assert_eq!(
            DisplayCommand::Height(700).packet(),
            [0x01, 0x01, 0x02, 0xbc]
        );
        assert_eq!(
            DisplayCommand::Height(1000).packet(),
            [0x01, 0x01, 0x03, 0xe8]
        );
        assert_eq!(
            DisplayCommand::Height(1286).packet(),
            [0x01, 0x01, 0x05, 0x06]
        );
    }

    #[test]
    fn error_packets_match_protocol() {
        let errors = [
            (HandsetError::E01, 0x01),
            (HandsetError::E02, 0x02),
            (HandsetError::E03, 0x04),
            (HandsetError::E04, 0x08),
            (HandsetError::E05, 0x10),
            (HandsetError::E06, 0x20),
            (HandsetError::E07, 0x40),
            (HandsetError::E08, 0x80),
        ];

        for (error, arg0) in errors {
            assert_eq!(
                DisplayCommand::Error(error).packet(),
                [0x01, 0x02, arg0, 0x00]
            );
        }
    }

    #[test]
    fn program_packets_match_protocol() {
        let programs = [
            (ProgramCommand::Pending, 0x00),
            (ProgramCommand::Preset1, 0x01),
            (ProgramCommand::Preset2, 0x02),
            (ProgramCommand::Preset3, 0x04),
            (ProgramCommand::Preset4, 0x08),
        ];

        for (program, arg0) in programs {
            assert_eq!(
                DisplayCommand::Program(program).packet(),
                [0x01, 0x06, arg0, 0x00]
            );
        }
    }

    #[test]
    fn documented_button_lines_decode() {
        let cases = [
            (ButtonLines::default(), ButtonReading::None),
            (
                ButtonLines::new(false, false, false, true),
                ButtonReading::Button(Button::Up),
            ),
            (
                ButtonLines::new(false, false, true, false),
                ButtonReading::Button(Button::Down),
            ),
            (
                ButtonLines::new(false, false, true, true),
                ButtonReading::Button(Button::M1),
            ),
            (
                ButtonLines::new(false, true, false, false),
                ButtonReading::Button(Button::M2),
            ),
            (
                ButtonLines::new(false, true, true, false),
                ButtonReading::Button(Button::M3),
            ),
            (
                ButtonLines::new(false, true, false, true),
                ButtonReading::Button(Button::M4),
            ),
            (
                ButtonLines::new(true, false, false, false),
                ButtonReading::Button(Button::Rec),
            ),
        ];

        for (lines, reading) in cases {
            assert_eq!(decode_button(lines), reading);
        }
    }

    #[test]
    fn unknown_button_masks_are_reported() {
        assert_eq!(decode_button_mask(0b0011), ButtonReading::Unknown(0b0011));
        assert_eq!(decode_button_mask(0b1111), ButtonReading::Unknown(0b1111));
    }

    #[test]
    fn button_lines_round_trip_to_raw_mask() {
        let lines = ButtonLines::new(true, false, true, false);

        assert_eq!(lines.raw_low_mask(), 0b0101);
        assert_eq!(ButtonLines::from_raw_low_mask(0b0101), lines);
        assert_eq!(
            ButtonLines::from_raw_low_mask(0b1111).raw_low_mask(),
            0b1111
        );
    }

    #[test]
    fn button_tracker_emits_pressed_and_released_edges() {
        let mut tracker = ButtonTracker::default();

        assert_eq!(tracker.update(ButtonLines::from_raw_low_mask(0b0000)), None);
        assert_eq!(
            tracker.update(ButtonLines::from_raw_low_mask(0b1000)),
            Some(ButtonEvent::Pressed(Button::Up))
        );
        assert_eq!(tracker.update(ButtonLines::from_raw_low_mask(0b1000)), None);
        assert_eq!(
            tracker.update(ButtonLines::from_raw_low_mask(0b0000)),
            Some(ButtonEvent::Released(Button::Up))
        );
        assert_eq!(
            tracker.update(ButtonLines::from_raw_low_mask(0b1000)),
            Some(ButtonEvent::Pressed(Button::Up))
        );
        assert_eq!(
            tracker.update(ButtonLines::from_raw_low_mask(0b0100)),
            Some(ButtonEvent::Pressed(Button::Down))
        );
        assert_eq!(
            tracker.update(ButtonLines::from_raw_low_mask(0b0011)),
            Some(ButtonEvent::Released(Button::Down))
        );
    }

    #[test]
    fn button_tracker_ignores_none_and_unknown_edges_without_previous_button() {
        let mut tracker = ButtonTracker::default();

        assert_eq!(tracker.update(ButtonLines::from_raw_low_mask(0b0011)), None);
        assert_eq!(tracker.update(ButtonLines::from_raw_low_mask(0b1111)), None);
        assert_eq!(tracker.update(ButtonLines::default()), None);
    }

    #[test]
    fn controllers_construct_for_supported_capabilities() {
        let full = Controller::default();
        assert_eq!(full.reset(), [0x01, 0x04, 0x01, 0xaa]);

        let display_only = Controller::display_only();
        assert_eq!(
            display_only.program(ProgramCommand::Pending),
            [0x01, 0x06, 0x00, 0x00]
        );

        let mut buttons_only = Controller::buttons_only();
        assert_eq!(
            buttons_only.update_buttons(ButtonLines::from_raw_low_mask(0b1100)),
            Some(ButtonEvent::Pressed(Button::M1))
        );

        let explicit = Controller::builder().with_display().with_buttons().build();
        assert_eq!(explicit.height(42), [0x01, 0x01, 0x00, 0x2a]);
    }
}
