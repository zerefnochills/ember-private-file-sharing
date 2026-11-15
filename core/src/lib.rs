//! Ember core: encryption, link format, expiry rules, metadata stripping.
//!
//! Milestone M0 placeholder. Security-critical code arrives in M2 and must use
//! audited libraries only (no custom cryptography).

/// Crate version, exposed so clients can report which core they embed.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
