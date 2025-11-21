# アーキテクチャ / 実装指針

このドキュメントは、README と Tasks に記載したゴール・ロードマップを支えるモジュール設計を記録する。具体的な優先順位や作業項目は `readme.md` / `docs/Tasks.md` を参照し、ここではコンポーネント間の責務分担に集中する。

---

## 1. システム全体像

1. `ingest` 層が入力ファイル（画像 / PDF）を受け取り、ページ画像およびメタ情報を生成して `result/<入力ファイル名>/` に配置する。
2. `ocr` 層がページ画像を YomiToku（lite/full）で処理し、Markdown + 図表画像を同ディレクトリ以下に書き出す。必要に応じて fallback エンジンを呼び出す。
3. `postprocess` 層がページ順に Markdown を連結 (`poppler/merged_md.py` など) し、中間成果物 `*_merged.md` を生成する。
4. `export` 層が Markdown を Word/Excel など最終フォーマットへ変換する。

データの流れを単純化すると下記のようになる。

```
入力ファイル → ingest → result/<name>/*.png
                           ↓
                        ocr (YomiToku / fallback)
                           ↓
                 result/<name>/*.md + figures
                           ↓
                postprocess (merge, cleanup)
                           ↓
        result/<name>/<name>_merged.md (docx/xlsx)
```

---

## 2. コンポーネント責務

### ingest.py
- 画像 / PDF / テキスト PDF の判定。
- PDF→画像変換（`pdf2image` + `poppler`）。
- 一時ファイルや作業ディレクトリ (`result/`, `result/figures/`) の管理とクリーンアップ。

### ocr.py / ocr_chanked.py
- YomiToku CLI を呼び出すラッパ。`lite`/`full` モード切り替えや `--figure` オプションのオン/オフ管理。
- 1 ページごとの Markdown / 図表ファイル命名規則（`fig_page001_01.png` 等）を統一。
- フェーズ 3 で fallback チェーン（YomiToku → Tesseract → PaddleOCR etc.）を組み込み、失敗ログを残す。

### dispatcher.py（フェーズ 2）
- 入力ファイル情報を受け取り、OCR すべきかテキスト抽出すべきかを決定。
- 判定ロジックを 1 箇所に集約し、CLI からは `dispatcher.run(path, mode="auto")` のように呼び出せる形にする。

### text_pdf.py（フェーズ 2）
- テキスト埋め込み PDF から `markitdown` / `pdfplumber` 等でテキスト抽出。
- 基本的な Markdown 整形（段落、箇条書き、表の検出）を担当。

### postprocess.py / poppler/merged_md.py
- ページ Markdown のソート＋マージと `# Page n` の挿入。
- `--keep-pages` フラグでページファイルを残すかどうかを制御。
- マージ後のクリーンアップ（`layout.jpeg` / `ocr.jpeg` など不要ファイル削除）。

### export.py / export_docx.py
- Markdown → docx 変換（`python-docx` または Pandoc ラッパ）。
- 将来的に Markdown → xlsx 変換を担う。テーブル抽出は `markdown-it-py` 等の再利用も検討。

---

## 3. データ配置と命名

| ディレクトリ | 役割 |
| --- | --- |
| `result/<name>/` | ページ分割後の画像 (`page_0001.png`) とページ単位 Markdown (`page_0001.md`) |
| `result/<name>/figures/` | 図表抽出画像。`fig_page001_01.png` のようにページ + 連番で命名 |
| `result/<name>/<name>_merged.md` 等 | 中間成果物（`merged.md`, `*.docx` など） |

命名規則の例：
- ページ画像: `page_{page:04d}.png`
- ページ Markdown: `page_{page:04d}_p{part}.md`（チャンク単位を含める場合）
- 図表: `page-{page:03d}-fig-{idx:02d}.png`

---

## 4. 今後の検討メモ

- **CLI 統一**: ingest → ocr → merge → export を一つの `ocrdoc run input.pdf` のような CLI で統括。
- **ロギング/監視**: ページ単位の処理時間と fallback 発生状況を構造化ログで残し、失敗ページを再投入しやすくする。
- **エクスポート拡張**: docx 出力に加え、表だけを CSV/xlsx に切り出すパスを `export.py` に用意する。

上記は README/Tasks での優先度管理とは独立して、アーキテクチャ的に意識しておきたいポイントである。
