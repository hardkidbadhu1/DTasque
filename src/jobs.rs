use std::error::Error;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use crate::interface::ResultsTraits;
use crate::server::{Server};
use rmp_serde::{to_vec};


#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub on_success: Option<Vec<Job>>,
    pub task: String,
    pub payload: Vec<u8>,
    pub on_error: Option<Vec<Job>>,
    pub opts: JobOpts,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobOpts {
    pub id: Option<String>,
    pub queue: String,
    pub max_retries: u32,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub on_success_ids: Vec<String>,
    pub status: String,
    pub queue: String,
    pub max_retry: u32,
    pub retried: u32,
    pub prev_err: Option<String>,
    pub processed_at: Option<SystemTime>,
    pub prev_job_result: Option<Vec<u8>>,
}

impl Meta {
    pub fn default_meta(opts: &JobOpts) -> Self {
        Meta {
            id: opts.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            on_success_ids: vec![],
            status: "started".to_string(),
            queue: opts.queue.clone(),
            max_retry: opts.max_retries,
            retried: 0,
            prev_err: None,
            processed_at: None,
            prev_job_result: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobMessage {
    pub meta: Meta,
    pub job: Job,
}

impl Job {
    pub fn message(self, meta: Meta) -> JobMessage {
        JobMessage {
            meta,
            job: self,
        }
    }
}

pub struct JobCtx {
    pub store: Arc<dyn ResultsTraits>,
    pub meta: Meta,
}

impl JobCtx {
    pub async fn save(&self, b: &[u8]) -> Result<(),Arc<dyn Error>> {
        self.store.set(&self.meta.id, b).await
    }
}

impl Server {
    pub async fn enqueue(&self, job: Job) -> Result<String, Box<dyn Error>> {
        let meta = Meta::default_meta(&job.opts);
        self.enqueue_with_meta(job, meta).await
    }
}

impl Server {
    async fn enqueue_with_meta(&self, job: Job, meta: Meta) -> Result<String, Box<dyn Error>> {
        let mut msg = job.message(meta);

        // Set job status in the results backend
        self.status_started(&mut msg).await?;

        // Enqueue the message
        self.enqueue_message(&msg).await?;

        Ok(msg.meta.id.clone())
    }
}

impl Server {
    async fn enqueue_message(&self, msg: &JobMessage) -> Result<(), Box<dyn Error>> {
        let serialized_msg = rmp_serde::to_vec(msg)?;
        self.broker.enqueue(&msg.meta.queue, &serialized_msg).await?;
        Ok(())
    }
}


const JOB_PREFIX: &str = "job:msg:";

impl Server {
    pub(crate) async fn set_job_message(&self, msg: &JobMessage) -> Result<(), Box<dyn Error>> {
        let key = format!("{}{}", JOB_PREFIX, msg.meta.id);
        let serialized_msg = rmp_serde::to_vec(msg)?;
        self.results.set(&key, &serialized_msg).await?;
        Ok(())
    }
}

impl Server {
    pub async fn get_job(&self, id: &str) -> Result<JobMessage, Box<dyn Error>> {
        let key = format!("{}{}", JOB_PREFIX, id);
        let data = self.results.get(&key).await?;
        let msg: JobMessage = rmp_serde::from_slice(&data)?;
        Ok(msg)
    }
}
