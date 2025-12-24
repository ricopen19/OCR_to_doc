# 変更ログ

## 2025-12-24

- **GUI 改善**: 設定画面で未保存の変更がある場合、別画面へ移動する前に確認モーダル（保存して移動 / 保存せず移動 / キャンセル）を表示するようにした。

## 2025-12-21

- **ドキュメント更新**: `docs/Tasks.md`, `docs/ui_design_plan.md`, `docs/ui_requirements.md` を、現状の UI 実装（Tauri commands / 画面構成 / 設定保存先 / 既知の未反映項目）に合わせて更新。
- **タスク整理**: `docs/Tasks.md` は未完のみを残し、完了項目はこの `docs/change_log.md` に集約する方針を明文化。

## 2025-12-12

- **GUI 実装 (Tauri + React)**: 基本的な GUI 実装が完了し、`dispatcher.py` を介した PDF 処理の実行、進捗表示、結果確認が可能になった。
- **PDF → Markdown / Word**: 変換フローが安定し、フェーズ 1 の主要ゴールを達成。
- **ドキュメント更新**: `Tasks.md`, `context_engineering.md`, `ui_design_plan.md` を更新し、GUI 機能改善（設定、履歴、プレビュー等）や Excel 変換に関するタスクを追加。

## 2025-12-05

- `markdown_cleanup.py` にページ見出し `# Page n` 内側の孤立 H1 を自動で H2 に落とす処理を追加し、ページヘッダー階層と本文見出しの衝突を防止。
- 同ファイルに「ビット」単位の崩れ（`\text{\text{$\text{ビット}$}}` の多重ネストなど）を検出してプレーンな「ビット」へ正規化するルールを追加し、通信分野の記述が毎回崩れる問題を解消。
- `result/応用情報技術者_p77-80/応用情報技術者_p77-80_merged.md` を新ルールで再クリーンアップし、単位表記を正常化。

## 2025-11-28

- `export_excel_poc.py` に機能追加: (1) レビュー用ステータス列（○/×ドロップダウン、Table範囲外に配置）、(2) 数値/百分率/日付の自動判定・書式設定、(3) メタ情報シート自動生成、(4) 罫線と列幅の調整改善。結合セルを含む表では Table 化をスキップし破損警告を回避。オプション `--csv-tables-only` で CSV の段落行を除外できるようにした。
- `pyproject.toml` に `openpyxl` を追加し、PoC 実行環境を整備。
- `docs/poc_results/2025-11-25-excel_format_comparison.md` と 要件シートを更新し、Excel 出力 PoC の進捗（JSON を真実ソース、CSV/HTML を補助）と実装済み機能を記録。

## 2025-11-26

- `dispatcher.py` を本格導入し、単一の CLI で PDF/画像/HEIC/SVG を自動判別。PDF は `ocr_chanked.py` へ委譲し、画像は内部で HEIC/SVG→PNG 変換後に `ocr.run_ocr` を呼び出して `result/<入力名>/` へ出力するようにした。`--` 区切りで追加の `ocr_chanked.py` パラメータも手渡し可能。
- `image_normalizer.py` を追加し、HEIC/HEIF/SVG を PNG へ正規化する共通ユーティリティを実装。これに合わせて `ingest.py` の拡張子判定と README を更新し、入力一覧に HEIC/HEIF/SVG を officially 掲載。依存として `pyheif` / `cairosvg` を追加。
- `image_normalizer.py` を追加し、HEIC/HEIF/SVG を PNG へ正規化する共通ユーティリティを実装。これに合わせて `ingest.py` の拡張子判定と README を更新し、入力一覧に HEIC/HEIF/SVG を officially 掲載。依存として `pillow-heif` / `cairosvg` を追加し、HEIC 変換の libheif 依存を排除した。
- `tests/test_image_normalizer.py` を作成し、拡張子判定・SVG 変換・HEIC 変換フックの単体テストを整備。`unittest.skipUnless` で `cairosvg` 未導入環境でも graceful にスキップできるようにした。
- `image_preprocessor.py` を新設し、画像入力時に OCR 向け（高コントラスト・グレー）を生成。PowerPoint 向けの同時生成と `--presentation-profile` は廃止し、必要時は別プロファイル実行で対応。
- `ocr_chanked.py` の JSON 出力はデフォルト無効。`--emit-json on` で常に、`--emit-json auto` で数式がありそうなページのみ出力に対応。
- README / env_and_limits / architectured に前処理パイプラインと複数画像を PDF 化する運用ガイドを追記。

