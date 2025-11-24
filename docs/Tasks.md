# Tasks

## フェーズ別ロードマップ

### フェーズ 1：基本 OCR パイプライン
- `ingest.py` で画像/PDF の判定を実装し、必要になった時点で PDF→画像変換を拡張する（現状は後回し）。
- `ocr.py`（もしくは既存の `ocr_chanked.py`）で YomiToku full/lite の切り替えと、図表画像の抽出 (`--figure`) を安定化させる。`ocr_chanked.py` は `--label` で出力フォルダを分けられるよう再構成し、同一 PDF でもページ範囲ごとに別名で保存できるようにする。
- `ocr_chanked.py` は `--label` で出力フォルダを選べるよう再構成し、同一 PDF でもページ範囲ごとに別名で保存できるようにする。
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
- プレーンテキスト出力の安定化: `ocr_chanked.py` / `markdown_cleanup.py` の既定動作をプレーンテキスト優先とし、余計な `$$` や数式置換を入れないフローを仕上げる（必要時のみオプションで MathRefiner を利用）。
- PoC テンプレートの運用: `docs/requirements_template.md` / `docs/templates/requirements_general_template.md` を埋めた上で新機能に着手するプロセスを定着させる。

## 中期 (Mid-term)

- エラー処理とログの強化: OCR 実行やファイル I/O に失敗した際のメッセージ、再実行フローを整理する。
- CLI UX 向上: Click や argparse を導入し、共通の CLI インターフェースにまとめる。
- Windows CI 拡充: `python -m compileall` だけでなく、サンプル PDF を使った結合・docx 出力の smoke test を実施できるようにする。
- プレーンテキスト + 後工程整形のワークフローを確立し、LLM/人手どちらでも整形できるようにする。
- 分数や数式が崩れた際に該当領域だけを画像として埋め込めるよう、将来の GUI/補助ツールで手動マーキング→トリミング→Markdown 反映までを一貫操作できる仕組みを検討する（当面は案として保持）。
- OCR エンジン側で数式領域を直接検出し、そのボックス情報から数式だけをトリミングして Markdown へ差し込む機能は、実装コスト（追加モデルや推論時間）との兼ね合いを見て今後採否を判断する。
- layoutparser などのレイアウト解析ライブラリを導入して公式ブロックを自動抽出する案も候補として保持し、必要性とコストが見合うタイミングで着手可否を決める。

## 自動化 TODO（Cleanup pipeline）

- 画像参照の整理: `ocr.py` の小型アイコンフィルタをログで検証しつつ、単色・重複画像を段階的に除外できるよう閾値調整と結果レポート（例: `icon_candidates.json`）を追加する。
- 記号・ギリシャ文字の統一: 誤 OCR 例（σ→o、ρ→p 等）を収集し、`markdown_cleanup.py` に置換テーブルを設けて自動補正する。対象一覧をメンテしやすい YAML/JSON へ切り出すことも検討。
- 簡単な分数・数式の整形: `MTBF/MTBF+MTTR` など頻出パターンを正規表現で検出し、`$\frac{...}{...}$` など LaTeX 形式へ変換するオプションを `markdown_cleanup.py` に追加する。対象式のテンプレートを決めた上で段階的に拡張する。

## 長期 (Long-term)

- GUI 化の検討: 将来的な GUI 展開を見据えて、フロントエンド構成の検討（PySide/Qt 等）やバックエンド API 化の調査を進める。
- Excel 出力機能: Markdown から Excel へ変換するパイプラインの設計・実装。
- クラウド OCR 連携: CPU で重い処理を軽減するために Azure / Google Vision などクラウド OCR とのハイブリッド運用を検討する。
- ローカル LLM 連携: データを外部へ出さずに整形精度を高めるオプションを PoC→実装の流れで検討する。
