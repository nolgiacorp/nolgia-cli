//! Marketplace skills: registry-backed skills served by nolgia-api.
//! Distinct from `nolgia skills` (SKILL.md packs bundled in the binary):
//! marketplace skills are published by Nolgia, installed per agent, and
//! materialized onto the agent pod by `nolgia skill sync` (or the chart's
//! sync initContainer, which does the same thing).

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::{Args, Subcommand};
use nolgia_client::types::{
    PublishSkillRequest, PublishSkillRequestMinTier, PublishSkillRequestName,
    PublishSkillRequestSlug, PublishSkillRequestVersion, Skill, SkillVisibility,
};
use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use crate::output::{OutputFormat, print_json};

use super::CommandContext;

/// Sync marker inside each materialized skill dir; records the synced
/// version. Dirs without it (hand-authored skills) are never touched.
const MARKER: &str = ".nolgia-skill.json";

#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// List the marketplace catalog visible to this account
    List,
    /// Show one marketplace skill
    Show(SlugArgs),
    /// List skills installed for this account's agent
    Installed,
    /// Install a marketplace skill for this account's agent
    Install(SlugArgs),
    /// Uninstall a marketplace skill from this account's agent
    Uninstall(SlugArgs),
    /// Materialize installed skills into a skills directory (what the agent
    /// pod's initContainer runs on boot)
    Sync(SyncArgs),
    /// Publish a skill package directory to the marketplace (admin only)
    Publish(PublishArgs),
}

#[derive(Args, Debug)]
pub struct SlugArgs {
    pub slug: String,
}

#[derive(Args, Debug)]
pub struct SyncArgs {
    /// Target skills directory (default: $HERMES_HOME/skills, HERMES_HOME
    /// defaulting to /opt/data)
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PublishArgs {
    /// Skill package directory containing skill.json + SKILL.md
    pub dir: PathBuf,
}

pub async fn run(command: SkillCommand, ctx: &CommandContext) -> Result<()> {
    match command {
        SkillCommand::List => list(ctx).await,
        SkillCommand::Show(args) => show(args, ctx).await,
        SkillCommand::Installed => installed(ctx).await,
        SkillCommand::Install(args) => install(args, ctx).await,
        SkillCommand::Uninstall(args) => uninstall(args, ctx).await,
        SkillCommand::Sync(args) => sync(args, ctx).await,
        SkillCommand::Publish(args) => publish(args, ctx).await,
    }
}

async fn list(ctx: &CommandContext) -> Result<()> {
    let skills = ctx
        .client()
        .list_skills()
        .send()
        .await
        .context("listing marketplace skills")?
        .into_inner();
    match ctx.format() {
        OutputFormat::Json => print_json(&skills),
        OutputFormat::Text => {
            for skill in &skills {
                print_skill_line(skill);
            }
            Ok(())
        }
    }
}

async fn show(args: SlugArgs, ctx: &CommandContext) -> Result<()> {
    let skill = ctx
        .client()
        .get_skill()
        .slug(&args.slug)
        .send()
        .await
        .context("fetching skill")?
        .into_inner();
    match ctx.format() {
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Text => {
            println!("{} v{}  {}", skill.slug, skill.latest_version, skill.name);
            println!("  {}", skill.description);
            if !skill.min_tier.is_empty() {
                println!(
                    "  requires plan: {} (entitled: {})",
                    skill.min_tier, skill.entitled
                );
            }
            if !skill.credit_cost_hint.is_empty() {
                println!("  credits: {}", skill.credit_cost_hint);
            }
            if !skill.required_env.is_empty() {
                println!("  env: {}", skill.required_env.join(", "));
            }
            Ok(())
        }
    }
}

async fn installed(ctx: &CommandContext) -> Result<()> {
    let skills = ctx
        .client()
        .list_agent_skills()
        .send()
        .await
        .context("listing installed skills")?
        .into_inner();
    match ctx.format() {
        OutputFormat::Json => print_json(&skills),
        OutputFormat::Text => {
            for skill in &skills {
                println!(
                    "{:28} v{:10} {}",
                    skill.slug, skill.latest_version, skill.name
                );
            }
            Ok(())
        }
    }
}

async fn install(args: SlugArgs, ctx: &CommandContext) -> Result<()> {
    let skill = ctx
        .client()
        .install_agent_skill()
        .slug(&args.slug)
        .send()
        .await
        .context("installing skill")?
        .into_inner();
    match ctx.format() {
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Text => {
            println!(
                "installed {} v{} — it lands on the agent pod on its next restart or `nolgia skill sync`",
                skill.slug, skill.latest_version
            );
            Ok(())
        }
    }
}

async fn uninstall(args: SlugArgs, ctx: &CommandContext) -> Result<()> {
    ctx.client()
        .uninstall_agent_skill()
        .slug(&args.slug)
        .send()
        .await
        .context("uninstalling skill")?;
    match ctx.format() {
        OutputFormat::Json => print_json(&serde_json::json!({ "uninstalled": args.slug })),
        OutputFormat::Text => {
            println!("uninstalled {}", args.slug);
            Ok(())
        }
    }
}

// --- sync ---------------------------------------------------------------

#[derive(serde::Serialize)]
struct SyncResult {
    slug: String,
    version: String,
    action: &'static str, // "synced" | "current" | "removed" | "skipped"
}

async fn sync(args: SyncArgs, ctx: &CommandContext) -> Result<()> {
    let root = args.dir.unwrap_or_else(|| {
        let home = std::env::var("HERMES_HOME").unwrap_or_else(|_| "/opt/data".into());
        PathBuf::from(home).join("skills")
    });
    fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

    let installed = ctx
        .client()
        .list_agent_skills()
        .send()
        .await
        .context("listing installed skills")?
        .into_inner();

    let mut results = Vec::new();
    for skill in &installed {
        let dir = root.join(&skill.slug);
        if marker_version(&dir).as_deref() == Some(skill.latest_version.as_str()) {
            results.push(SyncResult {
                slug: skill.slug.clone(),
                version: skill.latest_version.clone(),
                action: "current",
            });
            continue;
        }
        let content = match ctx
            .client()
            .get_skill_content()
            .slug(&skill.slug)
            .send()
            .await
        {
            Ok(response) => response.into_inner(),
            Err(err) => {
                // Entitlement/visibility is enforced server-side per download;
                // skip rather than fail the whole sync.
                eprintln!("skipping {}: {}", skill.slug, err);
                results.push(SyncResult {
                    slug: skill.slug.clone(),
                    version: skill.latest_version.clone(),
                    action: "skipped",
                });
                continue;
            }
        };
        let raw = BASE64
            .decode(&content.content_base64)
            .with_context(|| format!("decoding content for {}", skill.slug))?;
        materialize(&root, &skill.slug, &content.version, &raw)?;
        results.push(SyncResult {
            slug: skill.slug.clone(),
            version: content.version.clone(),
            action: "synced",
        });
    }

    // Remove marketplace-managed dirs no longer in the install list.
    let keep: Vec<&str> = installed.iter().map(|s| s.slug.as_str()).collect();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let dir = entry.path();
        if dir.is_dir()
            && !keep.contains(&name.as_str())
            && let Some(version) = marker_version(&dir)
        {
            fs::remove_dir_all(&dir)
                .with_context(|| format!("removing stale skill {}", dir.display()))?;
            results.push(SyncResult {
                slug: name,
                version,
                action: "removed",
            });
        }
    }

