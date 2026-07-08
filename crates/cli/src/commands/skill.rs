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
    /// Scaffold a new skill authoring directory (skill.json, SKILL.md,
    /// payload/)
    Init(InitArgs),
    /// Validate a skill authoring directory and assemble the publishable
    /// package.
    ///
    /// Copies skill.json + SKILL.md and the contents of payload/ (which land
    /// at the package root, next to SKILL.md) into the output directory,
    /// ready for `nolgia skill publish`. The optional manifest field
    /// `python_requirements` (an array of pip requirement strings) is passed
    /// through to the marketplace manifest verbatim.
    Pack(PackArgs),
    /// Publish a skill package directory to the marketplace (admin only).
    ///
    /// The authoring loop is `skill init <slug>` to scaffold, `skill pack
    /// <dir>` to validate and assemble, then `skill publish dist/<slug>`.
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
pub struct InitArgs {
    /// Skill slug: lowercase letters, digits, hyphens (max 64 chars)
    pub slug: String,
    /// Directory to create (default: ./<slug>)
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PackArgs {
    /// Skill authoring directory (from `nolgia skill init`)
    pub dir: PathBuf,
    /// Output package directory (default: dist/<slug>)
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct PublishArgs {
    /// Skill package directory containing skill.json + SKILL.md
    /// (assembled by `nolgia skill pack`)
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
        SkillCommand::Init(args) => init(args, ctx),
        SkillCommand::Pack(args) => pack(args, ctx),
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

// --- init / pack (authoring) ---------------------------------------------

/// Server-side decoded package size limit (skill_versions.content).
const MAX_PACKAGE_BYTES: u64 = 5 * 1024 * 1024;

/// Server-side slug rule (PublishSkillRequest.slug: ^[a-z0-9][a-z0-9-]{0,63}$).
fn is_valid_slug(slug: &str) -> bool {
    (1..=64).contains(&slug.len())
        && slug
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && b == b'-'))
}

/// Server-side version rule (PublishSkillRequest.version: ^[0-9]+\.[0-9]+\.[0-9]+$).
fn is_valid_version(version: &str) -> bool {
    let numeric =
        |part: &&str| -> bool { !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) };
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3 && parts.iter().all(numeric)
}

fn title_case(slug: &str) -> String {
    slug.split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(serde::Serialize)]
struct InitResult {
    slug: String,
    dir: PathBuf,
    files: Vec<String>,
}

fn init(args: InitArgs, ctx: &CommandContext) -> Result<()> {
    let dir = args.dir.unwrap_or_else(|| PathBuf::from(&args.slug));
    let result = scaffold(&args.slug, &dir)?;
    match ctx.format() {
        OutputFormat::Json => print_json(&result),
        OutputFormat::Text => {
            println!("initialized {} at {}", result.slug, result.dir.display());
            println!(
                "  skill.json  marketplace manifest — fill in description/visibility/min_tier"
            );
            println!("  SKILL.md    agent-facing instructions (frontmatter + body)");
            println!("  payload/    optional code; packaged at the skill root next to SKILL.md");
            println!();
            println!("next: edit skill.json + SKILL.md, add code under payload/, then");
            println!("  nolgia skill pack {}", result.dir.display());
            Ok(())
        }
    }
}

fn scaffold(slug: &str, dir: &Path) -> Result<InitResult> {
    if !is_valid_slug(slug) {
        bail!(
            "invalid slug {slug:?} — must match ^[a-z0-9][a-z0-9-]{{0,63}}$ \
             (lowercase letters, digits, hyphens)"
        );
    }
    if dir.exists() {
        bail!("{} already exists — refusing to overwrite", dir.display());
    }
    fs::create_dir_all(dir.join("payload"))
        .with_context(|| format!("creating {}", dir.display()))?;
    let name = title_case(slug);
    fs::write(dir.join("skill.json"), manifest_template(slug, &name))?;
    fs::write(dir.join("SKILL.md"), skill_md_template(slug, &name))?;
    fs::write(dir.join("payload").join(".gitkeep"), "")?;
    Ok(InitResult {
        slug: slug.to_string(),
        dir: dir.to_path_buf(),
        files: vec![
            "skill.json".into(),
            "SKILL.md".into(),
            "payload/.gitkeep".into(),
        ],
    })
}

fn manifest_template(slug: &str, name: &str) -> String {
    format!(
        r#"{{
  "slug": "{slug}",
  "name": "{name}",
  "version": "0.1.0",
  "description": "TODO: one paragraph shown in the marketplace catalog — what the skill does and when to install it.",
  "required_env": ["NOLGIA_TOKEN", "NOLGIA_API_URL"],
  "credit_cost_hint": "TODO: how this skill spends credits (or note that it is free to use).",
  "min_tier": "",
  "visibility": "private",
  "python_requirements": []
}}
"#
    )
}

