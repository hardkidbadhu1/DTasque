use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::{self, Receiver, Sender};
use std::error::Error;
use std::time::SystemTime;
use crate::jobs::{JobMessage, JobCtx};
use std::future::Future;
use std::pin::Pin;
use crate::interface::{BrokerTraits, DynError, ResultsTraits};
use rmp_serde::{from_slice, to_vec};

// Constants
pub const STATUS_STARTED: &str = "queued";
pub const STATUS_PROCESSING: &str = "processing";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_DONE: &str = "successful";
pub const STATUS_RETRYING: &str = "retrying";
pub const TRACER: &str = "tasqueue";

// Server struct
pub struct Server {
    pub(crate) broker: Arc<dyn BrokerTraits + Send + Sync>,
    pub(crate) results: Arc<dyn ResultsTraits + Send + Sync>,
    tasks: RwLock<HashMap<String, Task>>,
    queues: RwLock<HashMap<String, u32>>,
    default_conc: usize,
}

pub struct ServerOpts {
    pub(crate) broker: Arc<dyn BrokerTraits + Send + Sync>,
    pub(crate) results: Arc<dyn ResultsTraits + Send + Sync>,
}

type HandlerResult = Result<(), Box<dyn Error + Send + Sync>>;
type Handler = Arc<
    dyn Fn(Vec<u8>, JobCtx) -> Pin<Box<dyn Future<Output = HandlerResult> + Send>>
    + Send
    + Sync,
>;

#[derive(Clone)]
pub struct TaskOpts {
    pub concurrency: u32,
    pub queue: String,
    pub success_cb: Option<Arc<dyn Fn(JobCtx) + Send + Sync>>,
    pub processing_cb: Option<Arc<dyn Fn(JobCtx) + Send + Sync>>,
    pub retrying_cb: Option<Arc<dyn Fn(JobCtx, &dyn Error) + Send + Sync>>,
    pub failed_cb: Option<Arc<dyn Fn(JobCtx, &dyn Error) + Send + Sync>>,
}

#[derive(Clone)]
pub struct Task {
    pub name: String,
    pub handler: Handler,
    pub opts: TaskOpts,
}

impl Server {
    pub fn new(
        broker: Arc<dyn BrokerTraits + Send + Sync>,
        results: Arc<dyn ResultsTraits + Send + Sync>,
    ) -> Self {
        Server {
            broker,
            results,
            tasks: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            default_conc: num_cpus::get(),
        }
    }

    pub fn register_task(
        &self,
        name: String,
        handler: Handler,
        mut opts: TaskOpts,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if opts.queue.is_empty() {
            opts.queue = "default".to_string();
        }
        if opts.concurrency == 0 {
            opts.concurrency = self.default_conc as u32;
        }

        {
            let mut queues = self.queues.write().unwrap();
            if let Some(&existing_conc) = queues.get(&opts.queue) {
                if existing_conc != opts.concurrency {
                    return Err(format!(
                        "Queue '{}' is already defined with concurrency {}",
                        opts.queue, existing_conc
                    )
                        .into());
                }
            } else {
                queues.insert(opts.queue.clone(), opts.concurrency);
            }
        }

        {
            let mut tasks = self.tasks.write().unwrap();
            tasks.insert(
                name.clone(),
                Task {
                    name,
                    handler,
                    opts,
                },
            );
        }

        Ok(())
    }

    fn get_handler(&self, name: &str) -> Option<Task> {
        let tasks = self.tasks.read().unwrap();
        tasks.get(name).cloned()
    }

    pub async fn start(self: Arc<Self>) {
        let queues = {
            let queues_read = self.queues.read().unwrap();
            queues_read.clone()
        };

        for (queue, _conc) in queues {
            // For each queue, spawn one consumer and one processor

            // Create a channel
            let (work_tx, work_rx) = mpsc::channel::<Vec<u8>>(100);

            // Start consumer
            let server_clone = self.clone();
            let queue_clone = queue.clone();
            let work_tx_clone = work_tx.clone();
            tokio::spawn(async move {
                server_clone.consume(queue_clone, work_tx_clone).await;
            });

            // Start processor
            let server_processor = self.clone();
            tokio::spawn(async move {
                server_processor.process(work_rx).await;
            });
        }
    }

