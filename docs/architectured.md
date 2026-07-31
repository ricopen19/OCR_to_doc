# アーキテクチャ

コードから読み取れない情報のみ記載。コンポーネントの責務や CLI オプションはコードを参照。

GLM-OCR 移行（`docs/tasks.md` 参照）により OCR 本体は Rust + Ollama に置き換わった。
Python は「エクスポート（docx/xlsx/csv）」「図表検出（YOLOv8x-DocLayNet）」「`--cli`/`--self-test`
経由の旧パイプライン（`dispatcher.py`、CI 自己診断用）」に役割が縮小している。

## 1. システム構成

```
┌─────────────────────────────────┐
│  GUI (Tauri + React + Mantine)  │
│  ui/src/                        │
└──────────┬──────────────────────┘
           │ invoke('run_job_ollama', ...)
┌──────────▼──────────────────────┐
│  Rust OCR パイプライン           │
│  ui/src-tauri/src/ocr/pipeline.rs│
│  - PDF→画像化 (Poppler pdftoppm) │
│  - Ollama へ画像を送り OCR       │
│  - Markdown 整形・表プレースホルダ│
└──────────┬───────────┬──────────┘
           │            │ subprocess（結果整形後にのみ呼ぶ）
┌──────────▼──────┐ ┌───▼─────────────────────┐
│  Ollama          │ │  Python (エクスポート限定)│
│  Unlimited OCR   │ │  export_docx.py          │
│  (glm-ocr は表の  │ │  export_excel_poc.py     │
│   み再OCR に使用) │ │  detect_figures.py       │
└──────────────────┘ │  (YOLOv8x-DocLayNet)     │
                      └───────────────────────────┘
```

`--cli <input>` / `--self-test`（`ui/src-tauri/src/cli.rs`）は上記とは別経路で、
旧 Python パイプライン（`dispatcher.py`）を直接叩く。GUI からは到達しない
CI 自己診断専用のフォールバック。

## 2. データフロー概要

```
入力(PDF/画像)
  → [PDF] Poppler で1ページずつ画像化 → Ollama (Unlimited OCR) で OCR
       → 表領域はページ処理後に glm-ocr でまとめて再OCR（enableTableReocr ON 時）
  → page_###.md → マージ → *_merged.md
  → エクスポート（Python subprocess）→ docx / xlsx / csv
```

## 3. 想定環境

| 項目 | 配布先 | 開発 |
|---|---|---|
| OS | macOS (Apple Silicon) | macOS (Apple Silicon) |
| Ollama | ユーザーが別途インストール（Unlimited OCR / glm-ocr を pull） | 同左 |
| Python | システム Python3（`.app` に同梱されない。エクスポート・図表検出用のみ） | uv 管理の `.venv` |
| Poppler | Homebrew または PATH から検出 | Homebrew |

Windows portable 版は GLM-OCR 移行前の構成を前提としており、現状は配布対象外
（`docs/decisions.md` ADR-011/ADR-012 参照）。