    match ctx.format() {
        OutputFormat::Json => print_json(&results),
        OutputFormat::Text => {
            for r in &results {
                println!("{:8} {} v{}", r.action, r.slug, r.version);
            }
            println!("skills dir: {}", root.display());
            Ok(())
        }
    }
}

fn marker_version(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join(MARKER)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(value.get("version")?.as_str()?.to_string())
}

/// Extract a skill tarball into a staging dir and atomically swap it in.
fn materialize(root: &Path, slug: &str, version: &str, targz: &[u8]) -> Result<()> {
    let staging = root.join(format!(".{slug}.syncing"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let decoder = flate2::read::GzDecoder::new(targz);
    let mut archive = tar::Archive::new(decoder);
    // tar-rs refuses path traversal and absolute paths on unpack.
    archive
        .unpack(&staging)
        .with_context(|| format!("extracting {slug}"))?;

    fs::write(
        staging.join(MARKER),
        serde_json::to_string_pretty(&serde_json::json!({ "slug": slug, "version": version }))?
            + "\n",
    )?;

    let target = root.join(slug);
    let _ = fs::remove_dir_all(&target);
    fs::rename(&staging, &target)
        .with_context(|| format!("activating {} -> {}", staging.display(), target.display()))?;
    Ok(())
}

// --- publish ------------------------------------------------------------

async fn publish(args: PublishArgs, ctx: &CommandContext) -> Result<()> {
    let manifest_path = args.dir.join("skill.json");
    let manifest_raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&manifest_raw).context("parsing skill.json")?;
    if !args.dir.join("SKILL.md").is_file() {
        bail!(
            "{} has no SKILL.md — not a skill package",
            args.dir.display()
        );
    }

    let field = |key: &str| -> Result<String> {
        manifest
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .with_context(|| format!("skill.json is missing required string field {key:?}"))
    };
    let slug: PublishSkillRequestSlug = field("slug")?.parse().context("invalid slug")?;
    let name: PublishSkillRequestName = field("name")?.parse().context("invalid name")?;
    let version: PublishSkillRequestVersion =
        field("version")?.parse().context("invalid version")?;
    let description = manifest
        .get("description")
        .and_then(|v| v.as_str())
        .map(|d| d.parse())
        .transpose()
        .context("invalid description")?;
    let credit_cost_hint = manifest
        .get("credit_cost_hint")
        .and_then(|v| v.as_str())
        .map(|h| h.parse())
        .transpose()
        .context("invalid credit_cost_hint")?;
    let required_env: Vec<String> = manifest
        .get("required_env")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let min_tier: Option<PublishSkillRequestMinTier> = manifest
        .get("min_tier")
        .and_then(|v| v.as_str())
        .map(|t| serde_json::from_value(serde_json::Value::String(t.to_string())))
        .transpose()
        .context("skill.json min_tier is not a valid subscription tier")?;
    let visibility: Option<SkillVisibility> = manifest
        .get("visibility")
        .and_then(|v| v.as_str())
        .map(|t| serde_json::from_value(serde_json::Value::String(t.to_string())))
        .transpose()
        .context("skill.json visibility must be \"public\" or \"private\"")?;

    let content_base64 = BASE64.encode(pack(&args.dir)?);

    let body = PublishSkillRequest {
        slug,
        name,
        version,
        description,
        credit_cost_hint,
        required_env,
        min_tier,
        visibility,
        manifest,
        content_base64,
    };
    let skill = ctx
        .client()
        .publish_skill()
        .body(body)
        .send()
        .await
        .context("publishing skill (admin only)")?
        .into_inner();
    match ctx.format() {
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Text => {
            println!(
                "published {} v{} ({}, min_tier: {})",
                skill.slug,
                skill.latest_version,
                skill.visibility,
                if skill.min_tier.is_empty() {
                    "free"
                } else {
                    &skill.min_tier
                }
            );
            Ok(())
        }
    }
}

/// Tar+gzip the package directory (contents at the archive root).
fn pack(dir: &Path) -> Result<Vec<u8>> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    append_dir(&mut builder, dir, Path::new(""))?;
    Ok(builder.into_inner()?.finish()?)
}

