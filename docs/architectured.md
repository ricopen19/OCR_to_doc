# アーキテクチャ / 実装指針

このドキュメントは、モジュール間の責務分担（アーキテクチャ）に集中します。  
確定した入出力仕様・ディレクトリ構成・既定挙動は `docs/spec.md` を参照してください。

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
- PDF→画像変換（`pdf2image` + `poppler`）および前処理（DPI・二値化・ノイズ除去など）の適用。
- 一時ファイルや作業ディレクトリ (`result/`, `result/figures/`) の管理とクリーンアップ。
- 画像入力では `image_normalizer` で形式統一し、`image_preprocessor` で OCR 用の前処理プロファイルを生成する。

### ocr.py / ocr_chanked.py
- YomiToku CLI を呼び出すラッパ。`lite`/`full` モード切り替えや `--figure` オプションのオン/オフ管理。`ocr_chanked.py` は `--label` によって同一 PDF でもページ範囲ごとに別名ディレクトリへ出力できる。
- 命名規則・出力ファイルは `docs/spec.md` に集約。
- fallback や高度化の優先度は `docs/Tasks.md` を参照。

### dispatcher.py
- 入力ファイル情報を受け取り、PDF/画像の処理経路を切り替えるエントリポイント。
- `python dispatcher.py <path> -- <ocr_chanked.py の追加引数>` の形式で、PDF 側の追加引数を透過できる。

### text_pdf.py（フェーズ 2）
- テキスト埋め込み PDF から `markitdown` / `pdfplumber` 等でテキスト抽出。
- 基本的な Markdown 整形（段落、箇条書き、表の検出）を担当。現状は PoC 途中で、抽出精度/整形ルールは今後詰める。

### postprocess.py / poppler/merged_md.py
- ページ Markdown のソート＋マージと `# Page n` の挿入。
- 結合後に `markdown_cleanup.py` で整形して一貫性を保つ（詳細は `docs/spec.md`）。

### export.py / export_docx.py
- Markdown → docx 変換（`python-docx` 実装、テーブル/箇条書き/画像のスタイル調整済み）。
- 将来的に Markdown → xlsx 変換を担う。テーブル抽出は `markdown-it-py` 等の再利用も検討。Excel 変換 PoC は `export_excel_poc.py` で進行中。

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
- **エクスポート拡張**: docx 出力に加え、表だけを CSV/xlsx に切り出すパスを `export.py` に用意する。

未確定の改善案や優先度は `docs/Tasks.md` / `docs/poc_results/` を参照してください。
