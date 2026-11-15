//! Server-side proof-of-work challenge issuance, verification, and anti-replay cache.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use dryoc::rng::copy_randombytes;
use ember_core::pow::{verify_pow, PowChallengeToken};

use crate::error::ServerError;

/// Default PoW difficulty (number of leading zero bits).
pub const DEFAULT_POW_DIFFICULTY: u32 = 16;
/// Default challenge lifetime in seconds (5 minutes).
pub const DEFAULT_CHALLENGE_TTL_SECS: u64 = 300;

/// Manages PoW challenge issuance, runtime difficulty adjustment, and anti-replay tracking.
pub struct PowManager {
    server_secret: [u8; 32],
    difficulty: AtomicU32,
    challenge_ttl_secs: u64,
    consumed_challenges: Mutex<HashMap<[u8; 16], u64>>,
}

impl PowManager {
    /// Creates a new `PowManager` with a random secret key and the specified difficulty.
    pub fn new(difficulty: u32) -> Self {
        let mut secret = [0u8; 32];
        copy_randombytes(&mut secret);
        Self::with_secret(difficulty, secret)
    }

    /// Creates a `PowManager` with an explicit secret key and difficulty (useful for tests).
    pub fn with_secret(difficulty: u32, secret: [u8; 32]) -> Self {
        Self {
            server_secret: secret,
            difficulty: AtomicU32::new(difficulty),
            challenge_ttl_secs: DEFAULT_CHALLENGE_TTL_SECS,
            consumed_challenges: Mutex::new(HashMap::new()),
        }
    }

    /// Updates the PoW difficulty target at runtime.
    pub fn set_difficulty(&self, difficulty: u32) {
        self.difficulty.store(difficulty, Ordering::SeqCst);
    }

    /// Returns the current PoW difficulty target.
    pub fn difficulty(&self) -> u32 {
        self.difficulty.load(Ordering::SeqCst)
    }

    /// Issues a new signed, time-limited proof-of-work challenge token.
    ///
    /// Returns `(challenge_string, difficulty, expires_at)`.
    pub fn issue_challenge(&self) -> (String, u32, u64) {
        let mut id = [0u8; 16];
        copy_randombytes(&mut id);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        let expires_at = now + self.challenge_ttl_secs;
        let diff = self.difficulty();

        let token_str = PowChallengeToken::create(id, now, expires_at, diff, &self.server_secret);

        (token_str, diff, expires_at)
    }

    /// Verifies a submitted challenge and nonce solution against the server secret,
    /// current timestamp, and anti-replay cache.
    pub async fn verify_solution(
        &self,
        challenge_str: &str,
        nonce: u64,
    ) -> Result<(), ServerError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        // 1. Verify cryptographic signature and expiry
        let token =
            match PowChallengeToken::verify_and_decode(challenge_str, &self.server_secret, now) {
                Ok(t) => t,
                Err(ember_core::EmberError::Expired(e)) => return Err(ServerError::PowExpired(e)),
                Err(e) => return Err(ServerError::PowInvalid(e.to_string())),
            };

        // 2. Check difficulty requirement
        let current_target = self.difficulty();
        if token.difficulty < current_target {
            return Err(ServerError::PowInvalid(format!(
                "Challenge difficulty ({}) below current server threshold ({})",
                token.difficulty, current_target
            )));
        }

        // 3. Check anti-replay cache
        let mut cache = self.consumed_challenges.lock().await;

        // Clean up expired cache entries
        cache.retain(|_, &mut exp| exp >= now);

        if cache.contains_key(&token.id) {
            return Err(ServerError::PowReplayed);
        }

        // 4. Verify SHA-256 hashcash condition
        if !verify_pow(challenge_str.as_bytes(), nonce, token.difficulty) {
            return Err(ServerError::PowInvalid(
                "Proof-of-work hash does not meet required difficulty".into(),
            ));
        }

        // 5. Record challenge as consumed until its expiry
        cache.insert(token.id, token.expires_at);

        Ok(())
    }

    /// Purges expired challenges from the anti-replay cache.
    pub async fn sweep_expired(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let mut cache = self.consumed_challenges.lock().await;
        cache.retain(|_, &mut exp| exp >= now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember_core::pow::solve_pow;

    #[tokio::test]
    async fn test_issue_solve_and_verify_success() {
        let pow = PowManager::new(8); // Low difficulty for quick test
        let (challenge, diff, _) = pow.issue_challenge();
        assert_eq!(diff, 8);

        let nonce = solve_pow(challenge.as_bytes(), diff, Some(100_000)).unwrap();
        assert!(pow.verify_solution(&challenge, nonce).await.is_ok());
    }

    #[tokio::test]
    async fn test_replay_is_rejected() {
        let pow = PowManager::new(8);
        let (challenge, diff, _) = pow.issue_challenge();

        let nonce = solve_pow(challenge.as_bytes(), diff, Some(100_000)).unwrap();
        assert!(pow.verify_solution(&challenge, nonce).await.is_ok());

        // Second submission of the same challenge must be rejected as replayed
        let err = pow.verify_solution(&challenge, nonce).await.unwrap_err();
        match err {
            ServerError::PowReplayed => {}
            other => panic!("expected PowReplayed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_wrong_nonce_is_rejected() {
        let pow = PowManager::new(8);
        let (challenge, diff, _) = pow.issue_challenge();

        let nonce = solve_pow(challenge.as_bytes(), diff, Some(100_000)).unwrap();
        let wrong_nonce = nonce.wrapping_add(1);

        // Could accidentally match if difficulty is low, but verify with 250 difficulty check or ensure mismatch
        let err = pow.verify_solution(&challenge, wrong_nonce).await;
        // In the rare case wrong_nonce also meets 8 leading zeros, this test might pass or fail,
        // so test with an impossible difficulty or check failure if not matching
        if !ember_core::pow::verify_pow(challenge.as_bytes(), wrong_nonce, diff) {
            assert!(err.is_err());
        }
    }

    #[tokio::test]
    async fn test_expired_challenge_is_rejected() {
        let secret = [42u8; 32];
        let pow = PowManager::with_secret(8, secret);

        // Manually create an expired challenge token
        let id = [9u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expired_token = PowChallengeToken::create(id, now - 500, now - 10, 8, &secret);

        let nonce = solve_pow(expired_token.as_bytes(), 8, Some(100_000)).unwrap();
        let err = pow
            .verify_solution(&expired_token, nonce)
            .await
            .unwrap_err();
        match err {
            ServerError::PowExpired(_) => {}
            other => panic!("expected PowExpired, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_runtime_difficulty_adjustment() {
        let pow = PowManager::new(8);
        assert_eq!(pow.difficulty(), 8);

        let (challenge, diff, _) = pow.issue_challenge();
        assert_eq!(diff, 8);

        // Increase difficulty to 20
        pow.set_difficulty(20);
        assert_eq!(pow.difficulty(), 20);

        // An older challenge with difficulty 8 should now be rejected as below threshold
        let nonce = solve_pow(challenge.as_bytes(), 8, Some(100_000)).unwrap();
        let err = pow.verify_solution(&challenge, nonce).await.unwrap_err();
        match err {
            ServerError::PowInvalid(msg) => {
                assert!(msg.contains("below current server threshold"));
            }
            other => panic!("expected PowInvalid, got {other:?}"),
        }
    }
}
