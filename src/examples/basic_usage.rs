use taskqueue::{Server, ServerOpts, TaskOpts, Job, JobOpts};
use taskqueue::broker::{Broker, Options as BrokerOptions};
use taskqueue::results::{Results, Options as ResultsOptions};
use taskqueue::tasks::tasks::{sum_processor, SumPayload};

use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::oneshot;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Handle Ctrl+C signal
    let (shutdown_tx, _) = oneshot::channel();

    // Spawn a task to listen for shutdown signals
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for event");
        shutdown_tx.send(()).expect("Failed to send shutdown signal");
    });

    // Initialize broker options
    let broker_opts = BrokerOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
        ..Default::default()
    };

    // Initialize results options
    let results_opts = ResultsOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
        meta_expiry: Duration::from_secs(5),
        ..Default::default()
    };

    println!("redis addr {}", broker_opts.addrs[0].to_string());
    // Create broker and results instances
    let broker = Broker::new(broker_opts).await;
    let results = Results::new(results_opts).await;

    // Create the server
    let server = Arc::new(Server::new(
        Arc::new(broker),
        Arc::new(results),
    ));

    // Register the "add" task
    server.register_task(
        "add".to_string(),
        Arc::new(move |payload: Vec<u8>, ctx: taskqueue::JobCtx| {
            Box::pin(async move {
                sum_processor(&ctx, &payload).await
            })
        }),
        TaskOpts {
            concurrency: 5,
            queue: "default".to_string(),
            success_cb: None,
            processing_cb: None,
            retrying_cb: None,
            failed_cb: None,
        },
    )?;

    // Start the server in a background task
    let server_clone = server.clone();
    tokio::spawn(async move {
        server_clone.start().await;
    });

    // Create the payload
    let payload = SumPayload { arg1: 5, arg2: 4 };
    let payload_bytes = serde_json::to_vec(&payload)?;

    // Create the first job
    let job = Job {
        on_success: None,
        task: "add".to_string(),
        payload: payload_bytes.clone(),
        on_error: None,
        opts: JobOpts {
            id: None,
            queue: "default".to_string(),
            max_retries: 3,
            timeout: None,
        },
    };

    // Enqueue the first job
    server.enqueue(job).await?;

    // Create the second job
    let job2 = Job {
        on_success: None,
        task: "add".to_string(),
        payload: payload_bytes,
        on_error: None,
        opts: JobOpts {
            id: None,
            queue: "default".to_string(),
            max_retries: 3,
            timeout: None,
        },
    };

    // Enqueue the second job
    server.enqueue(job2).await?;

    println!("exit..");

    let shutdown_signal = signal::ctrl_c();
    tokio::pin!(shutdown_signal);
    // Periodically check for successful jobs
    loop {
        tokio::select! {
            _ = &mut shutdown_signal => {
                println!("Shutting down...");
                break;
            }
            _ = time::sleep(Duration::from_secs(1)) => {
                let ids = server.get_success().await?;
                println!("Successful Job IDs: {:?}", ids);
            }
        }
    }

    Ok(())
}