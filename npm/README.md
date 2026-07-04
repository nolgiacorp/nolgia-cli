# @nolgia/cli

Command line interface for the [Nolgia](https://nolgia.ai) generative media platform: images, video (including native multi-shot sequences), and audio from your terminal or from AI agents

This package downloads the prebuilt `nolgia` binary for your platform during postinstall (macOS universal, Linux x86_64, Windows x86_64) and exposes it as `nolgia` on your PATH. The binary is the same one attached to each [GitHub release](https://github.com/nolgiacorp/nolgia-cli/releases)

## Install

```bash
npm install -g @nolgia/cli
```

Other install paths (Homebrew, cargo, curl installer, raw binaries) are covered in the [repository README](https://github.com/nolgiacorp/nolgia-cli#installation)

## Quick start

```bash
nolgia auth login                 # device-code sign-in via your browser
nolgia models list                # live model catalog with capabilities and credit pricing

nolgia gen image --model flux-pro --prompt "A futuristic city at sunset" --out city.png

nolgia gen video --model veo-3.1 --prompt "A slow dolly through a neon atelier" \
  --duration-seconds 8 --out clip.mp4

nolgia gen video --model fal-ai/bytedance/seedance/v2/pro/text-to-video \
  --prompt "Gritty 35mm film look" \
  --shot "8:WIDE SHOT. Rural highway, a single car heading south|engine, wind" \
  --shot "4:MCU. The driver glances at the dead radio|AM static cuts out" \
  --generate-audio true --out sequence.mp4

nolgia gen audio --model fal-ai/elevenlabs/tts/eleven-v3 --prompt "Welcome to Nolgia" --out hello.mp3
```

## Highlights

`nolgia gen video` supports text-to-video and image-to-video; pass `--input` with a local file or an asset UUID. Models with native image input like Veo and Omni Flash take it directly, while Kling and Seedance switch to their image-to-video variants. Multi-shot sequences are first class through repeated `--shot SECONDS:PROMPT|AUDIO` flags; the platform composes the cut natively

Costs are transparent: `nolgia gen video --cost-only` prints the credit estimate from the live catalog before you spend anything, and `nolgia models list` shows per-model pricing and capabilities

Every command takes `--json` for machine-readable output, which is how agent pipelines drive the CLI. Authentication accepts a browser device-code flow (`nolgia auth login`) or a personal access token via `--token` / `NOLGIA_TOKEN`

## Command overview

| Command | Purpose |
|---|---|
| `nolgia auth login` / `status` / `logout` | Device-code sign-in, token storage in the system keyring |
| `nolgia gen image` / `video` / `audio` | Submit generations, wait, and download results |
| `nolgia status` / `wait` | Inspect or block on a job |
| `nolgia assets list` / `get` / `upload` / `delete` / `tag` | Manage generated and uploaded assets, including tags |
| `nolgia characters list` / `get` / `create` / `update` / `delete` | Reusable characters with reference images |
| `nolgia projects list` / `get` / `create` / `update` / `delete` / `add-assets` / `remove-asset` | Group assets into projects |
| `nolgia models list` / `get` | Live model catalog, capabilities, credit pricing |
| `nolgia account` / `billing` | Usage, credits, and billing portal links |
| `nolgia pat create` / `list` / `revoke` | Personal access tokens for API use |
| `nolgia skills list` / `show` / `install` | Bundled skills for Claude Code and other agents |
| `nolgia completion <shell>` | Shell completions |

## Environment

| Variable | Effect |
|---|---|
| `NOLGIA_TOKEN` | PAT (`nol_...`) or JWT used instead of the keyring |
| `NOLGIA_API_URL` | Override the API base URL |
| `NOLGIA_NO_UPDATE_CHECK` | Disable the once-daily update hint |
| `NOLGIA_SURFACE` | Self-identify agent traffic |

Full documentation, the OpenAPI-generated client, and the development guide live in the [repository](https://github.com/nolgiacorp/nolgia-cli). The platform itself is documented at [nolgia.ai](https://nolgia.ai)

## License

MIT
