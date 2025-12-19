# 実行環境と制約

確定した挙動（入出力/既定値/成果物）は `docs/spec.md` に集約しています。ここでは「環境」と「制約の背景」に絞ります。

## 1. 想定環境(職場)

- OS: Windows 11（最終ターゲット）
- CPU: Intel i5-8500 クラス
- メモリ: 16GB
- GPU: なし
- Python: 3.10〜3.12
- Poetry を用いた仮想環境管理
- 外部ツール:
  - Poppler for Windows（プロジェクト内 `poppler/Library/bin` に配置）
  - 必要に応じて Pandoc（補助用途）。Word 出力の既定は `python-docx` 実装。

## 1.1 開発環境(現在)

- OS: macOS (Apple silicon)
- Python: 3.12（Poetry 管理 `.venv`）
- Poppler: Homebrew 版を使用し、`/opt/homebrew/opt/poppler/bin` を PATH に追加。
- 方針: macOS で開発・検証しつつ、Windows 向けにはリポジトリ同梱のバイナリを利用する。

## 2. OCR パイプラインと利用制約

CPU 環境で安定運用するため、基本は lite モードを前提に設計しています。フルモードは負荷が高く、職場 PC 相当の環境ではトラブル（BSOD 等）につながる可能性があります。

### 2.1 入力フォーマットの扱い
入力サポートの一覧や変換後の配置は `docs/spec.md` を参照してください。

## 3. 既知の問題と対策

- 現象:
  - `MEMORY_MANAGEMENT (0x1A)` や `SYSTEM_SERVICE_EXCEPTION (0x3B)` などの BSOD が発生したことがある
  - フルPDFを一括処理したときに発生しやすい
- 対策:
  - lite モードに限定
  - チャンク処理＋スリープを導入
  - 必要に応じて DPI を 150 程度に抑えて画像サイズを小さくする

## 4. 将来の改善余地

- GPU 搭載マシンでのフルモード検証
- クラウド OCR（Azure / Google / ABBYY など）とのハイブリッド運用
- テーブル構造の再現性向上（特に Excel 出力に向けて）
- GitHub Actions (Windows runner) での自動テスト整備（後日対応）

## 5. OS 別バイナリの扱い

- Windows 版 Poppler を `poppler/Library/bin` に同梱し、Windows 実行時はこのディレクトリを PATH に追加する。
- macOS では Homebrew 版 Poppler を利用し、`POPPLER_PATH` で `/opt/homebrew/opt/poppler/bin` を指定するか、起動スクリプトから PATH を通す。
- 将来的に GitHub Actions (Windows) で smoke test を走らせ、Windows バイナリが破損していないかチェックする（当面は手動確認）。

### macOS での Poppler 準備手順
1. `brew install poppler`
2. `ocr_chanked.py` が `sys.platform == "darwin"` の場合、以下の優先順でパスを解決する：
   - `poppler/macos/bin`（手動で配置した場合）
   - `/opt/homebrew/opt/poppler/bin`
   - `/usr/local/opt/poppler/bin`
3. 上記いずれかが存在しない場合はエラーになるので、必要に応じて `poppler/macos/bin` にシンボリックリンクを置くか PATH を調整する。

出力ディレクトリ構成は `docs/spec.md` を参照してください。