fn skill_md_template(slug: &str, name: &str) -> String {
    format!(
        r#"---
name: {slug}
description: |
  TODO: one or two sentences the agent uses to decide when to reach for this
  skill — what it does and the trigger situations.
version: 0.1.0
author: TODO
license: proprietary
metadata:
  tags: [nolgia]
---

# {name}

TODO: short overview. After install + sync this file lands at
$HERMES_HOME/skills/{slug}/SKILL.md and the agent reads it at session start.

## When to use

- TODO: situations where the agent should apply this skill.
- TODO: situations where it should not.

## How

TODO: step-by-step instructions. Files under payload/ are packaged at the
skill's directory root (next to this file), so reference them relative to
the skill directory, e.g.:

```bash
cd "${{HERMES_HOME:-/opt/data}}/skills/{slug}"
python3 tool.py --help
```

## Cost notes

TODO: which steps spend credits and rough magnitude. Anything over ~2k
credits needs human confirmation before running.
"#
    )
}

#[derive(Debug, serde::Serialize)]
struct PackResult {
    slug: String,
    version: String,
    out: PathBuf,
    files: Vec<String>,
    total_bytes: u64,
}

fn pack(args: PackArgs, ctx: &CommandContext) -> Result<()> {
    let result = assemble(&args.dir, args.out)?;
    match ctx.format() {
        OutputFormat::Json => print_json(&result),
        OutputFormat::Text => {
            println!(
                "packed {} v{} -> {} ({} files, {} bytes)",
                result.slug,
                result.version,
                result.out.display(),
                result.files.len(),
                result.total_bytes
            );
            for file in &result.files {
                println!("  {file}");
            }
            println!(
                "publish with: nolgia skill publish {}",
                result.out.display()
            );
            Ok(())
        }
    }
}

/// Validate an authoring directory and assemble the package `skill publish`
/// consumes: skill.json + SKILL.md + payload/ contents at the package root.
fn assemble(dir: &Path, out: Option<PathBuf>) -> Result<PackResult> {
    let manifest_path = dir.join("skill.json");
    let manifest_raw = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "reading {} — scaffold an authoring directory with `nolgia skill init`",
            manifest_path.display()
        )
    })?;
    let manifest: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&manifest_raw).context("parsing skill.json")?;
    validate_manifest(&manifest)?;
    let slug = manifest["slug"].as_str().unwrap_or_default().to_string();
    let version = manifest["version"].as_str().unwrap_or_default().to_string();

    if !dir.join("SKILL.md").is_file() {
        bail!(
            "{} has no SKILL.md — every skill needs agent-facing instructions",
            dir.display()
        );
    }

    let out = out.unwrap_or_else(|| PathBuf::from("dist").join(&slug));
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("clearing {}", out.display()))?;
    }
    fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    fs::copy(dir.join("skill.json"), out.join("skill.json"))?;
    fs::copy(dir.join("SKILL.md"), out.join("SKILL.md"))?;
    let payload = dir.join("payload");
    if payload.is_dir() {
        copy_payload(&payload, &out, true).inspect_err(|_| {
            let _ = fs::remove_dir_all(&out);
        })?;
    }

    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    collect_files(&out, Path::new(""), &mut files, &mut total_bytes)?;
    files.sort();
    if total_bytes > MAX_PACKAGE_BYTES {
        let _ = fs::remove_dir_all(&out);
        bail!(
            "package is {total_bytes} bytes decoded — the marketplace limit is 5 MiB \
             ({MAX_PACKAGE_BYTES} bytes); trim payload/"
        );
    }
    Ok(PackResult {
        slug,
        version,
        out,
        files,
        total_bytes,
    })
}

