# スマホスキャン OCR → Markdown / Word 変換ツール

## 概要

スマホで撮った画像や iOS の「書類をスキャン」で作った PDF を入力に、
- 日本語 OCR と図表の切り出し
- Markdown 正規化と Word/将来的な Excel 変換
を段階的に実現するためのツール群です。

## ゴール

**入力**
- スマホ写真（jpg, png）
- 画像 PDF / テキスト埋め込み PDF

**処理**
- 入力種別を判定し、YomiToku など適切なエンジンでテキスト化
- 図表画像を抽出し、Markdown 内にリンクとして埋め込む

**出力**
- Markdown（中間フォーマット）
- Word（.docx）
- 将来的には Excel（.xlsx）

## パイプライン概要

1. ファイル投入（画像 / PDF）
2. PDF は `pdf2image + poppler` でページ単位に画像化
3. YomiToku（lite/full）を中心に OCR を実行
4. ページごとの Markdown を結合して `merged.md` を生成
5. Markdown を元に docx（将来は xlsx）へ変換

## 主な機能

- PDF / 画像のページ分解と前処理
- YomiToku CLI をラップした OCR 実行（`lite`/`full` 切り替え）
- 入力ファイルごとに `result/<ファイル名>/` を作り、ページ Markdown と `figures/` 配下の図版を保存
- ページ Markdown の結合と図表画像の整理
- Pandoc などを使った docx 変換

## 想定入力

- 数的処理系の問題集やプリントをスマホで撮影した画像
- iOS「書類をスキャン」で生成した 1〜数十ページの PDF

## 実装フェーズ概要

### フェーズ 1：基本 OCR パイプライン
- 対象：画像 PDF / 画像ファイル
- モジュール：`ingest.py`（PDF→画像）、`ocr.py`（YomiToku ラッパ）、`postprocess.py`（Markdown 結合）
- 成果物：図表付き Markdown を一括生成

### フェーズ 2：エンジン切り替えとテキスト PDF 対応
- `dispatcher.py` で入力種別を判定し、画像なら OCR、テキスト PDF なら `text_pdf.py` で抽出
- 将来的な `markitdown` など別エンジンとの連携を想定

### フェーズ 3：エクスポートと fallback
- `export.py` で Markdown→docx/xlsx へ変換
- `ocr.py` に fallback 戦略を用意し、YomiToku が失敗したページを Tesseract/PaddleOCR などで再処理

## ざっくり使い方

```bash
# 仮想環境に入る
poetry shell

# 依存インストール（初回のみ）
poetry install

# OCR 実行（例：全ページ、lite モード）
poetry run python ocr_chanked.py input.pdf

# OCR 実行（例：11〜20ページのみ、モード full）
poetry run python ocr_chanked.py input.pdf --start 11 --end 20 --mode full

# Markdown を結合（`result/input/input_merged.md` が生成される）
poetry run python poppler/merged_md.py --input result/input --base-name input

# Word に変換
poetry run python export_docx.py output_merged.md
```

## 参考ドキュメント
- 詳細仕様: `docs/spec.md`
- タスクとロードマップ: `docs/Tasks.md`
- アーキテクチャ詳細: `docs/architectured.md`
- 環境と制約: `docs/env_and_limits.md`（macOS 開発 + Windows 配布ポリシーを記載）
