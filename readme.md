# OCR to Doc（スマホスキャン OCR → Markdown / Word 変換）

スマホで撮った画像や、iOS の「書類をスキャン」で作った PDF を入力に、ローカルで OCR→Markdown（→docx/xlsx）まで繋ぐツールです。  
確定仕様は `docs/spec.md` に集約しています。

## できること
- 入力: PDF / HEIC / JPG / PNG（UI からドラッグ&ドロップ or ファイル選択）
- 出力: Markdown / Word（docx）/ Excel（xlsx）/ CSV（設定で選択）
- 既定の出力先: `result/<入力名>/`（アプリと同じフォルダ配下）

## 使い方（Windows ポータブル版 / はじめての人向け）

### 1) 配布物を入手
配布zipは以下のどちらかから取得します。
- GitHub Releases（推奨: ログイン無しでDLしやすい）
- GitHub Actions の Artifacts（要GitHubログイン/権限。一定期間で消えます）

### 2) 解凍して起動
1. zip を任意の場所に解凍（例: `Downloads` や `Desktop`。`Program Files` 配下は避ける）
2. `ocr-to-doc.exe` を起動
   - 初回は Windows の警告（SmartScreen）が出ることがあります

### 3) ファイルを追加して実行
1. 画面の枠へ **ドラッグ&ドロップ**、またはクリックしてファイル選択
2. 必要ならトリミング/出力形式などを設定
3. 「処理を実行」
4. 結果は `result/` 配下に出力されます（UIの「結果」からも参照できます）

### 4) 初回ネットワークについて
初回実行時に、OCRライブラリ側がモデル等を取得する場合があります（環境により時間がかかります）。  
社内PCなどでプロキシ制限がある場合は、まず小さな画像で動作確認してください。

## 使い方（CLI / 自己テスト）
ポータブル版は `ocr-to-doc.exe` 単体で簡易CLIと自己テストが動きます。

```powershell
# 自己テスト（配布物の不備検知用）
.\ocr-to-doc.exe --self-test

# CLI実行（例: sample.pdf を処理。-- 以降は dispatcher.py に透過）
.\ocr-to-doc.exe --cli "C:\path\to\sample.pdf" -- --formats md docx --mode lite
```

## 開発者向け（Poetry で実行）

```bash
# 依存インストール（初回のみ）
poetry install

# PDF / 画像 / HEIC を自動判定して処理（result/<入力名>/ に出力）
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
- 配布設計（暫定）: `docs/distribution.md`
