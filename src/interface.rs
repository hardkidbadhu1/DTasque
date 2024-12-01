use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedSender};

// Define the Result trait
#[async_trait]
pub trait ResultsTraits: Send + Sync{
    async fn get(&self, id: &str) -> Result<Vec<u8>, Arc<dyn Error>>;
    // fn nil_error(&self) -> Result<(), Arc<dyn Error>>;
    async fn set(&self, id: &str, data: &[u8]) -> Result<(), Arc<dyn Error>>;
    async fn delete_job(&self, id: &str) -> Result<(), Arc<dyn Error>>;
    async fn get_failed(&self) -> Result<Vec<String>, Arc<dyn Error>>;
    async fn get_success(&self) -> Result<Vec<String>, Arc<dyn Error>>;
    async fn set_failed(&self, id: &str) -> Result<(), Arc<dyn Error>>;
    async fn set_success(&self, id: &str) -> Result<(), Arc<dyn Error>>;
}

// Define the Broker trait
#[async_trait]
pub trait BrokerTraits: Send + Sync {
    async fn enqueue(&self, queue: &str, message: &[u8]) -> Result<(),Arc<dyn Error>>;
    async fn consume(&self, queue: String, sender: Sender<Vec<u8>>);
    async fn get_pending(&self, queue: &str) -> Result<Vec<String>, Arc<dyn Error>>;
}