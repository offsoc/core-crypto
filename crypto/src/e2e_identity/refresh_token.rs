use super::error::{Error, Result};
use crate::KeystoreError;
use core_crypto_keystore::connection::FetchFromDatabase;
use core_crypto_keystore::entities::E2eiRefreshToken;
use mls_crypto_provider::MlsCryptoProvider;
use zeroize::Zeroize;

/// An OIDC refresh token managed by CoreCrypto to benefit from encryption-at-rest
#[derive(Debug, serde::Serialize, serde::Deserialize, Zeroize, derive_more::From, derive_more::Deref)]
#[zeroize(drop)]
pub struct RefreshToken(String);

impl RefreshToken {
    pub(crate) async fn find(key_store: &impl FetchFromDatabase) -> Result<RefreshToken> {
        key_store
            .find_unique::<E2eiRefreshToken>()
            .await
            .map_err(KeystoreError::wrap("finding refresh token"))?
            .try_into()
    }

    pub(crate) async fn replace(self, backend: &MlsCryptoProvider) -> Result<()> {
        let keystore = backend.keystore();
        let rt = E2eiRefreshToken::from(self);
        keystore
            .save(rt)
            .await
            .map_err(KeystoreError::wrap("replacing refresh token"))?;
        Ok(())
    }
}

impl TryFrom<E2eiRefreshToken> for RefreshToken {
    type Error = Error;

    fn try_from(mut entity: E2eiRefreshToken) -> Result<Self> {
        let content = std::mem::take(&mut entity.content);
        let content = String::from_utf8(content).map_err(|_| Error::InvalidRefreshToken)?;
        Ok(Self(content))
    }
}

impl From<RefreshToken> for E2eiRefreshToken {
    fn from(mut rt: RefreshToken) -> Self {
        let content = std::mem::take(&mut rt.0);
        Self {
            content: content.into_bytes(),
        }
    }
}
