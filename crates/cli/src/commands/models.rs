//! Live model catalog: the server is the source of truth for models,
//! capabilities, and pricing — new models appear here with no CLI release.

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use nolgia_client::types::{Modality, Model};

use super::CommandContext;
use crate::output::{OutputFormat, print_json};

#[derive(Subcommand, Debug)]
pub enum ModelsCommand {
    /// List available models with capabilities and credit pricing
    List(ListArgs),
    /// Show one model's capabilities, pricing, and (for audio) voices
    Get(GetArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(long, value_enum)]
    pub modality: Option<ModalityFilter>,
}

#[derive(Args, Debug)]
pub struct GetArgs {
    pub id: String,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ModalityFilter {
    Image,
    Video,
    Audio,
}

impl ModalityFilter {
    fn matches(self, modality: &Modality) -> bool {
        matches!(
            (self, modality),
            (Self::Image, Modality::Image)
                | (Self::Video, Modality::Video)
                | (Self::Audio, Modality::Audio)
        )
    }
}

pub async fn run(command: ModelsCommand, ctx: &CommandContext) -> Result<()> {
    match command {
        ModelsCommand::List(args) => list(args, ctx).await,
        ModelsCommand::Get(args) => get(args, ctx).await,
    }
}

async fn fetch(ctx: &CommandContext) -> Result<Vec<Model>> {
    Ok(ctx
        .client()
        .list_models()
        .send()
        .await
        .context("fetching model catalog")?
        .into_inner()
        .models)
}

fn cost_line(model: &Model) -> String {
    match &model.cost {
        Some(cost) => {
            let unit = format!("{:?}", cost.unit).to_lowercase();
            match cost.baseline_seconds {
                Some(base) => format!("{} credits per {base}s clip", cost.credits),
                None => format!("{} credits ({unit})", cost.credits),
            }
        }
        None => "pricing pending".to_string(),
    }
}

fn capability_line(model: &Model) -> String {
    if let Some(video) = &model.video {
        let durations = if !video.durations.is_empty() {
            video
                .durations
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("/")
                + "s"
        } else if let (Some(min), Some(max)) = (video.min_duration, video.max_duration) {
            format!("{min}-{max}s")
        } else {
            String::new()
        };
        let ratios = video
            .aspect_ratios
            .iter()
            .map(|r| format!("{r:?}").replace('N', "").replace('_', ":"))
            .collect::<Vec<_>>()
            .join(" ");
        let image_input = if video.image_input == Some(true) {
            "  image-input"
        } else {
            ""
        };
        return format!("{durations}  {ratios}{image_input}")
            .trim()
            .to_string();
    }
    if let Some(audio) = &model.audio
        && !audio.voices.is_empty()
    {
        return format!("{} voices", audio.voices.len());
    }
    String::new()
}

/// Estimate the credit charge for a video generation from the live catalog.
/// Mirrors the server: ceil(credits * duration / baseline_seconds).
pub async fn quote_video(
    ctx: &CommandContext,
    model_id: &str,
    duration_seconds: u64,
) -> Result<String> {
    let models = fetch(ctx).await?;
    let model = models.iter().find(|m| m.id == model_id).with_context(|| {
        format!("model {model_id:?} not in the catalog — see `nolgia models list`")
    })?;
    let Some(cost) = &model.cost else {
        return Ok(format!(
            "{model_id}: pricing pending — the server will quote at submit time"
        ));
    };
    let credits = match cost.baseline_seconds {
        Some(base) => (cost.credits.get() * duration_seconds).div_ceil(base.get()),
        None => cost.credits.get(),
    };
    Ok(format!(
        "{credits} credits ({model_id}, {duration_seconds}s)"
    ))
}

async fn list(args: ListArgs, ctx: &CommandContext) -> Result<()> {
    let mut models = fetch(ctx).await?;
    if let Some(filter) = args.modality {
        models.retain(|m| filter.matches(&m.modality));
    }
    match ctx.format() {
        OutputFormat::Json => print_json(&models),
        OutputFormat::Text => {
            for model in &models {
                let star = if model.recommended { "*" } else { " " };
                println!(
                    "{star} {:52} {:7} {:28} {}",
                    model.id,
                    format!("{:?}", model.modality).to_lowercase(),
                    cost_line(model),
                    capability_line(model),
                );
            }
            println!("\n* recommended. `nolgia models get <id>` for details.");
            Ok(())
        }
    }
}

async fn get(args: GetArgs, ctx: &CommandContext) -> Result<()> {
    let models = fetch(ctx).await?;
    let model = models.iter().find(|m| m.id == args.id).with_context(|| {
        format!(
            "model {:?} not in the catalog — see `nolgia models list`",
            args.id
        )
    })?;
    match ctx.format() {
        OutputFormat::Json => print_json(model),
        OutputFormat::Text => {
            println!("{}", model.id);
            println!("  modality:    {:?}", model.modality);
            println!("  recommended: {}", model.recommended);
            println!("  cost:        {}", cost_line(model));
            let caps = capability_line(model);
            if !caps.is_empty() {
                println!("  supports:    {caps}");
            }
            if let Some(audio) = &model.audio
                && !audio.voices.is_empty()
            {
                for voice in &audio.voices {
                    match &voice.label {
                        Some(label) => println!("  voice:       {}  ({label})", voice.id),
                        None => println!("  voice:       {}", voice.id),
                    }
                }
            }
            Ok(())
        }
    }
}
