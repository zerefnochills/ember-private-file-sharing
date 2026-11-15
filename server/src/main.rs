//! Ember relay server (M3). Stores ciphertext only; never sees keys or plaintext.
fn main() {
    println!(
        "ember-server {} (milestone M0 placeholder, max lifetime {}s)",
        ember_core::version(),
        ember_protocol::MAX_LIFETIME_SECS
    );
}
