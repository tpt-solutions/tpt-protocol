use core::fmt;
use core::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quality {
    Good,
    Bad,
    Stale,
    Uncertain,
}

impl Quality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Stale => "stale",
            Self::Uncertain => "uncertain",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        Self::from_str_inner(s)
    }

    fn from_str_inner(s: &str) -> Option<Self> {
        match s {
            "good" => Some(Self::Good),
            "bad" => Some(Self::Bad),
            "stale" => Some(Self::Stale),
            "uncertain" => Some(Self::Uncertain),
            _ => None,
        }
    }
}

impl FromStr for Quality {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_inner(s).ok_or(())
    }
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
