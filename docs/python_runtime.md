# Python 実行環境の同梱方針（暫定）

当面は「仮想環境 (.venv) を同梱する」運用で進める。UI/機能が固まったら、最終的に embeddable Python へ移行して容量を抑える。

## 0. 配布要件（前提）

- ユーザーに PATH 設定や事前インストールを求めない（ダウンロードしてすぐ使える）。
- Teams チャット添付の上限（100MB）を意識し、配布物は 100MB 以下を目標にする。
- 初回のみネット接続は利用可能（職場PCの通常環境を想定）。

## 1. 同梱 .venv の作り方（開発マシン）
```bash
poetry install            # ルートで依存を入れる
poetry env info -p        # 仮想環境のパスを確認
```
確認した .venv をリポジトリ直下に配置（例: `./.venv`）。  
注意: `.venv` は OS/アーキテクチャ依存のため、**配布用に同梱する場合は配布対象 OS 上で作ったもの**を同梱します（Windows 用 `.venv` を macOS からそのままコピーしても動きません）。

## 2. Tauri からの実行パス解決
`ui/src-tauri/src/lib.rs` の `resolve_python_bin` で、以下の優先順位で python を探す:
1. 環境変数 `PYTHON_BIN`
2. `project_root/resources/python/python.exe`（Windows / portable runtime）
3. `project_root/resources/python/bin/python`（macOS/Linux / portable runtime）
4. `project_root/resources/.venv/(Scripts|bin)/python(.exe)`（互換）
5. `project_root/.venv/(Scripts|bin)/python(.exe)`
6. 最後のフォールバック: `python`

配布時は `resources/python` に同梱しておけば、追加設定なしで動く（互換として `resources/.venv` も探索する）。

補足:
- Python スクリプト（`dispatcher.py` 等）は `project_root/resources/py/` を優先して探索し、見つからない場合は `project_root/` 直下を参照します（`resolve_python_entry`）。

## 3. 配布サイズの目安
- 同梱 .venv: Python本体 + 依存を含め 40〜80MB が多い。100MB 目標なら削減が必要。  
- 後で embeddable Python に移行し、必要パッケージのみ事前展開することで 20〜40MB まで圧縮可能。

## 4. 次のステップ（後で実施）
- `poetry export --without-hashes -o requirements.txt` を用意し、ビルドスクリプトで embeddable にインストールするパッケージリストを固定化。  
- Windows用に embeddable 配布を `resources/python/` に展開し、`PYTHON_BIN` をその `python.exe` に向ける。  
- macOS/Linux 向けは同梱 .venv か、pyenv/uv でローカルPythonにフォールバックできるようにする。

## 5. Teams 100MB 対策：配布物の分割（暫定）

現状の依存（例: `yomitoku/torch`）は `.venv` 同梱だと 100MB を大きく超えやすい。  
そのため「アプリ本体」と「Python ランタイム」を分け、初回のみランタイムを取得する方針を採る。

- アプリ本体（≤100MB目標）
  - Tauri のビルド成果物
  - `poppler/`（PDF 変換に必須。こちらは同梱する）
  - Python スクリプト群（`dispatcher.py` など）
  - Python 実行環境は含めない
- Python ランタイム（初回のみ取得 / 手動コピーでも可）
  - Windows 向けの Python 実行環境 + 依存（`.venv` もしくは embeddable Python + 展開済み site-packages）
  - 配布先は GitHub Releases を第一候補

※ この「初回自動取得」は実装タスク（UI 側の起動時セットアップ）になるため、ここでは方針のみ定義する。

## 6. 職場PCでの事前試用（GitHub Releases 前）

GitHub Releases に置く前段として、職場PCへ「アプリ本体」と「ランタイム」をコピーして試用する。

- 目的: ネットワーク制約や権限、性能（CPU/メモリ）で問題がないか先に確認する
- 進め方:
  - まずは現状実装どおり `resources/.venv` を用意して動作確認（最短）
  - その後、配布方針（ランタイム分割）に合わせて「ランタイムzipを所定場所へ展開 → 起動」で試用する
