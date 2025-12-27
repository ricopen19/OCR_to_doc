# CLI コマンド一覧

UI の設定画面を検討するための「現状使えるコマンド/オプション」を 1 箇所に集約します。  
原則は `dispatcher.py` を入口として使い、必要に応じて補助コマンドを使います。

## 1. 推奨エントリポイント: `dispatcher.py`

### 基本
```bash
poetry run python dispatcher.py <input>
```

`<input>` は PDF / 画像（png/jpg/webp/tif/bmp）/ HEIC/HEIF / SVG を受け付けます。

### よく使う例
```bash
# 画像/HEIC/SVG/PDF を自動判定して OCR（md を出力）
poetry run python dispatcher.py sample.pdf
poetry run python dispatcher.py sample.heic

# docx まで作る
poetry run python dispatcher.py sample.pdf --formats md docx

# xlsx（JSON 経由の PoC）
poetry run python dispatcher.py sample.pdf --formats md xlsx

# xlsx（テーブルモード: 結合解除＋構造変化ごとに別シート）
poetry run python dispatcher.py sample.pdf --formats md xlsx --excel-mode table

# csv（JSON tables から結合解除して、構造変化ごとに分割）
poetry run python dispatcher.py sample.pdf --formats md csv

# NOTE: csv 出力は `result/<name>/` 直下に `<name>__<table_name>.csv` を複数作成します。
# - 通常（レイアウト）: `<name>__table_01.csv` など
# - テーブル（結合解除）: `<name>__<table_name>.csv`（例: `...__神奈川.csv`）

# NOTE: xlsx 出力では、JSON tables の空セルに対して page_images から記号（主に ○/□）を補完する場合があります（PoC）。

# PDF のページ範囲などを ocr_chanked.py に渡す（-- 以降が透過されます）
# NOTE: Poetry 経由で `--` を使う場合は、Poetry 側の区切り `--` も必要です。
poetry run -- python dispatcher.py sample.pdf -- --start 11 --end 20
```

### オプション
- `--mode {lite,full}`: YomiToku のモード（既定 `lite`）
- `--device <str>`: YomiToku のデバイス（既定 `cpu`）
- `--output-root <dir>`: 出力ルート（既定 `result`）
- `--svg-dpi <int>`: SVG→PNG の DPI（既定 `300`）
- `--ocr-profile <name>`: 画像向け前処理プロファイル（既定 `ocr_default`）
- `--figure / --no-figure`: 図表抽出の ON/OFF（既定 ON）
- `--image-as-pdf / --no-image-as-pdf`: 画像を PDF 化して PDF 経路で処理（既定 OFF）
- `--image-dpi <int>`: 画像→PDF の DPI（既定 `300`）
- `--crop <left,top,width,height>`: 正規化トリミング範囲（0〜1）。PDF/画像どちらにも適用（例: `--crop 0.05,0.08,0.90,0.85`）
- `--fallback-tesseract / --no-fallback-tesseract`: pytesseract フォールバック（既定 OFF）
- `--force-tesseract-merge / --no-force-tesseract-merge`: tesseract 結果を追記（既定 OFF）
- `--math-refiner / --no-math-refiner`: PDF 経路で Pix2Text を有効化（既定 OFF）
- `--formats <list>`: 生成物（既定 `md`、例: `--formats md docx xlsx csv`）
- `--docx-math {text,image}`: docx 出力時の数式の扱い（既定 `text`。image は数式領域を画像で貼る）
- `--excel-mode {layout,table}`: xlsx 出力モード（既定 `layout`、`table` は結合解除＋テーブル化）
- 追加引数透過: `--` 以降は `ocr_chanked.py` の引数として解釈されます（例: `--start/--end` 等）。
  - UI で PDF ごとにページ範囲を変えたい場合は、PDF ファイル単位で `dispatcher.py` を実行し、各 PDF に対応する `-- --start/--end` を付与します（未指定=全ページ）。

## 2. PDF を直接処理: `ocr_chanked.py`

PDF をページ画像化し、チャンク処理しながら OCR します。最後に `postprocess.py` により自動マージします。

### 基本
```bash
poetry run python ocr_chanked.py <input.pdf>
```

### よく使う例
```bash
# ページ範囲
poetry run python ocr_chanked.py input.pdf --start 11 --end 20

# チャンク休憩
poetry run python ocr_chanked.py input.pdf --enable-rest --rest-seconds 10

# JSON 出力（常に/自動）
poetry run python ocr_chanked.py input.pdf --emit-json on
poetry run python ocr_chanked.py input.pdf --emit-json auto
```

### オプション（主要）
- `--start <int>` / `--end <int>`: 処理ページ範囲（1 起点）
- `--dpi <int>`: PDF→画像変換の DPI（既定 `300`）
- `--chunk-size <int>`: チャンクサイズ（既定 `10`）
- `--enable-rest`: 休憩を有効化（既定 無効）
- `--rest-seconds <int>`: チャンク休憩秒（既定 `10`）
- `--mode {lite,full}`: YomiToku モード（既定 `lite`）
- `--device <str>`: YomiToku に渡すデバイス指定（既定 `cpu`）
- `--label <str>`: 出力ディレクトリのラベル（`<output-root>/<PDF名>_<label>/`）
- `--output-root <dir>`: 出力ルート（既定 `result`）
- `--drop-page-images`: `page_images` を保存しない（既定は保存）
- `--emit-json {off,on,auto}`: JSON 出力（既定 `off`）
- `--emit-csv / --no-emit-csv`: CSV 出力（既定 OFF）
- `--fallback-tesseract / --no-fallback-tesseract`: pytesseract フォールバック（既定 OFF）
- `--force-tesseract-merge / --no-force-tesseract-merge`: tesseract 結果を追記（既定 OFF）
- `--crop <left,top,width,height>`: 正規化トリミング範囲（0〜1）。全ページに適用（例: `--crop 0.05,0.08,0.90,0.85`）

