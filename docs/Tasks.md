# Tasks

## 優先度高 (Short-term)

- export_docx.py: `python-docx` 変換のレイアウト品質確認。特に表や箇条書きが崩れていないかサンプル PDF を使って検証し、必要ならスタイル調整。
- poppler/merged_md.py: 画像リンクを含む Markdown のマージと docx 出力の相性確認。必要なら export_docx.py 側で `<img>` / `![]()` を扱う処理を追加する。
- docs/architectured.md 更新: 現状はモジュール案と実装が乖離しており、実際のスクリプト構成（`ocr_per_page.py`, `ocr_chanked.py`, `merged_md.py`, `export_docx.py` 等）に合わせた説明へアップデートする。

## 中期 (Mid-term)

- エラー処理とログの強化: OCR 実行やファイル I/O に失敗した際のメッセージ、再実行フローを整理する。
- CLI UX 向上: Click や argparse を導入し、共通の CLI インターフェースにまとめる。
- Windows CI 拡充: `python -m compileall` だけでなく、サンプル PDF を使った結合・docx 出力の smoke test を実施できるようにする。

## 長期 (Long-term)

- GUI 化の検討: 将来的な GUI 展開を見据えて、フロントエンド構成の検討（PySide/Qt 等）やバックエンド API 化の調査を進める。
- Excel 出力機能: Markdown から Excel へ変換するパイプラインの設計・実装。
- クラウド OCR 連携: CPU で重い処理を軽減するために Azure / Google Vision などクラウド OCR とのハイブリッド運用を検討する。
