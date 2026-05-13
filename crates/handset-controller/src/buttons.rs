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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonTracker {
    last_reading: ButtonReading,
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

pub fn decode_button(lines: ButtonLines) -> ButtonReading {
    decode_button_mask(lines.raw_low_mask())
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

impl Default for ButtonTracker {
    fn default() -> Self {
        Self {
            last_reading: ButtonReading::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Button, ButtonEvent, ButtonLines, ButtonReading, ButtonTracker, decode_button,
        decode_button_mask,
    };

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
}
