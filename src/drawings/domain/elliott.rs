//! The degree of an Elliott wave count.

use serde::{Deserialize, Serialize};

/// The degree of an Elliott wave count, which sets the notation of its labels so that counts of
/// different degree on one chart can be told apart. From the largest degree to the smallest.
///
/// Corrective waves, the waves of a triangle and of combinations (A to E, W, X, Y, Z) take the
/// letter notation of their degree. The three degrees above Grand Supercycle repeat the notation
/// of Grand Supercycle, Supercycle and Cycle, and the three below Subminuette that of Primary,
/// Intermediate and Minor, in square brackets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ElliottDegree {
    /// Waves `[((I))]` to `[((V))]` and `[((A))]`, `[((B))]`, `[((C))]`.
    Supermillennium,
    /// Waves `[(I)]` to `[(V)]` and `[(A)]`, `[(B)]`, `[(C)]`.
    Millennium,
    /// Waves `[I]` to `[V]` and `[A]`, `[B]`, `[C]`.
    Submillennium,
    /// Waves ((I)) to ((V)) and ((A)), ((B)), ((C)).
    GrandSupercycle,
    /// Waves (I) to (V) and (A), (B), (C).
    Supercycle,
    /// Waves I to V and A, B, C.
    Cycle,
    /// Waves ((1)) to ((5)) and ((A)), ((B)), ((C)).
    Primary,
    /// Waves (1) to (5) and (A), (B), (C), the degree of a new drawing.
    Intermediate,
    /// Waves 1 to 5 and A, B, C, the degree of drawings saved without one.
    #[default]
    Minor,
    /// Waves ((i)) to ((v)) and ((a)), ((b)), ((c)).
    Minute,
    /// Waves (i) to (v) and (a), (b), (c).
    Minuette,
    /// Waves i to v and a, b, c.
    Subminuette,
    /// Waves `[((1))]` to `[((5))]` and `[((A))]`, `[((B))]`, `[((C))]`.
    Micro,
    /// Waves `[(1)]` to `[(5)]` and `[(A)]`, `[(B)]`, `[(C)]`.
    Submicro,
    /// Waves `[1]` to `[5]` and `[A]`, `[B]`, `[C]`.
    Miniscule,
}

impl ElliottDegree {
    /// Every degree, from the largest to the smallest.
    #[cfg(feature = "ui")]
    pub(crate) const ALL: [Self; 15] = [
        Self::Supermillennium,
        Self::Millennium,
        Self::Submillennium,
        Self::GrandSupercycle,
        Self::Supercycle,
        Self::Cycle,
        Self::Primary,
        Self::Intermediate,
        Self::Minor,
        Self::Minute,
        Self::Minuette,
        Self::Subminuette,
        Self::Micro,
        Self::Submicro,
        Self::Miniscule,
    ];

    /// The name of the degree, such as "Minor".
    #[cfg(feature = "ui")]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Supermillennium => "Supermillennium",
            Self::Millennium => "Millennium",
            Self::Submillennium => "Submillennium",
            Self::GrandSupercycle => "Grand Supercycle",
            Self::Supercycle => "Supercycle",
            Self::Cycle => "Cycle",
            Self::Primary => "Primary",
            Self::Intermediate => "Intermediate",
            Self::Minor => "Minor",
            Self::Minute => "Minute",
            Self::Minuette => "Minuette",
            Self::Subminuette => "Subminuette",
            Self::Micro => "Micro",
            Self::Submicro => "Submicro",
            Self::Miniscule => "Miniscule",
        }
    }

    /// The label of `wave`, a digit `'1'` to `'5'` for a motive wave or a capital letter for a
    /// corrective one, in this degree's notation.
    pub(crate) fn label(self, wave: char) -> String {
        let bracketed = match self {
            Self::Supermillennium => Self::GrandSupercycle,
            Self::Millennium => Self::Supercycle,
            Self::Submillennium => Self::Cycle,
            Self::Micro => Self::Primary,
            Self::Submicro => Self::Intermediate,
            Self::Miniscule => Self::Minor,
            _ => self,
        };
        if bracketed != self {
            return format!("[{}]", bracketed.label(wave));
        }
        let (open, close) = match self {
            Self::GrandSupercycle | Self::Primary | Self::Minute => ("((", "))"),
            Self::Supercycle | Self::Intermediate | Self::Minuette => ("(", ")"),
            // Cycle, Minor and Subminuette.
            _ => ("", ""),
        };
        let lower = matches!(self, Self::Minute | Self::Minuette | Self::Subminuette);
        let numeral = wave
            .to_digit(10)
            .filter(|n| (1..=5).contains(n))
            .map(|n| n as usize - 1);
        let mut label = String::from(open);
        match (self, numeral) {
            (Self::GrandSupercycle | Self::Supercycle | Self::Cycle, Some(n)) => {
                label.push_str(["I", "II", "III", "IV", "V"][n]);
            }
            (_, Some(n)) if lower => label.push_str(["i", "ii", "iii", "iv", "v"][n]),
            _ if lower => label.push(wave.to_ascii_lowercase()),
            _ => label.push(wave),
        }
        label.push_str(close);
        label
    }
}