fn validate_manifest(manifest: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let string_field = |key: &str| -> Result<&str> {
        manifest
            .get(key)
            .and_then(|v| v.as_str())
            .with_context(|| format!("skill.json is missing required string field {key:?}"))
    };
    let slug = string_field("slug")?;
    if !is_valid_slug(slug) {
        bail!(
            "skill.json slug {slug:?} is invalid — must match ^[a-z0-9][a-z0-9-]{{0,63}}$ \
             (lowercase letters, digits, hyphens)"
        );
    }
    let name = string_field("name")?;
    if name.is_empty() || name.len() > 128 {
        bail!("skill.json name must be 1..=128 characters");
    }
    let version = string_field("version")?;
    if !is_valid_version(version) {
        bail!("skill.json version {version:?} is invalid — must be semver digits like \"1.0.0\"");
    }
    if string_field("description")?.is_empty() {
        bail!("skill.json description must not be empty — it is the marketplace listing text");
    }
    if let Some(value) = manifest.get("visibility")
        && !matches!(value.as_str(), Some("public") | Some("private"))
    {
        bail!("skill.json visibility must be \"public\" or \"private\", got {value}");
    }
    const TIERS: &[&str] = &[
        "",
        "starter",
        "pro",
        "studio",
        "studio_min",
        "studio_mid",
        "studio_max",
    ];
    if let Some(value) = manifest.get("min_tier")
        && !value.as_str().is_some_and(|t| TIERS.contains(&t))
    {
        bail!(
            "skill.json min_tier {value} is not a subscription tier (empty for free, or {})",
            TIERS[1..].join(", ")
        );
    }
    for key in ["required_env", "python_requirements"] {
        if let Some(value) = manifest.get(key) {
            let ok = value.as_array().is_some_and(|items| {
                items
                    .iter()
                    .all(|item| item.as_str().is_some_and(|s| !s.is_empty()))
            });
            if !ok {
                bail!("skill.json {key} must be an array of non-empty strings");
            }
        }
    }
    Ok(())
}

/// Copy payload/ contents into the package root, skipping scaffolding and
/// cache noise. Top-level names may not shadow the manifest or SKILL.md.
fn copy_payload(src: &Path, dst: &Path, top: bool) -> Result<()> {
    fs::create_dir_all(dst)?;
    let mut entries: Vec<_> = fs::read_dir(src)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "__pycache__" || name == ".gitkeep" || name == ".git" {
            continue;
        }
        if top && (name == "skill.json" || name == "SKILL.md") {
            bail!("payload/{name} would overwrite the package {name} — rename it");
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_payload(&from, &to, false)?;
        } else {
            fs::copy(&from, &to).with_context(|| format!("copying {}", from.display()))?;
        }
    }
    Ok(())
}

