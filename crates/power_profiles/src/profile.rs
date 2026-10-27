use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    PowerSaver,
    Balanced,
    Performance,
}

impl Profile {
    pub const ALL: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    /// The name power-profiles-daemon uses for this profile.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    /// The profile after this one, going from power saver to performance and back around.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::PowerSaver => Self::Balanced,
            Self::Balanced => Self::Performance,
            Self::Performance => Self::PowerSaver,
        }
    }
}

impl FromStr for Profile {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "power-saver" => Ok(Self::PowerSaver),
            "balanced" => Ok(Self::Balanced),
            "performance" => Ok(Self::Performance),
            _ => Err(crate::Error::ParseError(s.to_string())),
        }
    }
}
