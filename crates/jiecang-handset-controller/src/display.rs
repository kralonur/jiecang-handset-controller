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

#[cfg(test)]
mod tests {
    use super::{DisplayCommand, HandsetError, ProgramCommand};

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
}
