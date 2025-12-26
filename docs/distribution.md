# 配布設計メモ（暫定）

## 目的

- ユーザーに PATH 設定や事前インストールを求めず、ダウンロード後すぐ使える状態にする。
- Teams チャット添付の上限（100MB）を意識し、「軽量本体」と「重いランタイム」を分割する。
- 初回のみネット接続は利用可能（職場PCの通常環境を想定）。

## 配布物（2段構え）

### 1) アプリ本体（Teams 添付用 / ≤100MB目標）

- Tauri のビルド成果物（実行ファイル + 付随ファイル）
- Python スクリプト群（`dispatcher.py` など）
- `poppler/Library/bin`（Windows 用。PDF→画像化で必須。※将来 `poppler/win/bin` へ整理してもよい）
- 依存（`.venv` / embeddable Python / モデル等）は含めない

### 2) Python ランタイム（初回のみ取得）

- Windows 向けの Python 実行環境 + 依存（`yomitoku` 等）
- 配布先は GitHub Releases を第一候補
- 目標: 初回起動時に自動でダウンロード/検証/展開し、以後はローカルにキャッシュして使う

※ 自動取得は実装タスク（起動時セットアップ）であり、本ドキュメントは方針のメモに留める。

## 実装上の前提（重要）

現状の GUI（Tauri）は、実行中に `dispatcher.py` の場所から `project_root` を解決し、その配下で Python を探して起動する。

- `project_root/dispatcher.py` または `project_root/resources/py/dispatcher.py` が見つからないと GUI から Python を呼べない
- Python の探索優先度（要点）
  - `project_root/resources/python/python(.exe)`（portable runtime）
  - `project_root/resources/.venv/.../python(.exe)`
  - `project_root/.venv/.../python(.exe)`
  - 最後に `python`（システム依存。配布では使わない）

配布/試用では `dispatcher.py` を `resources/py/` にまとめて配置し、ユーザーが誤って編集/削除するリスクを下げる。

## 期待するフォルダ構成（Windows / portable zip 案）

「解凍して起動」を成立させるための最小構成例。

```
ocr-to-doc/
  ocr-to-doc.exe
  configs/
    settings.json
  resources/
    py/
      dispatcher.py
      ocr_chanked.py
      ocr.py
      postprocess.py
      export_docx.py
      export_excel_poc.py
      ui_preview.py
      configs/
        icon_profiles/
      poppler/
        Library/
          bin/
            pdfinfo.exe ...
    python/
      python.exe
      Lib/
      DLLs/
      Scripts/
```

メモ:
- `resources/py` 配下の Python から見て、`resources/py/poppler/Library/bin` に Poppler がある前提（Python 側は `__file__` 基準で解決するため）。
  - `ocr_chanked.py` は候補として `poppler/win/bin` も探すため、整理する場合はそちらに置いてもよい（現状リポジトリ同梱は `Library/bin`）。
- Poppler は `ocr_chanked.py` 側で PATH を一時的に追加して使うため、ユーザーに PATH 設定は要求しない。
- 出力先 `result/` は `project_root` 配下に作られる想定（既定動作）。

## ランタイムの保存先（将来案）

- `%LOCALAPPDATA%\\ocr-to-doc\\runtime\\<version>\\...`
  - 管理者権限不要で、職場PCでもトラブルが少ない想定

※ ただし現状実装ではこの場所は自動参照しないため、参照ロジック追加（起動時セットアップ実装）が必要。

## 職場PCでの事前試用（GitHub Releases 前）

### ねらい

- ネットワーク制約、権限（書き込み先）、性能（CPU/メモリ）を先に確認する。

### GitHub Actions で portable zip を作る（Windows 環境なしの前提）

本リポジトリには、Windows runner で `tauri build` し、portable 用 zip を作って Artifacts に出す workflow を用意する。

- workflow: `.github/workflows/windows_portable.yml`
- 使い方（概要）:
  - GitHub の Actions から `windows-portable` を `Run workflow` で実行
    - `with_runtime` を ON にすると `resources/python` まで同梱する（サイズ/時間が大きい）
  - 実行後、Artifacts から `ocr-to-doc-portable-windows.zip` をダウンロード
  - 職場PCで zip を展開して起動（必要なら `resources/python` を同梱/配置）

自動化（タグ運用）:
- `v*` タグを push すると自動で workflow が走る（例: `v0.1.0`）
- タグ起動時は runtime 同梱を前提に実行される（`resources/python` を作成して zip に含める）

### 手順（手動コピーでの試用：最短）

1. 職場PCに `ocr-to-doc/` フォルダをコピー（USB/ネットワーク共有など）
2. `ocr-to-doc/resources/python/` に Windows 用ランタイム一式を配置
   - GitHub Releases に置く前は、手元の zip を展開して配置してOK
3. `ocr-to-doc/ocr-to-doc.exe` を起動
4. PDF を入力にして 1〜2 ページで smoke test（OCR→md、可能なら docx）

チェック観点（最低限）:
- PDF が `poppler/Library/bin` を使って画像化できるか
- `result/` への書き込みが許可されるか
- 速度/メモリ（チャンク休憩の必要性）や、lite モードの運用が現実的か

### 手順（参考：CLI での試用）

GUI を使わず、Python 側のみ先に確かめたい場合。

```bash
poetry run python dispatcher.py sample.pdf --formats md docx
```

関連: `docs/env_and_limits.md` / `docs/python_runtime.md`
