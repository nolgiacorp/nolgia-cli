use anyhow::{Context, Result};
use clap::Args;
use std::num::NonZeroU64;
use uuid::Uuid;

use crate::output::{OutputFormat, print_json};

use super::CommandContext;

#[derive(Args, Debug)]
pub struct WaitArgs {
    pub job_id: Uuid,
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

pub async fn run(args: WaitArgs, ctx: &CommandContext) -> Result<()> {
    let timeout = NonZeroU64::new(args.timeout).context("--timeout must be greater than zero")?;
    let job = ctx
        .client()
        .wait_for_job()
        .id(args.job_id)
        .timeout_seconds(timeout)
        .send()
        .await
        .context("waiting for job")?
        .into_inner();

    match ctx.format() {
        OutputFormat::Json => print_json(&job),
        OutputFormat::Text => {
            println!("{} {} {}", job.id, job.modality, job.status);
            Ok(())
        }
    }
}
