use async_trait::async_trait;
use thiserror::Error;


#[derive(Error, Debug)]
pub enum KVStoreErrors {
    #[error("key not found : {0}")]
    KeyNotFound(String),
    #[error("set too large : {0}")]
    TooLarge(String),
    #[error("internal error : {0}")]
    InternalError(String),

}

pub type Result<T> = std::result::Result<T, KVStoreErrors>; 

#[async_trait]
pub trait KVStoreProvider: Send + Sync {
    async fn get(&self, key: String) -> Result<String>;
    async fn set(&self, key: String, value: String) -> Result<()>;
}