fn collect_files(
    dir: &Path,
    prefix: &Path,
    files: &mut Vec<String>,
    total: &mut u64,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let rel = prefix.join(entry.file_name());
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &rel, files, total)?;
        } else {
            *total += entry.metadata()?.len();
            files.push(rel.to_string_lossy().into_owned());
        }
    }
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

    let content_base64 = BASE64.encode(pack_targz(&args.dir)?);

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
fn pack_targz(dir: &Path) -> Result<Vec<u8>> {
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

        let targz = pack_targz(&src).unwrap();
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

    #[test]
    fn slug_and_version_rules_match_the_publish_api() {
        for slug in ["a", "short-film", "x9", "a-b-c-1", &"a".repeat(64)] {
            assert!(is_valid_slug(slug), "expected valid: {slug}");
        }
        for slug in ["", "-x", "Short-Film", "a_b", "a b", "é", &"a".repeat(65)] {
            assert!(!is_valid_slug(slug), "expected invalid: {slug}");
        }
        for version in ["0.1.0", "1.0.0", "10.20.30"] {
            assert!(is_valid_version(version), "expected valid: {version}");
        }
        for version in ["", "1.0", "1.0.0.0", "v1.0.0", "1.0.x", "1..0"] {
            assert!(!is_valid_version(version), "expected invalid: {version}");
        }
    }

    #[test]
    fn init_scaffolds_valid_pack_input() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("my-skill");
        let result = scaffold("my-skill", &dir).unwrap();
        assert_eq!(result.slug, "my-skill");
        assert!(dir.join("skill.json").is_file());
        assert!(dir.join("SKILL.md").is_file());
        assert!(dir.join("payload/.gitkeep").is_file());

        // The scaffold packs as-is, and its manifest fields parse into the
        // exact request types `skill publish` builds.
        let out = base.path().join("dist/my-skill");
        let packed = assemble(&dir, Some(out.clone())).unwrap();
        assert_eq!(packed.version, "0.1.0");
        let manifest: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(out.join("skill.json")).unwrap()).unwrap();
        manifest["slug"]
            .as_str()
            .unwrap()
            .parse::<PublishSkillRequestSlug>()
            .unwrap();
        manifest["name"]
            .as_str()
            .unwrap()
            .parse::<PublishSkillRequestName>()
            .unwrap();
        manifest["version"]
            .as_str()
            .unwrap()
            .parse::<PublishSkillRequestVersion>()
            .unwrap();
        let visibility: SkillVisibility =
            serde_json::from_value(manifest["visibility"].clone()).unwrap();
        assert_eq!(visibility, SkillVisibility::Private);
        assert_eq!(manifest["min_tier"], serde_json::json!(""));
        assert_eq!(manifest["python_requirements"], serde_json::json!([]));
        // Scaffolding noise stays out of the package.
        assert!(!out.join(".gitkeep").exists());

        // Refuses bad slugs and existing directories.
        assert!(scaffold("Bad_Slug", &base.path().join("x")).is_err());
        assert!(scaffold("my-skill", &dir).is_err());
    }

    fn authoring_dir(base: &Path, manifest: serde_json::Value) -> PathBuf {
        let dir = base.join("src");
        fs::create_dir_all(dir.join("payload")).unwrap();
        fs::write(dir.join("skill.json"), manifest.to_string()).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: my-skill\n---\n").unwrap();
        dir
    }

    fn manifest_json() -> serde_json::Value {
        serde_json::json!({
            "slug": "my-skill", "name": "My Skill", "version": "0.1.0",
            "description": "d", "required_env": [], "credit_cost_hint": "",
            "min_tier": "", "visibility": "private"
        })
    }

    #[test]
    fn pack_rejects_invalid_input() {
        let base = tempfile::tempdir().unwrap();
        let out_dir = base.path().join("out");
        let out = Some(out_dir.clone());

        let mut bad_slug = manifest_json();
        bad_slug["slug"] = "Bad_Slug".into();
        let dir = authoring_dir(&base.path().join("a"), bad_slug);
        let err = assemble(&dir, out.clone()).unwrap_err();
        assert!(err.to_string().contains("slug"), "{err}");

        let mut bad_version = manifest_json();
        bad_version["version"] = "1.0".into();
        let dir = authoring_dir(&base.path().join("b"), bad_version);
        let err = assemble(&dir, out.clone()).unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");

        let mut bad_tier = manifest_json();
        bad_tier["min_tier"] = "platinum".into();
        let dir = authoring_dir(&base.path().join("c"), bad_tier);
        let err = assemble(&dir, out.clone()).unwrap_err();
        assert!(err.to_string().contains("min_tier"), "{err}");

        let mut bad_reqs = manifest_json();
        bad_reqs["python_requirements"] = serde_json::json!(["ok", 7]);
        let dir = authoring_dir(&base.path().join("d"), bad_reqs);
        let err = assemble(&dir, out.clone()).unwrap_err();
        assert!(err.to_string().contains("python_requirements"), "{err}");

        let dir = authoring_dir(&base.path().join("e"), manifest_json());
        fs::remove_file(dir.join("SKILL.md")).unwrap();
        let err = assemble(&dir, out.clone()).unwrap_err();
        assert!(err.to_string().contains("SKILL.md"), "{err}");

        let dir = authoring_dir(&base.path().join("f"), manifest_json());
        fs::write(
            dir.join("payload/big.bin"),
            vec![0u8; (MAX_PACKAGE_BYTES + 1) as usize],
        )
        .unwrap();
        let err = assemble(&dir, out).unwrap_err();
        assert!(err.to_string().contains("5 MiB"), "{err}");
        assert!(!out_dir.exists(), "oversize output must be cleaned up");
    }

    #[test]
    fn pack_output_matches_what_publish_consumes() {
        let base = tempfile::tempdir().unwrap();
        let dir = authoring_dir(base.path(), manifest_json());
        fs::write(dir.join("payload/tool.py"), "print('hi')\n").unwrap();
        fs::create_dir_all(dir.join("payload/pkg")).unwrap();
        fs::write(dir.join("payload/pkg/__init__.py"), "").unwrap();
        fs::create_dir_all(dir.join("payload/__pycache__")).unwrap();
        fs::write(dir.join("payload/__pycache__/junk.pyc"), "x").unwrap();

        let out = base.path().join("dist/my-skill");
        let packed = assemble(&dir, Some(out.clone())).unwrap();
        assert_eq!(packed.slug, "my-skill");
        assert_eq!(
            packed.files,
            vec!["SKILL.md", "pkg/__init__.py", "skill.json", "tool.py"]
        );

        // Payload contents sit at the package root, exactly where the
        // publish tarball + pod sync will put them next to SKILL.md.
        let targz = pack_targz(&out).unwrap();
        let root = base.path().join("skills");
        fs::create_dir_all(&root).unwrap();
        materialize(&root, "my-skill", "0.1.0", &targz).unwrap();
        let installed = root.join("my-skill");
        assert!(installed.join("skill.json").is_file());
        assert!(installed.join("SKILL.md").is_file());
        assert!(installed.join("tool.py").is_file());
        assert!(installed.join("pkg/__init__.py").is_file());
        assert!(!installed.join("payload").exists());
        assert!(!installed.join("__pycache__").exists());

        // A payload file that would shadow the manifest is refused.
        fs::write(dir.join("payload/skill.json"), "{}").unwrap();
        let err = assemble(&dir, Some(out)).unwrap_err();
        assert!(err.to_string().contains("payload/skill.json"), "{err}");
    }
}
