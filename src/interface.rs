use async_trait::async_trait;
use std::error::Error;
use tokio::sync::mpsc::Sender;

pub type DynError = Box<dyn Error + Send + Sync>;

// Define the Broker trait
#[async_trait]
pub trait BrokerTraits: Send + Sync {
    async fn enqueue(&self, queue: &str, message: &[u8]) -> Result<(), DynError>;
    async fn consume(&self, queue: String, sender: Sender<Vec<u8>>);
    async fn get_pending(&self, queue: &str) -> Result<Vec<String>, DynError>;
}

#[async_trait]
pub trait ResultsTraits: Send + Sync {
    async fn get(&self, key: &str) -> Result<Vec<u8>, DynError>;
    async fn set(&self, key: &str, value: &[u8]) -> Result<(), DynError>;
    async fn delete_job(&self, id: &str) -> Result<(), DynError>;
    async fn set_success(&self, id: &str) -> Result<(), DynError>;
    async fn set_failed(&self, id: &str) -> Result<(), DynError>;
    async fn get_success(&self) -> Result<Vec<String>, DynError>;
    async fn get_failed(&self) -> Result<Vec<String>, DynError>;
}