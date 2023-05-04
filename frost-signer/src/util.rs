use p256k1::ecdsa;

pub fn parse_public_key(public_key: &str) -> Result<ecdsa::PublicKey, ecdsa::Error> {
    ecdsa::PublicKey::try_from(public_key)
}
