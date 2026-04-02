---
name: clipcannon
description: >
  AI-powered video understanding, editing, and voice synthesis via 51 MCP tools. 22-stage
  analysis pipeline (transcription, scene detection, emotion, speaker diarization, OCR,
  narrative structure). Declarative editing with adaptive captions, face-tracking crop,
  split-screen. Voice cloning (Qwen3-TTS), lip-sync avatars (LatentSync), AI music
  (ACE-Step). 7 platform profiles (TikTok, Reels, Shorts, YouTube, LinkedIn). 100% local
  GPU. Use when the user says "edit this video", "find the best moments", "create a
  highlight reel", "add captions", "clone voice", "lip sync", "render for TikTok".
version: 0.1.0
author: ChrisRoyse
mcp_server: true
protocol: stdio
entry_point: clipcannon serve
tags:
  - video
  - editing
  - voice-cloning
  - lip-sync
  - transcription
  - mcp
  - gpu
env_vars:
  - CLIPCANNON_DATA_DIR
  - CLIPCANNON_GPU_DEVICE
---

# ClipCannon — AI Video Editor via MCP

Turns Claude into a professional video editor. Ingest video, run 22-stage AI analysis, then use 51 MCP tools to find moments, create edits, render platform-ready clips, generate music, clone voices, and produce lip-synced talking-head videos. Everything runs locally on GPU.

## When to Use This Skill

- **Video editing**: "edit this video", "cut the boring parts", "create a highlight reel"
- **Content discovery**: "find the most emotional moments", "find where they talk about X"
- **Platform rendering**: "render for TikTok", "create Instagram Reels version"
- **Voice**: "clone this speaker's voice", "generate narration", "lip sync"
- **Audio**: "add background music", "generate sound effects", "compose a score"
- **Analysis**: "transcribe this video", "who are the speakers?", "scene breakdown"

## When Not to Use

- For simple video format conversion — use `ffmpeg-processing`
- For AI image generation — use `comfyui` or `art`
- For agentic video production from scratch — use `open-montage`
- For meeting transcription — use `echoloop`
- For audio-only processing — use `ffmpeg-processing`

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌────────────────┐
│  Video Input     │────>│  22-Stage        │────>│  51 MCP Tools  │
│                  │     │  Analysis DAG    │     │                │
│  Any format      │     │                  │     │ Understanding  │
│  (via FFmpeg)    │     │ Transcription    │     │ Discovery      │
│                  │     │ Scene detection  │     │ Editing        │
│                  │     │ Emotion analysis │     │ Rendering      │
│                  │     │ Speaker diarize  │     │ Audio/Music    │
│                  │     │ Beat tracking    │     │ Voice/Avatar   │
│                  │     │ OCR / QA scoring │     │ Config/Billing │
│                  │     │ 5 embedding      │     │                │
│                  │     │ spaces (sqlite)  │     │                │
└─────────────────┘     └──────────────────┘     └────────────────┘
                              GPU Local
```

## MCP Tools (51)

### Understanding (4)
| Tool | Description |
|------|-------------|
| `clipcannon_ingest` | Ingest video, run 22-stage analysis pipeline |
| `clipcannon_get_transcript` | Get full transcript with timestamps |
| `clipcannon_get_frame` | Extract specific frame as image |
| `clipcannon_search_content` | Semantic search across all 5 embedding spaces |

### Discovery (2+)
| Tool | Description |
|------|-------------|
| `clipcannon_find_best_moments` | AI-ranked highlight moments |
| `clipcannon_find_cut_points` | Optimal cut points for editing |

### Editing (8+)
Declarative EDL with adaptive captions, face-tracking crop, split-screen, PIP, canvas compositing, motion effects, overlays, iterative version control.

### Rendering (7 platforms)
One-click rendering for TikTok, Instagram Reels, YouTube Shorts, YouTube Standard, YouTube 4K, Facebook, LinkedIn. NVENC GPU acceleration.

### Audio & Music (6)
| Tool | Description |
|------|-------------|
| `clipcannon_generate_music` | ACE-Step diffusion music generation |
| `clipcannon_compose_midi` | 6 MIDI presets with FluidSynth |
| `clipcannon_generate_sfx` | 9 DSP sound effects |
| `clipcannon_auto_music` | Smart music for edit |
| `clipcannon_compose_music` | Full composition |
| `clipcannon_audio_cleanup` | Noise reduction, normalisation |

### Voice & Avatar (4)
| Tool | Description |
|------|-------------|
| `clipcannon_voice_clone` | Qwen3-TTS 1.7B with multi-gate verification |
| `clipcannon_lip_sync` | LatentSync 1.6 (ByteDance) diffusion lip-sync |
| `clipcannon_extract_webcam` | Extract face/webcam footage |
| `clipcannon_voice_enhance` | Resemble Enhance to 44.1kHz broadcast quality |

### Config & Billing (7)
Configuration, credits balance, spending limits, provenance chain.

## Setup

```bash
# Install (requires Python 3.12+, CUDA GPU)
pip install clipcannon

# Or from source
cd /tmp && git clone https://github.com/ChrisRoyse/clipcannon.git
cd clipcannon && pip install -e .

# Start MCP server
clipcannon serve
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CLIPCANNON_DATA_DIR` | `~/.clipcannon` | Data/model storage directory |
| `CLIPCANNON_GPU_DEVICE` | `cuda:0` | GPU device for inference |
| `CLIPCANNON_NVENC` | `true` | Use NVENC GPU encoding for renders |

## 5 Embedding Spaces

| Space | Model | Dimensions | Use |
|-------|-------|------------|-----|
| Visual | SigLIP | 1152 | Scene similarity, visual search |
| Semantic | Nomic | 768 | Transcript/meaning search |
| Emotion | Wav2Vec2 | 1024 | Emotional moment detection |
| Speaker | WavLM | 512 | Speaker diarisation |
| Voice ID | ECAPA-TDNN | 2048 | Voice cloning verification |

All stored in sqlite-vec for local KNN search.

## Integration with Other Skills

| Skill | Relationship |
|-------|-------------|
| `open-montage` | OpenMontage produces from scratch; ClipCannon edits existing footage |
| `ffmpeg-processing` | ClipCannon uses FFmpeg internally; the skill is for standalone conversions |
| `echoloop` | EchoLoop captures meeting audio; ClipCannon edits the resulting video |
| `notebooklm` | Feed video transcripts as NotebookLM sources for study materials |
| `art` | Generate thumbnails/overlays via Nano Banana 2 |
| `comfyui` | Generate AI video segments to splice into edits |

## Provenance

SHA-256 hash chain links every pipeline operation. Every output is traceable to its source.

## Attribution

ClipCannon by Chris Royse. BSL 1.1 License.
