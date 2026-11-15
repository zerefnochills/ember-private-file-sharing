//! Ephemeral local handles for Ember threads (e.g. `amber-fox-4821`).
//!
//! Follows `docs/EMBER_PROJECT_BLUEPRINT.md` section 7.4:
//! - Local only, generated client-side from wordlists + 4 random digits.
//! - Encrypted inside thread messages, never shared with or indexed by the relay server.

use dryoc::rng::copy_randombytes;

const ADJECTIVES: &[&str] = &[
    "amber", "autumn", "azure", "bright", "cedar", "cobalt", "coral", "crimson",
    "crystal", "delta", "dusk", "ember", "fern", "flint", "frost", "golden",
    "hazel", "indigo", "jasper", "lunar", "maple", "moss", "nebula", "nova",
    "onyx", "opal", "pebble", "pine", "quartz", "river", "ruby", "sage",
    "shadow", "sienna", "silver", "slate", "solar", "stone", "storm", "swift",
    "topaz", "velvet", "willow", "winter",
];

const NOUNS: &[&str] = &[
    "badger", "bear", "beaver", "bison", "condor", "crane", "eagle", "falcon",
    "finch", "fox", "gecko", "hawk", "heron", "jaguar", "lemur", "lynx",
    "otter", "owl", "panda", "panther", "quail", "raven", "robin", "salmon",
    "sparrow", "swift", "tiger", "viper", "wolf", "wren",
];

/// Generates a randomized, memorable local handle (e.g. `amber-fox-4821`).
pub fn generate_handle() -> String {
    let mut rand_bytes = [0u8; 6];
    copy_randombytes(&mut rand_bytes);

    let adj_idx = (u16::from_be_bytes([rand_bytes[0], rand_bytes[1]]) as usize) % ADJECTIVES.len();
    let noun_idx = (u16::from_be_bytes([rand_bytes[2], rand_bytes[3]]) as usize) % NOUNS.len();
    let digits = (u16::from_be_bytes([rand_bytes[4], rand_bytes[5]]) as usize) % 10000;

    format!("{}-{}-{:04}", ADJECTIVES[adj_idx], NOUNS[noun_idx], digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_handle_format() {
        let handle = generate_handle();
        let parts: Vec<&str> = handle.split('-').collect();
        assert_eq!(parts.len(), 3, "Handle must have three parts: adj-noun-digits");
        assert!(ADJECTIVES.contains(&parts[0]));
        assert!(NOUNS.contains(&parts[1]));
        assert_eq!(parts[2].len(), 4);
        assert!(parts[2].chars().all(|c| c.is_ascii_digit()));
    }
}
