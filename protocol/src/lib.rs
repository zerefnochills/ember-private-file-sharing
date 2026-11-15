//! Ember wire types shared by the core and the server.
//!
//! Milestone M0 placeholder. Concrete types follow `docs/protocol-spec.md` (M1).

/// Maximum lifetime of any drop or thread, in seconds (12 hours).
pub const MAX_LIFETIME_SECS: u64 = 12 * 60 * 60;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_lifetime_is_twelve_hours() {
        assert_eq!(MAX_LIFETIME_SECS, 43_200);
    }
}
