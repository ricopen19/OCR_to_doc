# アーキテクチャ / 実装指針

このドキュメントは、モジュール間の責務分担（アーキテクチャ）に集中します。  
確定した入出力仕様・ディレクトリ構成・既定挙動は `docs/spec.md` を参照してください。

---

## 1. システム全体像

1. `dispatcher.py` が入力ファイル（PDF / 画像）を判定し、PDF は `ocr_chanked.py`、画像は `ocr.py` 経路へ分岐する。
2. PDF 経路（`ocr_chanked.py`）は Poppler + `pdf2image` でページ画像を生成しつつ、1ページずつ YomiToku を実行して `page_###.md` と `figures/` を作る（必要なら `yomi_formats/json` 等も出力）。
3. マージ（`postprocess.py` / `poppler/merged_md.py`）が `page_*.md` を結合して `<base>_merged.md` を作り、既定でページ md は削除する（`math_review.csv` も生成）。
4. エクスポートは `dispatcher.py` が担当し、`export_docx.py`（docx）や `export_excel_poc.py`（xlsx/PoC）等を呼び出す。

データの流れを単純化すると下記のようになる。

```
入力ファイル → dispatcher
     ├─ PDF → ocr_chanked → result/<base>/page_images/*.png
     │                 ↓
     │            ocr (YomiToku / fallback)
     │                 ↓
     │     result/<base>/page_###.md + figures (+ yomi_formats/*)
     │                 ↓
     │ postprocess (merge, cleanup) → <base>_merged.md (+ math_review.csv)
     │                 ↓
     │      export (docx/xlsx/csv) → <base>_merged.docx / <base>.xlsx / ...
     └─ 画像 → 正規化/前処理 → ocr (YomiToku) → result/<stem>/page_001.md (+ figures)
```

---

## 2. コンポーネント責務

### ingest.py
- 入力種別（PDF / 画像）の判定と、PDF のページ数の best-effort 取得。
- このリポジトリでは重い処理（PDF→画像化や OCR 実行）は `ocr_chanked.py` / `dispatcher.py` 側に寄せている。

### image_normalizer.py
- HEIC/HEIF/SVG を PNG へ正規化し、OCR 前段の「入力ゆらぎ」を吸収する。
- `dispatcher.py` の画像経路では `result/<stem>/converted/` 配下に変換物を保存する。

### image_preprocessor.py
- 画像 OCR 向けの前処理（コントラスト、二値化、デノイズ等）をプロファイルとして提供する。
- `dispatcher.py` の画像経路では `result/<stem>/preprocessed/<profile>/page_001.png` を生成して OCR 入力に使う。

### ocr.py
- YomiToku CLI を呼び出すラッパ（md/json/csv）。
- 出力 Markdown の正規化（`page_###.md` 命名）と、図版の命名統一（`fig_page###_##.png`）。
- 小型/単色アイコンの自動除外（既定は自動）。候補ログは `figures/icon_candidates.json` に出る場合がある。
- 必要に応じて pytesseract フォールバックや「追記マージ」を行う（`fallback.log` を出す）。

### ocr_chanked.py
- PDF→ページ画像化（Poppler + `pdf2image`）と、ページ単位の OCR 実行をまとめた CLI。
- `--start/--end` や `--chunk-size/--enable-rest` による低スペック対策、`--label` による出力ディレクトリ命名を担当。
- JSON/CSV の追加出力（`--emit-json` / `--emit-csv`）、トリミング（`--crop`）、アイコンフィルタ設定（`--icon-*`）もここで制御する。

### dispatcher.py
- 推奨エントリポイント（PDF/画像を自動判定して処理）。
- PDF は `ocr_chanked.py` に委譲し、`--` 以降で PDF 側の追加引数を透過できる。
- 画像は正規化（`image_normalizer`）→前処理（`image_preprocessor`）→`ocr.run_ocr` の順で実行する。
- `--formats` に応じて docx/xlsx/csv の後処理（`export_docx.py` / `export_excel_poc.py` 等）までまとめて実行する。
- `--image-as-pdf` を指定すると、画像でも一度 PDF 化して PDF 経路（`ocr_chanked.py`）へ回す。

### text_pdf.py（フェーズ 2）
- テキスト埋め込み PDF から `markitdown` / `pdfplumber` 等でテキスト抽出。
- 基本的な Markdown 整形（段落、箇条書き、表の検出）を担当。現状は PoC 途中で、抽出精度/整形ルールは今後詰める。

### postprocess.py / poppler/merged_md.py
- ページ Markdown のソート＋マージと `# Page n` の挿入。
- 結合後に `markdown_cleanup.py` で整形して一貫性を保つ（詳細は `docs/spec.md`）。

### export_docx.py
- Markdown → docx 変換（`python-docx` 実装）。

### export_excel_poc.py（PoC）
- YomiToku の出力（json/csv/html）を xlsx に変換する PoC。
- 実運用では `dispatcher.py --formats xlsx` が内部で `yomi_formats/json` を生成し、これを基に xlsx を作る。

### export_yomi_json.py（補助）
- `result/<base>/page_images/` から YomiToku JSON を再生成する（再OCRなしで json を取り直したい時の補助）。

---

### 2.5 GUI Layer (Tauri + React)
- **Tauri (Rust)**: バックエンドプロセス (`dispatcher.py` 等) の起動・監視、ファイルシステム操作、設定管理を担当。
- **React (Frontend)**: ユーザーインターフェース。ジョブの作成、進捗状況の可視化、結果プレビュー、設定変更を行う。
- **通信**: Tauri コマンドとイベントを通じてフロントエンドとバックエンドが非同期に連携する。

---

## 3. データ配置と命名
ディレクトリ構成・成果物一覧・命名規則は `docs/spec.md` に集約しました。

---

## 4. 今後の検討メモ

- **GUI 統合**: GUI から `dispatcher.py` を呼ぶ形を前提に、設定/履歴/プレビューなどの拡充を検討する。
- **CLI 統一**: ingest → ocr → merge → export を一つの `ocrdoc run input.pdf` のような CLI で統括（GUI からもこれを呼ぶ形が理想）。
- **ロギング/監視**: ページ単位の処理時間と fallback 発生状況を構造化ログで残し、失敗ページを再投入しやすくする。
- **エクスポート拡張**: docx 出力に加え、表だけを CSV/xlsx に切り出すパスを `dispatcher.py`（または専用の export スクリプト）として整理する。

未確定の改善案や優先度は `docs/Tasks.md` / `docs/poc_results/` を参照してください。
