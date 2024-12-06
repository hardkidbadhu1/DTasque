# TaskQueue - A Simple Asynchronous Task Queue Library for Rust

[![Crates.io](https://img.shields.io/crates/v/taskqueue.svg)](https://crates.io/crates/taskqueue)

TaskQueue is a simple, asynchronous task queue library for Rust, designed to handle background jobs and processing tasks using Redis as a broker and result store. It provides a straightforward API to define tasks, enqueue jobs, and process them asynchronously with concurrency support.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Getting Started](#getting-started)
    - [Prerequisites](#prerequisites)
    - [Setting Up the Server](#setting-up-the-server)
    - [Defining a Task](#defining-a-task)
    - [Registering a Task](#registering-a-task)
    - [Enqueuing Jobs](#enqueuing-jobs)
- [Examples](#examples)

## Features

- **Asynchronous Processing**: Built on top of Tokio for efficient async task execution.
- **Concurrency Control**: Define concurrency limits per task.
- **Redis Backend**: Uses Redis for task brokering and result storage.
- **Customizable Task Handling**: Define your own tasks with custom payloads and processing logic.
- **Result Storage**: Save and retrieve results of task execution.
- **Graceful Shutdown**: Handles graceful termination of tasks on shutdown signals.
- **Extensible Architecture**: Modular design allows for customization and extension.

## Installation

Add TaskQueue to your `Cargo.toml` dependencies:

```toml
[dependencies]
taskqueue = "0.1.0"
```
Ensure that you have Redis installed and running. You can download Redis from redis.io or install it via your package manager.

## Getting Started

### Prerequisites

	•	Rust: Ensure you have Rust installed. You can download it from rust-lang.org.
	•	Redis: Install and run a Redis server instance.

### Setting Up the Server

First, initialize the broker and results backends using Redis:
```rust
use taskqueue::broker::{Broker, Options as BrokerOptions};
use taskqueue::results::{Results, Options as ResultsOptions};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure broker options
    let broker_opts = BrokerOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
        ..Default::default()
    };
    let broker = Broker::new(broker_opts).await;

    // Configure results options
    let results_opts = ResultsOptions {
        addrs: vec!["127.0.0.1:6379".to_string()],
        password: None,
        db: 0,
        meta_expiry: Duration::from_secs(300),
        ..Default::default()
    };
    let results = Results::new(results_opts).await;

    // Create the server instance
    let server = Arc::new(taskqueue::Server::new(
        Arc::new(broker),
        Arc::new(results),
    ));

    // ... (rest of your code)

    Ok(())
}
```

### Defining a Task

Define a task by implementing the processing logic. For example, let’s create a task that sums two integers.

```
use taskqueue::jobs::JobCtx;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Serialize, Deserialize)]
pub struct SumPayload {
    pub arg1: i32,
    pub arg2: i32,
}

#[derive(Serialize)]
pub struct SumResult {
    pub result: i32,
}

pub async fn sum_processor(ctx: &JobCtx, payload: &[u8]) -> Result<()> {
    // Deserialize the payload
    let pl: SumPayload = serde_json::from_slice(payload)?;

    // Perform the computation
    let result = SumResult {
        result: pl.arg1 + pl.arg2,
    };

    // Serialize and save the result
    let rs = serde_json::to_vec(&result)?;
    ctx.save(&rs).await?;

    Ok(())
}
```

### Registering a Task

Register the task with the server, specifying concurrency and other options.

```
use std::sync::Arc;
use taskqueue::{Server, TaskOpts, JobCtx};

let server_clone = server.clone();
server.register_task(
    "add".to_string(),
    Arc::new(move |payload: Vec<u8>, ctx: JobCtx| {
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
```

Enqueuing Jobs

Create and enqueue jobs to be processed.

```
use taskqueue::{Job, JobOpts};
use serde_json;

let payload = SumPayload { arg1: 5, arg2: 4 };
let payload_bytes = serde_json::to_vec(&payload)?;

let job = Job {
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

server.enqueue(job).await?;
```

## Examples

An example demonstrating the usage of TaskQueue can be found in the examples directory.

To run the example:

```
cargo run --example basic_usage
```

