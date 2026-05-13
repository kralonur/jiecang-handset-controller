use core::marker::PhantomData;

use crate::{
    ButtonEvent, ButtonLines, ButtonReading, ButtonTracker, DisplayCommand, HandsetError,
    ProgramCommand, decode_button,
};

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

#[derive(Debug, Clone, Copy)]
pub struct ControllerBuilder<Display = WithoutDisplay, Buttons = WithoutButtons> {
    button_tracker: ButtonTracker,
    _display: PhantomData<Display>,
    _buttons: PhantomData<Buttons>,
}

pub type FullController = Controller<WithDisplay, WithButtons>;
pub type DisplayController = Controller<WithDisplay, WithoutButtons>;
pub type ButtonsController = Controller<WithoutDisplay, WithButtons>;

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

impl<D, B> Controller<D, B> {
    fn new(button_tracker: ButtonTracker) -> Self {
        Self {
            button_tracker,
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

impl Default for ControllerBuilder<WithoutDisplay, WithoutButtons> {
    fn default() -> Self {
        Self {
            button_tracker: ButtonTracker::default(),
            _display: PhantomData,
            _buttons: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Button, ButtonEvent, ButtonLines, Controller, ProgramCommand};

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
