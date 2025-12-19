# スマホスキャン OCR → Markdown / Word 変換ツール

スマホで撮った画像や、iOS の「書類をスキャン」で作った PDF を入力に、ローカルで OCR→Markdown（→docx/xlsx）まで繋ぐツール群です。  
確定仕様は `docs/spec.md` に集約しています。

## 使い方（最短）

```bash
# 依存インストール（初回のみ）
poetry install

# PDF / 画像 / HEIC / SVG を自動判定して処理（result/<入力名>/ に出力）
poetry run python dispatcher.py sample.pdf
poetry run python dispatcher.py sample.heic

# docx まで作る
poetry run python dispatcher.py sample.pdf --formats md docx

# PDF のページ範囲などを ocr_chanked.py に渡す（-- 以降が透過されます）
poetry run python dispatcher.py sample.pdf -- --start 11 --end 20
```

## 参考ドキュメント
- 確定仕様（実装済み/入出力）: `docs/spec.md`
- CLI コマンド一覧: `docs/cli_commands.md`
- タスク（未完のみ）: `docs/Tasks.md`
- アーキテクチャ（責務分割）: `docs/architectured.md`
- 実行環境/制約: `docs/env_and_limits.md`
