# 仕様（確定事項）

このドキュメントは、このリポジトリで **すでに実装されている機能** と **確定した挙動/入出力仕様** を 1 箇所に集約したものです。  
PoC や将来案（未確定）は `docs/Tasks.md` / `docs/poc_results/` / `docs/requirements/` を参照してください。

## 1. 対象範囲

- 対象: ローカル実行の OCR → Markdown（→ docx / xlsx）パイプライン
- 推奨エントリポイント: `dispatcher.py`
- 中核スクリプト:
  - PDF チャンク OCR: `ocr_chanked.py`
  - 画像 OCR: `ocr.py`（`dispatcher.py` から呼び出し）
  - Markdown マージ: `postprocess.py`（`poppler/merged_md.py` は互換ラッパ）
  - Word 変換: `export_docx.py`
  - Excel 変換（PoC）: `export_excel_poc.py`（`dispatcher.py --formats xlsx` から利用）

## 2. 入力仕様

`dispatcher.py` は入力を判定し、PDF なら `ocr_chanked.py` へ委譲、画像なら前処理して `ocr.py` で OCR します。

- PDF:
  - 画像 PDF を対象に `pdf2image + poppler` でページ画像化して OCR
  - テキスト埋め込み PDF は `ingest.py` で判定するが、抽出パイプラインは PoC/整備途中（確定仕様外）
- 画像:
  - `png/jpg/jpeg/webp/tif/tiff/bmp`
  - `heic/heif`（PNG へ正規化して処理）
  - `svg`（PNG へラスタ化して処理）

複数画像（スクショ等）は、精度/再現性の観点から PDF にまとめて投入する運用を推奨します（仕様として強制はしません）。

## 3. 出力仕様（ディレクトリ/成果物）

出力ルートは原則 `result/` です。

### 3.1 PDF 入力（`ocr_chanked.py` 経路）

- 出力先: `result/<pdf_stem>/`
  - ページ範囲が全ページ以外の場合は自動で `result/<pdf_stem>_p<start>-<end>/`
  - `--label` 指定時は `result/<pdf_stem>_<label>/`
- 生成物（代表例）:
  - `page_images/page_001.png`（デフォルト保持。`--drop-page-images` で削除）
  - `page_001.md`（ページ Markdown。マージ後にデフォルトで削除）
  - `figures/`（図表抽出画像、候補ログなど）
  - `<base>_merged.md`（結合済み Markdown。`base` は出力ディレクトリ名）
  - `math_review.csv`（数式崩れ疑いの簡易ログ）
  - `yomi_formats/json/*.json`（`--emit-json on/auto` の場合）
  - `yomi_formats/csv/*.csv`（`--emit-csv` の場合）

マージは `ocr_chanked.py` の最後で自動実行されます（`postprocess.py` を呼び出し）。マージ後、ページ単位の `.md` はデフォルトで削除されます。

### 3.2 画像入力（`dispatcher.py` の image 経路）

- 出力先: `result/<image_stem>/`
- 生成物（代表例）:
  - `converted/`（正規化後の PNG や、`--image-as-pdf` の PDF を保存）
  - `preprocessed/<profile>/page_001.png`（OCR 向け前処理済み画像）
  - `page_001.md`（ページ Markdown）
  - `figures/`（図表抽出画像、候補ログなど）
  - `yomi_formats/json/*.json`（`--formats xlsx` などで JSON を出す場合）

※ 画像単体では自動マージは走りません（`page_001.md` が成果物になります）。

## 4. コマンド一覧

使用可能な CLI とオプションの一覧は `docs/cli_commands.md` に集約します（UI 設計の参照元）。

## 5. 既定の方針（確定）

- ローカル完結（クラウド送信なし）
- PDF 経路は CPU 前提（`ocr_chanked.py` は `device=cpu` 固定）
- ページ画像はデフォルト保持（検証用）。不要なら `--drop-page-images`
- マージ後はページ Markdown をデフォルト削除（中間生成物を減らす）

## 6. 関連ドキュメント

- タスク/未完の一覧: `docs/Tasks.md`
- アーキテクチャ（責務分割）: `docs/architectured.md`
- 実行環境/制約（背景含む）: `docs/env_and_limits.md`
- 変更履歴: `docs/change_log.md`
