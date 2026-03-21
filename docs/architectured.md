# アーキテクチャ

コードから読み取れない情報のみ記載。コンポーネントの責務や CLI オプションはコードを参照。

## 1. システム構成

```
┌─────────────────────────────────┐
│  GUI (Tauri + React + Mantine)  │
│  ui/src-tauri/src/lib.rs        │
│  ui/src/                        │
└──────────┬──────────────────────┘
           │ invoke (Tauri commands)
┌──────────▼──────────────────────┐
│  Python パイプライン             │
│  dispatcher.py (エントリポイント) │
│  ocr_chanked.py / ocr.py        │
│  postprocess.py                 │
│  export_docx.py / export_excel  │
└──────────┬──────────────────────┘
           │
┌──────────▼──────────────────────┐
│  外部依存                        │
│  YomiToku (OCR エンジン)         │
│  Poppler (PDF → 画像)           │
│  python-docx / openpyxl         │
└─────────────────────────────────┘
```

## 2. データフロー概要

```
入力(PDF/画像)
  → dispatcher.py
    ├─ PDF → ocr_chanked.py → ページ画像化 → OCR → page_###.md + figures/
    │    → postprocess.py → *_merged.md
    └─ 画像 → 正規化 → 前処理 → OCR → page_001.md + figures/
  → エクスポート → docx / xlsx / csv
```

## 3. 想定環境

| 項目 | 配布先（職場） | 開発 |
|---|---|---|
| OS | Windows 11 | macOS (Apple Silicon) |
| CPU | i5-8500 クラス | Apple Silicon |
| メモリ | 16GB | - |
| GPU | なし | - |
| Python | 同梱（ユーザーインストール不要） | 3.12 (Poetry) |
| Poppler | プロジェクト同梱 | Homebrew |
