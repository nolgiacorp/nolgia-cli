use anyhow::{Context, Result};
use clap::Args;
use uuid::Uuid;

use crate::output::{print_json, OutputFormat};

use super::CommandContext;

#[derive(Args, Debug)]
pub struct StatusArgs {
    pub job_id: Uuid,
}

pub async fn run(args: StatusArgs, ctx: &CommandContext) -> Result<()> {
    let job = ctx
        .client()
        .get_job()
        .id(args.job_id)
        .send()
        .await
        .context("fetching job status")?
        .into_inner();

    match ctx.format() {
        OutputFormat::Json => print_json(&job),
        OutputFormat::Text => {
            println!("{} {} {}", job.id, job.modality, job.status);
            Ok(())
        }
    }
}