## 2025-11-25

- PDF→Markdown 前処理の A/B 検証を完了し、`docs/poc_results/2025-11-24-preprocess_ab.md` にベースライン設定（200DPI + adaptive + bilateral）と Go/No-Go を記録。`ingest.py` / `ocr_chanked.py` ではこの設定をデフォルトに据え、検証タスクをクローズ。
- `markdown_cleanup.py` / `postprocess.py` を刷新し、数式テンプレ（`formula_templates.json`）、単位の `\text{}` ラップ、ブロック数式内の余計な `$`/改行除去など一連の整形タスクを完了。`tests/test_markdown_cleanup.py` を追加し、回帰テストで挙動を検証できるようにした（ページ画像 `<details>` 埋め込みは後に廃止）。
- `ocr_chanked.py` のプレーンテキスト既定フローおよびドキュメント（README / docs/architectured.md）を最新状態へ反映。icon フィルタ閾値や `--icon-config` の説明も README で整備し、短期タスクの「ドキュメント更新」項目をクローズ。

## 2025-11-24

- `markdown_cleanup.py` を拡張し、見出し (`# $1-1-1$` → `### 1-1-1`)、章リスト (`- □ $1-1-1$ ...` → `- 1-1-1 ...`)、ページ末尾 (`...50` → `（p.50）`)、`<br>` だらけの本文などを自動整形するルールを追加。URL 行は崩さないよう例外処理し、`page_images/` 参照に混入した `$` も自動で除去するようにした。
- `ocr.py` で各ページ処理の最後に `markdown_cleanup.clean_file()` を呼び出し、OCR 直後から統一書式が得られるようにした。また `ocr_chanked.py` にはページ画像保存を既定化する `--(no-)keep-page-images` を追加（後に `<details>` 埋め込み自体は撤廃）。
- 小型かつ単色アイコンの図版を自動削除するフィルタを `ocr.py` へ追加（面積・最大辺・色数・標準偏差で判定し、削除時は Markdown から参照も除去）。閾値は今後ログを見ながら調整予定。
- README の「ざっくり使い方」に `poppler/merged_md.py --keep-pages` の例と、ページ画像参照の運用メモを追記（後に Markdown への埋め込みは廃止）。

## 2025-11-20

- OCR 出力ディレクトリを `result/` に統一し、図版は `result/figures/fig_page001_01.png` の形式で命名するよう `ocr.py` を更新。Markdown 内の画像リンクも自動で置換される。
- `ocr_chanked.py` に `--start` / `--end` / `--chunk-size` / `--rest-seconds` オプションを追加し、PDF の一部ページだけを変換できるようにした。これに伴い `ocr_per_page.py` は役割を終えたため削除。
- 出力先を `result/<入力ファイル名>/` に統一し、同フォルダ内に `figures/` と `<name>_merged.md` を配置するようにした。
- README へページ範囲指定と `--mode full` の使い方を追記。

## 2025-11-15

- `ocr_per_page.py` / `ocr_chanked.py` でページごとの Markdown を `page_001.md` 形式へ正規化し、処理完了時に `poppler/merged_md.py` を自動実行してページファイルを削除するように変更。`poppler/merged_md.py` には `--base-name` オプションを追加し、OCR 入力 PDF 名をベースに `<ファイル名>_merged.md` を出力できるようにした。既定ではページごとの Markdown を削除し、`--keep-pages` で保持可能。
- `export_docx.py` を Pandoc 実行から `python-docx` ベースの実装へ刷新。Markdown のヘッダー／箇条書き／表（Pipe テーブル）をパースして Word 上でもレイアウトを維持できるようにし、表は `Table Grid` でセル毎に展開。あわせて `python-docx` を依存関係へ追加。
- Poetry でルートプロジェクトのインストールエラーが出ていたため、`pyproject.toml` に `package-mode = false` を設定し、依存管理だけを行う構成とした（外部配布を想定しないモード）。
- GUI 展開を視野に入れて一時パッケージ化を試みたが、現状は Poetry による環境再現のみを重視する方針に戻し、`package-mode = false` を再設定するとともに `yomitoku_ocr/` ディレクトリを削除。
- Windows 環境での動作検証を自動化するため、`.github/workflows/windows-ci.yml` を追加。`actions/setup-python@v5` + Poetry で依存をインストールし、主要なスクリプトを `python -m compileall` で検証するワークフローを push / PR 時に実行。
