# Python 実行環境の同梱方針（暫定）

当面は「仮想環境 (.venv) を同梱する」運用で進める。UI/機能が固まったら、最終的に embeddable Python へ移行して容量を抑える。

## 1. 同梱 .venv の作り方（開発マシン）
```bash
poetry install            # ルートで依存を入れる
poetry env info -p        # 仮想環境のパスを確認
```
確認した .venv をリポジトリ直下にコピー（例: `./.venv`）。Windows/macOS 両対応のため、コピーは必ず Python 本体と site-packages を含める。

## 2. Tauri からの実行パス解決
`src-tauri/src/lib.rs` の `resolve_python_bin` で、以下の優先順位で python を探す:
1. 環境変数 `PYTHON_BIN`
2. `project_root/resources/.venv/(Scripts|bin)/python`
3. `project_root/.venv/(Scripts|bin)/python`
4. 最後のフォールバック: `python`

配布時は `resources/.venv` に同梱しておけば、追加設定なしで動く。

## 3. 配布サイズの目安
- 同梱 .venv: Python本体 + 依存を含め 40〜80MB が多い。100MB 目標なら削減が必要。  
- 後で embeddable Python に移行し、必要パッケージのみ事前展開することで 20〜40MB まで圧縮可能。

## 4. 次のステップ（後で実施）
- `poetry export --without-hashes -o requirements.txt` を用意し、ビルドスクリプトで embeddable にインストールするパッケージリストを固定化。  
- Windows用に embeddable 配布を `resources/python/` に展開し、`PYTHON_BIN` をその `python.exe` に向ける。  
- macOS/Linux 向けは同梱 .venv か、pyenv/uv でローカルPythonにフォールバックできるようにする。
