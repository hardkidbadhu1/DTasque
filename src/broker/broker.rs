use std::error::Error;
use std::sync::Arc;
use redis::{AsyncCommands, Client};use std::time::Duration;
use tokio::time;
use async_trait::async_trait;
use futures::SinkExt;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedSender};
use tokio::sync::Mutex;
use crate::interface::BrokerTraits;

pub struct Options {
    pub addrs: Vec<String>,
    pub password: Option<String>,
    pub db: u32,
    pub dial_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub idle_timeout: Duration,
    pub min_idle_conns: usize,
    pub poll_period: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            addrs: vec!["127.0.0.1:6379".to_string()],
            password: None,
            db: 0,
            dial_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            min_idle_conns: 1,
            poll_period: Duration::from_secs(1),
        }
    }
}

pub struct Broker {
    opts: Options,
    client: Client,
}

impl Broker {
    pub async fn new(opts: Options) -> Self {
        let connection_string = opts.addrs[0].clone(); // Assuming a single address for simplicity.
        let client = Client::open(connection_string).expect("Invalid Redis URL");

        Broker { opts, client }
    }
}

#[async_trait]
impl BrokerTraits for Broker {
    async fn enqueue(&self, queue: &str, message: &[u8]) -> Result<(),Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };

        match conn.lpush::<_, _, i64>(queue, message).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn consume(&self, queue: String, sender: Sender<Vec<u8>>) {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to Redis: {:?}", e);
                return;
            }
        };

        loop {
            match time::timeout(
                self.opts.poll_period,
                conn.blpop::<_, (String, Vec<u8>)>(
                    queue.clone(),
                    self.opts.poll_period.as_secs_f64(),
                ),
            )
                .await
            {
                Ok(Ok((_, message))) => {
                    let mut sender_guard = sender.lock().await;
                    if sender_guard.send(message).is_err() {
                        eprintln!("Work sender dropped; shutting down consumer.");
                        break;
                    }
                    // Sender guard is dropped here when it goes out of scope
                }
                Ok(Err(e)) => {
                    eprintln!("Error while consuming messages: {:?}", e);
                }
                Err(_) => {
                    eprintln!("Timeout; continue polling.");
                }
            }
        }
    }

    async fn get_pending(&self, queue: &str) -> Result<Vec<String>, Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };

        match conn.lrange(queue, 0, -1).await {
            Ok(pending) => Ok(pending),
            Err(e) => Err(Arc::new(e)),
        }
    }
}