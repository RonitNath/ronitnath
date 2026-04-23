use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::RngCore as _;
use sha2::Sha256;
use subtle::ConstantTimeEq as _;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy)]
pub(crate) enum TokenPurpose {
    Invite,
    Signup,
    FormNonce,
}

impl TokenPurpose {
    fn prefix(self) -> &'static str {
        match self {
            Self::Invite => "invite:v1",
            Self::Signup => "signup:v1",
            Self::FormNonce => "form_nonce:v1",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TokenHasher {
    secret: Vec<u8>,
}

impl TokenHasher {
    pub(crate) fn new(secret: impl AsRef<[u8]>) -> Self {
        Self {
            secret: secret.as_ref().to_vec(),
        }
    }

    pub(crate) fn generate() -> String {
        let mut bytes = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    pub(crate) fn hash(&self, purpose: TokenPurpose, token: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .unwrap_or_else(|_| unreachable!("hmac accepts keys of any size"));
        mac.update(purpose.prefix().as_bytes());
        mac.update(b":");
        mac.update(token.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    pub(crate) fn verify(&self, purpose: TokenPurpose, token: &str, expected_hash: &str) -> bool {
        let computed = self.hash(purpose, token);
        computed.as_bytes().ct_eq(expected_hash.as_bytes()).into()
    }

    pub(crate) fn generate_hash_pair(&self, purpose: TokenPurpose) -> (String, String) {
        let token = Self::generate();
        let hash = self.hash(purpose, &token);
        (token, hash)
    }
}

#[cfg(test)]
mod tests {
    use super::{TokenHasher, TokenPurpose};

    #[test]
    fn token_hashes_are_purpose_separated() {
        let hasher = TokenHasher::new("secret");
        let token = "abc";
        assert_ne!(
            hasher.hash(TokenPurpose::Invite, token),
            hasher.hash(TokenPurpose::Signup, token)
        );
    }
}
