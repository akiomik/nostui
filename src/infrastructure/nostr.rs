use std::borrow::Cow;
use std::error::Error;

use nostr_sdk::prelude::*;
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub enum PublicKeySignerError {
    Readonly,
}

impl Error for PublicKeySignerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeySigner {
    pubkey: PublicKey,
}

impl PublicKeySigner {
    pub fn new(pubkey: PublicKey) -> Self {
        Self { pubkey }
    }
}

impl NostrSigner for PublicKeySigner {
    fn backend(&self) -> SignerBackend<'_> {
        SignerBackend::Custom(Cow::Borrowed("PublicKeySigner"))
    }

    fn get_public_key(&self) -> BoxedFuture<'_, Result<PublicKey, SignerError>> {
        Box::pin(async { Ok(self.pubkey) })
    }

    fn sign_event(&self, _unsigned: UnsignedEvent) -> BoxedFuture<'_, Result<Event, SignerError>> {
        Box::pin(async { Err(SignerError::backend(PublicKeySignerError::Readonly)) })
    }

    fn nip04_encrypt<'a>(
        &'a self,
        _public_key: &'a PublicKey,
        _content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async { Err(SignerError::backend(PublicKeySignerError::Readonly)) })
    }

    fn nip04_decrypt<'a>(
        &'a self,
        _public_key: &'a PublicKey,
        _encrypted_content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async { Err(SignerError::backend(PublicKeySignerError::Readonly)) })
    }

    fn nip44_encrypt<'a>(
        &'a self,
        _public_key: &'a PublicKey,
        _content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async { Err(SignerError::backend(PublicKeySignerError::Readonly)) })
    }

    fn nip44_decrypt<'a>(
        &'a self,
        _public_key: &'a PublicKey,
        _payload: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async { Err(SignerError::backend(PublicKeySignerError::Readonly)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::eyre::eyre;

    fn example_pubkey() -> color_eyre::Result<PublicKey> {
        Ok(PublicKey::from_slice(&[1u8; 32])?)
    }

    #[test]
    fn backend_is_custom_public_key_signer() -> color_eyre::Result<()> {
        let signer = PublicKeySigner::new(example_pubkey()?);

        match signer.backend() {
            SignerBackend::Custom(name) => {
                assert_eq!(name.as_ref(), "PublicKeySigner");
            }
            other => {
                return Err(eyre!(format!("unexpected backend: {other:?}")));
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn get_public_key_returns_given_key() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let got = signer.get_public_key().await?;
        assert_eq!(got, pubkey);

        Ok(())
    }

    #[tokio::test]
    async fn sign_event_returns_readonly_error() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let unsigned = UnsignedEvent::new(pubkey, Timestamp::now(), Kind::TextNote, [], "hello");

        let result = signer.sign_event(unsigned).await;
        assert_eq!(
            result,
            Err(SignerError::backend(PublicKeySignerError::Readonly))
        );

        Ok(())
    }

    #[tokio::test]
    async fn nip04_encrypt_returns_readonly_error() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let result = signer.nip04_encrypt(&pubkey, "hello").await;
        assert_eq!(
            result,
            Err(SignerError::backend(PublicKeySignerError::Readonly))
        );

        Ok(())
    }

    #[tokio::test]
    async fn nip04_decrypt_returns_readonly_error() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let result = signer.nip04_decrypt(&pubkey, "payload").await;
        assert_eq!(
            result,
            Err(SignerError::backend(PublicKeySignerError::Readonly))
        );

        Ok(())
    }

    #[tokio::test]
    async fn nip44_encrypt_returns_readonly_error() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let result = signer.nip44_encrypt(&pubkey, "hello").await;
        assert_eq!(
            result,
            Err(SignerError::backend(PublicKeySignerError::Readonly))
        );

        Ok(())
    }

    #[tokio::test]
    async fn nip44_decrypt_returns_readonly_error() -> color_eyre::Result<()> {
        let pubkey = example_pubkey()?;
        let signer = PublicKeySigner::new(pubkey);

        let result = signer.nip44_decrypt(&pubkey, "payload").await;
        assert_eq!(
            result,
            Err(SignerError::backend(PublicKeySignerError::Readonly))
        );

        Ok(())
    }
}
