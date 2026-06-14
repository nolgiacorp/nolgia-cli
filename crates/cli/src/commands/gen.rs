use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nolgia_client::types::{
    AudioFormat, AudioModel, GenerateAudioRequest, GenerateImageRequest, GenerateVideoRequest,
    ImageModel, VideoModel,
};
use serde::Serialize;
use std::{fs, path::PathBuf};
use uuid::Uuid;

use crate::output::{print_json, OutputFormat};

use super::CommandContext;

#[derive(Subcommand, Debug)]
pub enum GenCommand {
    Image(ImageArgs),
    Video(VideoArgs),
    Audio(AudioArgs),
}

#[derive(Args, Debug)]
pub struct ImageArgs {
    #[arg(long, default_value = "fal-ai/flux-pro/v1.1")]
    pub model: ImageModel,
    #[arg(long)]
    pub prompt: String,
    #[arg(long)]
    pub input: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub wait: bool,
    #[arg(long, default_value_t = false)]
    pub no_wait: bool,
}

#[derive(Args, Debug)]
pub struct VideoArgs {
    #[arg(long, default_value = "fal-ai/kling-video/v3/text-to-video")]
    pub model: VideoModel,
    #[arg(long)]
    pub prompt: String,
    #[arg(long)]
    pub input: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub wait: bool,
    #[arg(long, default_value_t = false)]
    pub no_wait: bool,
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args, Debug)]
pub struct AudioArgs {
    #[arg(long, default_value = "fal-ai/stable-audio-25/text-to-audio")]
    pub model: AudioModel,
    #[arg(long)]
    pub prompt: String,
    #[arg(long)]
    pub input: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long, default_value = "mp3")]
    pub format: AudioFormat,
    #[arg(long, default_value_t = false)]
    pub wait: bool,
    #[arg(long, default_value_t = false)]
    pub no_wait: bool,
}

#[derive(Serialize)]
struct AsyncJob {
    job_id: String,
}

pub async fn run(command: GenCommand, ctx: &CommandContext) -> Result<()> {
    match command {
        GenCommand::Image(args) => image(args, ctx).await,
        GenCommand::Video(args) => video(args, ctx).await,
        GenCommand::Audio(args) => audio(args, ctx).await,
    }
}

async fn image(args: ImageArgs, ctx: &CommandContext) -> Result<()> {
    let body: GenerateImageRequest = GenerateImageRequest::builder()
        .model(args.model)
        .prompt(args.prompt)
        .try_into()
        .context("building image request")?;
    let response = ctx.client().generate_image().body(body).send().await.context("generating image")?.into_inner();
    if args.no_wait {
        return print_json(&AsyncJob { job_id: response.request_id });
    }
    if let Some(out) = args.out {
        download(&response.asset.signed_url, &out).await?;
    }
    match ctx.format() {
        OutputFormat::Json => print_json(&response),
        OutputFormat::Text => {
            println!("{}", response.asset.signed_url);
            Ok(())
        }
    }
}

async fn video(args: VideoArgs, ctx: &CommandContext) -> Result<()> {
    let body: GenerateVideoRequest = GenerateVideoRequest::builder()
        .model(args.model)
        .prompt(args.prompt)
        .try_into()
        .context("building video request")?;
    let mut job = ctx.client().generate_video().body(body).send().await.context("submitting video job")?.into_inner();
    if args.no_wait || !args.wait {
        return print_json(&AsyncJob { job_id: job.id.to_string() });
    }
    let timeout = std::num::NonZeroU64::new(args.timeout).context("--timeout must be greater than zero")?;
    job = ctx.client().wait_for_job().id(job.id).timeout_seconds(timeout).send().await.context("waiting for video job")?.into_inner();
    if let (Some(asset), Some(out)) = (&job.asset, args.out.as_ref()) {
        download(&asset.signed_url, out).await?;
    }
    match ctx.format() {
        OutputFormat::Json => print_json(&job),
        OutputFormat::Text => {
            println!("{} {}", job.id, job.status);
            Ok(())
        }
    }
}

async fn audio(args: AudioArgs, ctx: &CommandContext) -> Result<()> {
    let body: GenerateAudioRequest = GenerateAudioRequest::builder()
        .model(args.model)
        .prompt(args.prompt)
        .format(args.format)
        .try_into()
        .context("building audio request")?;
    let response = ctx.client().generate_audio().body(body).send().await.context("generating audio")?.into_inner();
    if args.no_wait {
        return print_json(&AsyncJob { job_id: Uuid::new_v4().to_string() });
    }
    if let Some(out) = args.out {
        download(&response.asset.signed_url, &out).await?;
    }
    match ctx.format() {
        OutputFormat::Json => print_json(&response),
        OutputFormat::Text => {
            println!("{}", response.asset.signed_url);
            Ok(())
        }
    }
}

async fn download(url: &str, out: &PathBuf) -> Result<()> {
    let bytes = reqwest::get(url).await.with_context(|| format!("downloading {url}"))?.bytes().await?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))
}
