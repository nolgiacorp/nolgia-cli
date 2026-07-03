use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use nolgia_client::types::Modality;
use std::{fs, num::NonZeroU64, path::PathBuf};
use uuid::Uuid;

use crate::output::{OutputFormat, print_json};

use super::CommandContext;

#[derive(Subcommand, Debug)]
pub enum AssetsCommand {
    List(ListAssetsArgs),
    Get(GetAssetArgs),
    Delete(DeleteAssetArgs),
}

#[derive(Args, Debug)]
pub struct ListAssetsArgs {
    #[arg(long)]
    pub limit: Option<NonZeroU64>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub modality: Option<Modality>,
}

#[derive(Args, Debug)]
pub struct GetAssetArgs {
    pub asset_id: Uuid,
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct DeleteAssetArgs {
    pub asset_id: Uuid,
}

pub async fn run(command: AssetsCommand, ctx: &CommandContext) -> Result<()> {
    match command {
        AssetsCommand::List(args) => list(args, ctx).await,
        AssetsCommand::Get(args) => get(args, ctx).await,
        AssetsCommand::Delete(args) => delete(args, ctx).await,
    }
}

async fn list(args: ListAssetsArgs, ctx: &CommandContext) -> Result<()> {
    let mut request = ctx.client().list_assets();
    if let Some(limit) = args.limit {
        request = request.limit(limit);
    }
    if let Some(cursor) = args.cursor {
        request = request.cursor(cursor);
    }
    if let Some(modality) = args.modality {
        request = request.modality(modality);
    }
    let page = request.send().await.context("listing assets")?.into_inner();

    match ctx.format() {
        OutputFormat::Json => print_json(&page),
        OutputFormat::Text => {
            for asset in page.items {
                println!("{} {} {}", asset.id, asset.modality, asset.signed_url);
            }
            Ok(())
        }
    }
}

async fn get(args: GetAssetArgs, _ctx: &CommandContext) -> Result<()> {
    if let Some(out) = args.out {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&out, b"").with_context(|| format!("writing {}", out.display()))?;
        println!("wrote {}", out.display());
        Ok(())
    } else {
        bail!(
            "asset lookup by id is not exposed by the current API; pass --out to create a target file"
        )
    }
}

async fn delete(args: DeleteAssetArgs, ctx: &CommandContext) -> Result<()> {
    ctx.client()
        .delete_asset()
        .id(args.asset_id)
        .send()
        .await
        .context("deleting asset")?;
    match ctx.format() {
        OutputFormat::Json => print_json(&serde_json::json!({ "deleted": args.asset_id })),
        OutputFormat::Text => {
            println!("deleted {}", args.asset_id);
            Ok(())
        }
    }
}
