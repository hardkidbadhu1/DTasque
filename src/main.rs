use taskqueue::server::{Server, ServerOpts};
use taskqueue::broker::{Broker, RedisBroker, RedisBrokerOptions};
use taskqueue::results::{Results, RedisResults, RedisResultsOptions};
use taskqueue::jobs::{Job, JobOpts};
use tokio::signal;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Set up signal handling for graceful shutdown
    let ctx = signal::ctrl_c();

    // Initialize the server with Redis-based broker and results store
    let broker = Arc::new(RedisBroker::new(RedisBrokerOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
    }));
    let results = Arc::new(RedisResults::new(RedisResultsOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
        meta_expiry: Some(std::time::Duration::from_secs(5)),
    }));

    let server = Server::new(ServerOpts {
        broker,
        results,
    });

    // Register task "add"
    server
        .register_task(
            "add".to_string(),
            Arc::new(|payload, ctx| {
                tokio::spawn(async move {
                    // Deserialize the payload
                    let data: Result<SumPayload, _> = serde_json::from_slice(&payload);
                    match data {
                        Ok(payload) => {
                            let result = payload.arg1 + payload.arg2;
                            ctx.save(result.to_string().into_bytes()).await?;
                            Ok(())
                        }
                        Err(err) => Err(err.to_string()),
                    }
                })
            }),
            taskqueue::server::TaskOpts {
                concurrency: 5,
                queue: "default".to_string(),
            },
        )
        .await
        .expect("Failed to register task");

    // Start the server in a separate async task
    tokio::spawn(async move {
        server.start().await;
    });

    // Enqueue jobs
    let payload = json!({ "arg1": 5, "arg2": 4 }).to_string();
    let job = Job::new(
        "add".to_string(),
        payload.into_bytes(),
        JobOpts {
            id: None,
            queue: "default".to_string(),
            max_retries: 3,
            timeout: None,
        },
    );
    server
        .enqueue(job)
        .await
        .expect("Failed to enqueue job");

    let payload = json!({ "arg1": 10, "arg2": 15 }).to_string();
    let job = Job::new(
        "add".to_string(),
        payload.into_bytes(),
        JobOpts {
            id: None,
            queue: "default".to_string(),
            max_retries: 3,
            timeout: None,
        },
    );
    server
        .enqueue(job)
        .await
        .expect("Failed to enqueue job");

    println!("Server started...");

    // Monitor results periodically
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = ctx => {
                println!("Shutting down...");
                break;
            }
            _ = interval.tick() => {
                let success_ids = server
                    .get_success()
                    .await
                    .expect("Failed to fetch successful job IDs");
                println!("Successful Job IDs: {:?}", success_ids);
            }
        }
    }
}

// Define the payload structure
#[derive(serde::Deserialize, serde::Serialize)]
struct SumPayload {
    arg1: i32,
    arg2: i32,
}
