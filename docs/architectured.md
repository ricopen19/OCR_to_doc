# アーキテクチャ

コードから読み取れない情報のみ記載。コンポーネントの責務や CLI オプションはコードを参照。

GLM-OCR 移行（`docs/tasks.md` 参照）により OCR 本体は Rust + Ollama に置き換わった。
表の再 OCR も含め本文・表とも単一モデルで処理する構成に統一済み（Unlimited OCR は撤去、
`docs/decisions.md` 参照）。OCR バックエンドは設定で切り替え可能で、既定は Ollama、
上級者向けに同一 PC 上の llama.cpp サーバー（OpenAI 互換 `/v1`）も選べる。エンジン差は
`ui/src-tauri/src/ollama/engine.rs`（`OcrBackend`）に閉じ込め、パイプラインはこれ経由で
呼ぶ（`docs/decisions.md` ADR-018 参照）。旧 YomiToku パイプライン（`dispatcher.py`/`ocr.py`/`ingest.py`
等）と、GUI 未到達だった `--cli` 実行パスは削除済み（`docs/decisions.md` ADR-017 参照）。
Python は「エクスポート（docx/xlsx/csv）」「図表検出（YOLOv8x-DocLayNet）」「HEIC/SVG
画像正規化（プレビュー用）」「`--self-test`（CI 自己診断用、`export_docx.py` のみ使用）」に
役割が縮小している。

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
│  - OCR backend へ画像を送り OCR │
│  - Markdown 整形・表プレースホルダ│
└──────────┬───────────┬──────────┘
           │            │ subprocess（結果整形後にのみ呼ぶ）
┌──────────▼──────┐ ┌───▼─────────────────────┐
│  OCR backend     │ │  Python (エクスポート限定)│
│  Ollama (既定)   │ │  export_docx.py          │
│  or llama.cpp    │ │  export_excel_poc.py     │
│  (単一モデル)     │ │  detect_figures.py       │
└──────────────────┘ │  (YOLOv8x-DocLayNet)     │
                      └───────────────────────────┘
```

`--self-test`（`ui/src-tauri/src/cli.rs`）は上記とは別経路で、`export_docx.py` の
最小動作確認のみ行う。GUI からは到達しない CI 自己診断専用のフォールバック。

## 2. データフロー概要

```
入力(PDF/画像)
  → [PDF] Poppler で1ページずつ画像化 → OCR backend (既定 Ollama/glm-ocr) でページ OCR
       → 表領域は必要に応じて glm-ocr で再OCR（enableTableReocr ON 時）
  → page_###.md → マージ → *_merged.md
  → エクスポート（Python subprocess）→ docx / xlsx / csv
```

## 3. 想定環境

| 項目 | 配布先 | 開発 |
|---|---|---|
| OS | macOS (Apple Silicon) | macOS (Apple Silicon) |
| OCR backend | 既定 Ollama（ユーザーが別途インストール・glm-ocr を pull）。設定で llama.cpp サーバー（同一 PC）も選択可 | 同左 |
| Python | システム Python3（`.app` に同梱されない。エクスポート・図表検出用のみ） | uv 管理の `.venv` |
| Poppler | Homebrew または PATH から検出 | Homebrew |

配布は現在 macOS DMG（`.github/workflows/macos-dmg.yml`）が主経路。Windows portable 版は
GLM-OCR 移行前（YomiToku 構成）を前提としたビルドが CI 上に残るのみで、配布対象外
（`docs/decisions.md` ADR-011/ADR-012 参照）。