fn append_dir<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    dir: &Path,
    prefix: &Path,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy() == "__pycache__" {
            continue;
        }
        let path = entry.path();
        let archived = prefix.join(&name);
        if path.is_dir() {
            append_dir(builder, &path, &archived)?;
        } else {
            let mut file = fs::File::open(&path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            let mut header = tar::Header::new_gnu();
            header.set_size(buf.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, &archived, buf.as_slice())?;
        }
    }
    Ok(())
}

fn print_skill_line(skill: &Skill) {
    let mut tags = Vec::new();
    if skill.visibility == SkillVisibility::Private {
        tags.push("private".to_string());
    }
    if !skill.min_tier.is_empty() {
        tags.push(format!("requires {}", skill.min_tier));
    }
    if !skill.entitled {
        tags.push("not entitled".to_string());
    }
    let suffix = if tags.is_empty() {
        String::new()
    } else {
        format!("  [{}]", tags.join(", "))
    };
    println!(
        "{:28} v{:10} {}{}",
        skill.slug, skill.latest_version, skill.name, suffix
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_and_materialize_roundtrip() {
        let base = std::env::temp_dir().join(format!("nolgia-skill-test-{}", std::process::id()));
        let src = base.join("src/my-skill");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("SKILL.md"), "---\nname: my-skill\n---\n").unwrap();
        fs::write(src.join("nested/tool.py"), "print('hi')\n").unwrap();
        fs::create_dir_all(src.join("__pycache__")).unwrap();
        fs::write(src.join("__pycache__/junk.pyc"), "x").unwrap();

        let targz = pack(&src).unwrap();
        let root = base.join("skills");
        fs::create_dir_all(&root).unwrap();
        materialize(&root, "my-skill", "1.2.3", &targz).unwrap();

        let installed = root.join("my-skill");
        assert!(installed.join("SKILL.md").is_file());
        assert!(installed.join("nested/tool.py").is_file());
        assert!(!installed.join("__pycache__").exists());
        assert_eq!(marker_version(&installed).as_deref(), Some("1.2.3"));

        // Re-materializing a new version replaces the dir atomically.
        materialize(&root, "my-skill", "1.3.0", &targz).unwrap();
        assert_eq!(marker_version(&installed).as_deref(), Some("1.3.0"));

        fs::remove_dir_all(&base).ok();
    }
}