    async fn consume(&self, queue: String, sender: Sender<Vec<u8>>) {
        println!("Starting task consumer for queue '{}'", queue);
        self.broker.consume(queue, sender).await;
    }

    async fn process(&self, mut work_rx: Receiver<Vec<u8>>) {
        println!("Starting processor...");
        while let Some(work) = work_rx.recv().await {
            let msg: JobMessage = match from_slice(&work) {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("Error unmarshalling task: {:?}", e);
                    continue;
                }
            };

            let task = match self.get_handler(&msg.job.task) {
                Some(task) => task,
                None => {
                    eprintln!("Handler not found for task '{}'", &msg.job.task);
                    continue;
                }
            };

            let mut msg = msg; // Make msg mutable

            if let Err(e) = self.status_processing(&mut msg).await {
                eprintln!("Error setting status to processing: {:?}", e);
                continue;
            }

            if let Err(e) = self.exec_job(msg, &task).await {
                eprintln!("Could not execute job: {:?}", e);
            }
        }
    }

    async fn exec_job(&self, mut msg: JobMessage, task: &Task) -> Result<(), DynError> {
        let task_ctx = JobCtx {
            store: self.results.clone(),
            meta: msg.meta.clone(),
        };

        // Execute the handler
        let handler_future = (task.handler)(msg.job.payload.clone(), task_ctx.clone());

        let handler_result = match msg.job.opts.timeout {
            Some(timeout) => {
                match tokio::time::timeout(timeout, handler_future).await {
                    Ok(result) => result,
                    Err(_) => Err("Job execution timed out".into()),
                }
            }
            None => handler_future.await,
        };

        if let Err(e) = handler_result {
            // Update msg.meta.prev_err
            msg.meta.prev_err = Some(e.to_string());

            if msg.meta.max_retry > msg.meta.retried {
                msg.meta.retried += 1;
                if let Some(ref retry_cb) = task.opts.retrying_cb {
                    retry_cb(task_ctx.clone(), &*e);
                }
                self.retry_job(&mut msg).await?;
            } else {
                if let Some(ref failed_cb) = task.opts.failed_cb {
                    failed_cb(task_ctx.clone(), &*e);
                }
                self.status_failed(&mut msg).await?;
            }
        } else {
            if let Some(ref success_cb) = task.opts.success_cb {
                success_cb(task_ctx.clone());
            }
            self.status_done(&mut msg).await?;
        }

        Ok(())
    }

    async fn status_processing(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        msg.meta.processed_at = Some(SystemTime::now());
        msg.meta.status = STATUS_PROCESSING.to_string();

        self.set_job_message(msg).await
    }

    pub(crate) async fn status_started(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        msg.meta.processed_at = Some(SystemTime::now());
        msg.meta.status = STATUS_STARTED.to_string();

        self.set_job_message(msg).await
    }

    async fn status_done(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        msg.meta.processed_at = Some(SystemTime::now());
        msg.meta.status = STATUS_DONE.to_string();

        self.results.set_success(&msg.meta.id).await?;
        self.set_job_message(msg).await
    }

    async fn status_failed(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        msg.meta.processed_at = Some(SystemTime::now());
        msg.meta.status = STATUS_FAILED.to_string();

        self.results.set_failed(&msg.meta.id).await?;
        self.set_job_message(msg).await
    }

    async fn status_retrying(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        msg.meta.processed_at = Some(SystemTime::now());
        msg.meta.status = STATUS_RETRYING.to_string();
        self.set_job_message(msg).await
    }

    async fn retry_job(&self, msg: &mut JobMessage) -> Result<(), DynError> {
        self.status_retrying(msg).await?;
        let serialized_msg = to_vec(msg)?;
        self.broker.enqueue(&msg.meta.queue, &serialized_msg).await?;
        Ok(())
    }


    pub async fn get_success(&self) -> Result<Vec<String>, DynError> {
        self.results.get_success().await
        }
}