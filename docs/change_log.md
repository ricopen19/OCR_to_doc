# 変更ログ

## 2025-11-15

- `ocr_per_page.py` / `ocr_chanked.py` でページごとの Markdown を `page_001.md` 形式へ正規化し、処理完了時に `poppler/merged_md.py` を自動実行してページファイルを削除するように変更。`poppler/merged_md.py` には `--base-name` オプションを追加し、OCR 入力 PDF 名をベースに `<ファイル名>_merged.md` を出力できるようにした。既定ではページごとの Markdown を削除し、`--keep-pages` で保持可能。
- `export_docx.py` を Pandoc 実行から `python-docx` ベースの実装へ刷新。Markdown のヘッダー／箇条書き／表（Pipe テーブル）をパースして Word 上でもレイアウトを維持できるようにし、表は `Table Grid` でセル毎に展開。あわせて `python-docx` を依存関係へ追加。
- Poetry でルートプロジェクトのインストールエラーが出ていたため、`pyproject.toml` に `package-mode = false` を設定し、依存管理だけを行う構成とした（外部配布を想定しないモード）。
- GUI 展開を視野に入れて一時パッケージ化を試みたが、現状は Poetry による環境再現のみを重視する方針に戻し、`package-mode = false` を再設定するとともに `yomitoku_ocr/` ディレクトリを削除。
- Windows 環境での動作検証を自動化するため、`.github/workflows/windows-ci.yml` を追加。`actions/setup-python@v5` + Poetry で依存をインストールし、主要なスクリプトを `python -m compileall` で検証するワークフローを push / PR 時に実行。

## 2025-11-20

- OCR 出力ディレクトリを `result/` に統一し、図版は `result/figures/fig_page001_01.png` の形式で命名するよう `ocr.py` を更新。Markdown 内の画像リンクも自動で置換される。
- `ocr_chanked.py` に `--start` / `--end` / `--chunk-size` / `--rest-seconds` オプションを追加し、PDF の一部ページだけを変換できるようにした。これに伴い `ocr_per_page.py` は役割を終えたため削除。
- 出力先を `result/<入力ファイル名>/` に統一し、同フォルダ内に `figures/` と `<name>_merged.md` を配置するようにした。
- README へページ範囲指定と `--mode full` の使い方を追記。
