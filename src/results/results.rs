use crate::interface::ResultsTraits;
use async_trait::async_trait;
use redis::{AsyncCommands, Client};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;

const RESULT_PREFIX: &str = "tq:res:";
const SUCCESS: &str = "success";
const FAILED: &str = "failed";

#[derive(Clone)]
pub struct Options {
    pub addrs: Vec<String>,
    pub password: Option<String>,
    pub db: u32,
    pub dial_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub idle_timeout: Duration,
    pub expiry: Duration,
    pub meta_expiry: Duration,
    pub min_idle_conns: usize,
}

#[derive(Clone)]
pub struct Results {
    options: Options,
    client: Client,
}

impl Results {
    pub async fn new(options: Options) -> Self {
        let client = Client::open(options.addrs[0].clone()).expect("Invalid Redis URL");
        let results = Results { options, client };

        if results.options.meta_expiry != Duration::ZERO {
            let expiry = results.options.meta_expiry;
            let results_clone = results.clone(); // Clone `results` here

            tokio::spawn(async move {
                results_clone.start_meta_purger(expiry).await;
            });
        }

        results
    }

    async fn start_meta_purger(&self, ttl: Duration) {
        let mut ticker = interval(ttl);

        loop {
            ticker.tick().await;

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - ttl.as_secs() as i64;

            let score = now.to_string();
            let mut conn = self
                .client
                .get_multiplexed_async_connection()
                .await
                .expect("Redis connection failed");

            for status in [SUCCESS, FAILED] {
                let key = format!("{RESULT_PREFIX}{}", status);
                if let Err(err) = conn.zrembyscore::<_, _, _, i64>(&key, 0, &score).await {
                    eprintln!("Error purging {} metadata: {:?}", status, err);
                }
            }
        }
    }
}

#[async_trait]
impl ResultsTraits for Results {

    async fn get(&self, id: &str) -> Result<Vec<u8>, Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{id}");
        match conn.get(key).await {
            Ok(data) => Ok(data),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn set(&self, id: &str, data: &[u8]) -> Result<(), Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{id}");
        match conn.set_ex::<_, _, ()>(key, data, self.options.expiry.as_secs() as u64).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn delete_job(&self, id: &str) -> Result<(), Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let mut pipeline = redis::pipe();
        let key = format!("{RESULT_PREFIX}{id}");

        pipeline
            .cmd("ZREM")
            .arg(format!("{RESULT_PREFIX}{SUCCESS}"))
            .arg(id)
            .cmd("ZREM")
            .arg(format!("{RESULT_PREFIX}{FAILED}"))
            .arg(id)
            .cmd("DEL")
            .arg(key);

        match pipeline.query_async::<()>( &mut conn).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn get_failed(&self) -> Result<Vec<String>, Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{FAILED}");
        match conn.zrevrangebyscore(key, 0, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64).await {
            Ok(failed) => Ok(failed),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn get_success(&self) -> Result<Vec<String>, Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{SUCCESS}");
        match conn.zrevrangebyscore(key, 0, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64).await {
            Ok(success) => Ok(success),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn set_failed(&self, id: &str) -> Result<(), Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{FAILED}");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as f64;
        match conn.zadd::<_, _, _, i64>(key, id, now).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Arc::new(e)),
        }
    }

    async fn set_success(&self, id: &str) -> Result<(), Arc<dyn Error>> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => return Err(Arc::new(e)),
        };
        let key = format!("{RESULT_PREFIX}{SUCCESS}");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as f64;
        match conn.zadd::<_, _, _, i64>(key, id, now).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Arc::new(e)),
        }
    }
}