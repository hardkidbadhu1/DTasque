use crate::jobs::JobCtx;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Deserialize)]
struct SumPayload {
    arg1: i32,
    arg2: i32,
}

#[derive(Serialize)]
struct SumResult {
    result: i32,
}

// SumProcessor prints the sum of two integer arguments.
pub async fn sum_processor(ctx: &JobCtx, payload: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Deserialize the payload
    let pl: SumPayload = serde_json::from_slice(payload)?;

    // Calculate the sum
    let result = SumResult { result: pl.arg1 + pl.arg2 };

    // Serialize the result
    let rs = serde_json::to_vec(&result)?;

    // Save the result
    ctx.save(&rs).await?;

    Ok(())
}