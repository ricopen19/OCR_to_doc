# Tasks

## フェーズ別ロードマップ

### フェーズ 1：基本 OCR パイプライン
- `ingest.py` で画像/PDF の判定を実装し、必要になった時点で PDF→画像変換を拡張する（現状は後回し）。
- `ocr.py`（もしくは既存の `ocr_chanked.py`）で YomiToku full/lite の切り替えと、図表画像の抽出 (`--figure`) を安定化させる。
- `postprocess.py` / `poppler/merged_md.py` でページ Markdown をソート結合し、`# Page n` ヘッダ付きの `*_merged.md` を生成する。
- `export_docx.py` は画像埋め込みまで対応済みだが、Word 数式への変換（LaTeX→OMML）は後続フェーズで検討する。

### フェーズ 2：エンジン切り替えとテキスト PDF
- `dispatcher.py` を用意し、入力種別に応じて OCR（画像系）か `text_pdf.py`（テキスト PDF）へ振り分ける。
- `text_pdf.py` で `markitdown` や `pdfplumber` を利用したテキスト抽出 → Markdown 整形パイプラインを構築する。
- LaTeX 数式を Word 数式に変換する機能はこのフェーズ以降で対応（`latex2mathml` + `mml2omml` 等を検討）。

### フェーズ 3：エクスポート＆fallback
- `export.py` を実装し、Pandoc などで Markdown→docx を自動化。表抽出を分離し、将来の xlsx 変換へ備える。
- `ocr.py` に fallback チェーン（YomiToku lite→full→Tesseract/PaddleOCR 等）を組み込み、失敗ページの再処理とログ出力を行う。

## 優先度高 (Short-term)

- export_docx.py: `python-docx` 変換のレイアウト品質確認。特に表や箇条書きが崩れていないかサンプル PDF を使って検証し、必要ならスタイル調整。
- poppler/merged_md.py: 画像リンクを含む Markdown のマージと docx 出力の相性確認。必要なら export_docx.py 側で `<img>` / `![]()` を扱う処理を追加する。
- docs/architectured.md 更新: 新しい README/Tasks への分割を踏まえて記述をスリム化し、アーキテクチャの芯になる情報に集中させる。
- 環境整備: macOS で開発しつつ Windows 用 Poppler バイナリを同梱する構成を維持（GitHub Actions の Windows テストは後日）。

## 中期 (Mid-term)

- エラー処理とログの強化: OCR 実行やファイル I/O に失敗した際のメッセージ、再実行フローを整理する。
- CLI UX 向上: Click や argparse を導入し、共通の CLI インターフェースにまとめる。
- Windows CI 拡充: `python -m compileall` だけでなく、サンプル PDF を使った結合・docx 出力の smoke test を実施できるようにする。

## 長期 (Long-term)

- GUI 化の検討: 将来的な GUI 展開を見据えて、フロントエンド構成の検討（PySide/Qt 等）やバックエンド API 化の調査を進める。
- Excel 出力機能: Markdown から Excel へ変換するパイプラインの設計・実装。
- クラウド OCR 連携: CPU で重い処理を軽減するために Azure / Google Vision などクラウド OCR とのハイブリッド運用を検討する。