### オプション（上級: アイコン/数式）
- アイコンフィルタ:
  - `--icon-profile {default,strict,lenient}`
  - `--icon-policy {auto,review,keep}`
  - `--icon-config <path>`（JSON）
  - `--icon-log / --no-icon-log`（候補ログ、既定 ON）
  - `--icon-log-all / --no-icon-log-all`（全統計ログ、既定 OFF）
- 数式（Pix2Text）:
  - `--math-refiner`（既定 OFF）
  - `--math-score <float>`（既定 `0.7`）
  - `--math-cache <dir>`（既定 `./.pix2text_cache`）
  - `--math-resized-shape <int>`（既定 `960`）

## 3. マージ（単体実行）: `postprocess.py` / `poppler/merged_md.py`

`result/<name>/` 配下の `page_*.md` を結合し、`<base-name>_merged.md` を作ります。マージ後、ページ md はデフォルトで削除されます。

```bash
poetry run python postprocess.py --input result/<name> --base-name <name>
poetry run python poppler/merged_md.py --input result/<name> --base-name <name>  # 互換ラッパ
```

- `--input <dir>`: ページ md のディレクトリ（既定 `result`）
- `--output <path>`: 出力パス（省略時は `<base-name>_merged.md`）
- `--base-name <str>`: 出力ベース名（既定 `merged`）
- `--no-heading`: `# Page n` を入れない

## 4. Word 変換: `export_docx.py`

```bash
poetry run python export_docx.py <input.md>
```

- 引数省略時は `merged.md` を入力として扱います。

## 5. Excel 変換（PoC）: `export_excel_poc.py`

YomiToku の出力（json/csv/html）を xlsx に変換します。

```bash
poetry run python export_excel_poc.py <input> <output.xlsx> --format json
```

- `--format {json,csv,html}`（必須）
- `--csv-tables-only`: CSV の段落ブロック（1列）を除外
- `--sheet-prefix <str>`: シート名プレフィックス（既定 `table`）
- `--meta / --no-meta`: メタシート（既定 ON）
- `--review-columns / --no-review-columns`: レビュー列（既定 OFF）
- `--auto-format / --no-auto-format`: 自動書式（既定 ON）
- `--excel-mode {layout,table}`: xlsx 出力モード（既定 `layout`、`table` は結合解除＋テーブル化）

## 6. JSON 追い出し（OCR 済み画像から）: `export_yomi_json.py`

`result/<name>/page_images/` から YomiToku JSON を再生成します。

```bash
poetry run python export_yomi_json.py --input result/<name> --mode lite
```

- `--input <dir>`（必須）
- `--pages 38,39`: 対象ページ（未指定なら `page_images` 全部）
- `--mode {lite,full}`（既定 `lite`）

## 7. 数式スニペット埋め込み（PoC）: `math_snippet_extractor.py`

```bash
poetry run python math_snippet_extractor.py --input result/<name> --base-name <name>
```

- `--input <dir>`（必須）
- `--base-name <str>`: `<base-name>_merged.md` を読み込み（省略時は input ディレクトリ名）
- `--json-dir <dir>` / `--page-images <dir>`: 入力差し替え
- `--output-md <path>`: 出力 md
- `--padding <int>`（既定 `6`）
- `--max-per-page <int>`（既定 `20`）
- `--min-score <float>`（既定 `0.6`）
- `--min-ops <int>`（既定 `1`）
- `--max-chars <int>`（既定 `120`）
- `--max-aspect <float>`（既定 `6.0`）

## 8. Markdown クリーンアップ: `markdown_cleanup.py`

```bash
poetry run python markdown_cleanup.py <input.md>
```

- `--output <path>`: 別ファイルに書き出し（省略時は in-place）

## 9. 画像前処理（単体）: `image_preprocessor.py`

```bash
poetry run python image_preprocessor.py <input> --output <out.png> --profile ocr_default
```

- `--profile <name>`（既定 `ocr_default`）
- `--target-long-edge <int>`
- `--contrast/--brightness/--sharpness <float>`
- `--binarize/--no-binarize`
- `--denoise-size <int>` / `--denoise-strong`
- `--keep-color/--no-keep-color` / `--no-grayscale`
- `--page-number <int>`（既定 `1`）

## 10. テキスト PDF を Markdown 化（PoC）: `text_pdf.py`

```bash
poetry run python text_pdf.py <input.pdf> --output <out.md>
```

## 11. コマンド早見（補助）: `scripts/command_help.py`

```bash
poetry run python scripts/command_help.py
```

## 12. UI 設定画面に反映する候補（たたき台）

まずは `dispatcher.py` のオプションを UI の基本設定にし、PDF 固有の設定は「詳細」へ寄せるのが安全です。

### 基本（出しやすい）
- `--mode`
- `--formats`
- `--figure`
- `--ocr-profile`（画像）
- `--image-as-pdf` / `--image-dpi`（画像）
- `--fallback-tesseract` / `--force-tesseract-merge`
- `--start` / `--end`（PDFごと・未指定=全ページ。詳細でも良い）

### 詳細/上級（隠す or 折り畳み）
- `--chunk-size` / `--enable-rest` / `--rest-seconds`
- `--drop-page-images`
- `--emit-json`（xlsx を使う場合は内部で on にする運用も可）
- `--icon-profile` / `--icon-policy` / `--icon-config` / `--icon-log*`
- `--math-refiner` / `--math-score` / `--math-resized-shape`
- `--output-root` / `--device` / `--svg-dpi`
