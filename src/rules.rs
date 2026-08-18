//! Custom neighborhood rules, parsed from a Golly-style rule string.
//!
//! A rule names which live-neighbor counts (0..8) cause a dead cell to be
//! *born* and which keep a live cell *alive*. The canonical form is
//! `B<births>/S<survive>` — Conway's Life is `B3/S23` and Day & Night is
//! `B368/S245`. Either half may be omitted (it then defaults to "never"), and
//! the digit lists may be comma-separated, e.g. `B3,6/S2,4,5`.

use std::fmt;
use std::str::FromStr;

/// A neighborhood rule, stored as two 9-bit masks over live-neighbor counts 0..8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Bit `n` set ⟺ a dead cell is born with `n` live neighbors.
    born: u32,
    /// Bit `n` set ⟺ a live cell survives with `n` live neighbors.
    survive: u32,
}

impl Rules {
    /// Conway's Game of Life: born on 3, survive on 2 or 3.
    pub const fn conway() -> Self {
        Rules {
            born: 1 << 3,
            survive: (1 << 2) | (1 << 3),
        }
    }

    /// A rule in which no cell is ever born or survives — everything dies out.
    pub const fn empty() -> Self {
        Rules {
            born: 0,
            survive: 0,
        }
    }

    /// `true` if the rule is exactly Conway's `B3/S23` (the bit-parallel fast path).
    pub fn is_conway(&self) -> bool {
        *self == Rules::conway()
    }

    /// The birth mask, for the word-level `BitBoard` path.
    pub fn born_mask(&self) -> u32 {
        self.born
    }

    /// The survival mask, for the word-level `BitBoard` path.
    pub fn survive_mask(&self) -> u32 {
        self.survive
    }

    pub fn born_on(&self, n: usize) -> bool {
        self.born & (1 << n) != 0
    }

    pub fn survives_on(&self, n: usize) -> bool {
        self.survive & (1 << n) != 0
    }
}

impl Default for Rules {
    fn default() -> Self {
        Rules::conway()
    }
}

impl fmt::Display for Rules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn digits(mask: u32) -> String {
            (0..9)
                .filter(|&n| mask & (1 << n) != 0)
                .map(|n| n.to_string())
                .collect()
        }
        write!(f, "B{}/S{}", digits(self.born), digits(self.survive))
    }
}

impl FromStr for Rules {
    type Err = ParseRulesErr;

    fn from_str(input: &str) -> Result<Self, ParseRulesErr> {
        let mut rules = Rules::empty();
        for (i, part) in input.split('/').enumerate() {
            let mask = match i {
                0 => &mut rules.born,
                1 => &mut rules.survive,
                _ => return Err(ParseRulesErr("at most one '/' separator is allowed".into())),
            };
            parse_set(part, mask)?;
        }
        Ok(rules)
    }
}

/// Parse one rule half (an optional `B`/`S` tag followed by 0–8 digit counts).
fn parse_set(part: &str, mask: &mut u32) -> Result<(), ParseRulesErr> {
    // Strip an optional leading B/b/S/s tag.
    let digits = match part.chars().next() {
        Some('B' | 'b' | 'S' | 's') => part.get(1..).unwrap_or(""),
        _ => part,
    };
    for c in digits.chars() {
        if c == ',' {
            continue;
        }
        let n = c
            .to_digit(10)
            .ok_or_else(|| ParseRulesErr(format!("invalid character '{c}' in rule")))?;
        if n > 8 {
            return Err(ParseRulesErr(format!(
                "neighbor count {n} out of range 0-8"
            )));
        }
        *mask |= 1 << n;
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub struct ParseRulesErr(String);

impl fmt::Display for ParseRulesErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseRulesErr {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conway() {
        assert_eq!(Rules::from_str("B3/S23").unwrap(), Rules::conway());
        assert_eq!(Rules::from_str("3/23").unwrap(), Rules::conway());
        assert_eq!(Rules::from_str("B3/S2,3").unwrap(), Rules::conway());
        assert!(Rules::from_str("B3/S23").unwrap().is_conway());
    }

    #[test]
    fn parses_named_rules() {
        let day_night = Rules::from_str("B368/S245").unwrap();
        assert!(day_night.born_on(3) && day_night.born_on(6) && day_night.born_on(8));
        assert!(day_night.survives_on(2) && day_night.survives_on(4) && day_night.survives_on(5));
        assert!(!day_night.is_conway());
    }

    #[test]
    fn empty_half_is_never() {
        let r = Rules::from_str("B3/").unwrap();
        assert!(r.born_on(3));
        assert!((0..9).all(|n| !r.survives_on(n)));
    }

    #[test]
    fn display_round_trips() {
        assert_eq!(Rules::conway().to_string(), "B3/S23");
        assert_eq!(
            Rules::from_str("B368/S245").unwrap().to_string(),
            "B368/S245"
        );
    }

    #[test]
    fn rejects_out_of_range_and_junk() {
        assert!(Rules::from_str("B9/S23").is_err());
        assert!(Rules::from_str("B3/Sx").is_err());
        assert!(Rules::from_str("B3/S23/extra").is_err());
    }

    #[test]
    fn default_is_conway() {
        assert_eq!(Rules::default(), Rules::conway());
        assert_ne!(Rules::default(), Rules::empty());
    }
}
