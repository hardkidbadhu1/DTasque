use redis::{AsyncCommands, Client};use std::time::Duration;
use tokio::time;
use async_trait::async_trait;
use tokio::sync::mpsc::{Sender};
use crate::interface::{BrokerTraits,DynError};

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
        let addr = opts.addrs[0].clone(); // Assuming a single address for simplicity.
        let mut url = String::new();

        if let Some(ref pwd) = opts.password {
            url = format!("redis://:{}@{}/", pwd, addr);
        } else {
            url = format!("redis://{}/", addr);
        }

        if opts.db != 0 {
            url = format!("{}{}/", url.trim_end_matches('/'), opts.db);
        }

        let client = Client::open(url.clone()).expect(&format!("Invalid Redis URL: {}", url));

        Broker { opts, client }
    }
}

#[async_trait]
impl BrokerTraits for Broker {
    async fn enqueue(&self, queue: &str, message: &[u8]) -> Result<(), DynError> {
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(|e| Box::new(e) as DynError)?;
        conn.lpush::<_, _, i64>(queue, message).await.map(|_| ()).map_err(|e| Box::new(e) as DynError)
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
                    if sender.send(message).await.is_err() {
                        eprintln!("Work sender dropped; shutting down consumer.");
                        break;
                    }
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

    async fn get_pending(&self, queue: &str) -> Result<Vec<String>, DynError> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Box::new(e) as DynError),
        };

        match conn.lrange(queue, 0, -1).await {
            Ok(pending) => Ok(pending),
            Err(e) => Err(Box::new(e) as DynError),
        }
    }
}