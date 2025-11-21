# 実行環境と制約

## 1. 想定環境(職場)

- OS: Windows 11（最終ターゲット）
- CPU: Intel i5-8500 クラス
- メモリ: 16GB
- GPU: なし
- Python: 3.10〜3.12
- Poetry を用いた仮想環境管理
- 外部ツール:
  - Poppler for Windows（プロジェクト内 `poppler/Library/bin` に配置）
  - Pandoc（Word 出力に利用）

## 1.1 開発環境(現在)

- OS: macOS (Apple silicon)
- Python: 3.12（Poetry 管理 `.venv`）
- Poppler: Homebrew 版を使用し、`/opt/homebrew/opt/poppler/bin` を PATH に追加。
- 方針: macOS で開発・検証しつつ、Windows 向けにはリポジトリ同梱のバイナリを利用する。

## 2. YomiToku の利用制約

- 通常モード（フルモデル）は GPU 前提で重く、CPU 環境では BSOD を引き起こすリスクが高い
- このため、本プロジェクトでは **`--lite -d cpu` を前提**とする
- さらに、長時間連続負荷を避けるために:
  - PDF をページ単位で画像化
  - 10 ページ単位のチャンク処理
  - ページごと／チャンクごとのスリープ（数秒〜10秒程度）を挿入

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

## 5. 実装順序（フェーズ別の進め方）

本プロジェクトは、負荷の高い処理を後回しにしつつ、常に「動く状態」を維持するために段階的に実装する。

### フェーズ 1（最優先）
- YomiToku（full / lite 切替）の OCR パイプラインを構築
- PDF → 画像分割 → ページ単位 OCR → Markdown + 図画像生成
- ページ単位 Markdown の結合（merged.md 作成）
- 不要ファイル（layout.jpeg / ocr.jpeg）の自動削除

### エクスポート（フェーズ 1 完了直後に実施）
- Markdown → docx 変換（Pandoc 使用、画像埋め込み対応）
- Markdown or CSV → xlsx 変換（pandas / openpyxl を利用）

※ フェーズ番号は振らないが、フェーズ 1 の自然な続きとして扱う。

### フェーズ 2（入力ファイル自動判定）
- dispatcher の実装（画像PDF / テキストPDF / 画像ファイル）
- テキスト PDF の場合は markitdown / pdfplumber による Markdown 化
- 全ての入力を Markdown に正規化できる段階まで拡張

### フェーズ 3（サブエンジン導入）
- YomiToku で失敗したページに対する fallback（Tesseract / PaddleOCR）
- fallback 採用ページのログ記録
- 必要に応じて、軽量→高精度への再 OCR（auto モード）の実装

## 6. OS 別バイナリの扱い

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

### 出力ディレクトリ構成
- OCR 実行時は `result/<入力ファイル名>/` を自動生成し、ページ Markdown・`figures/fig_page***.png`・結合済み `*_merged.md` を同じフォルダにまとめる。
- `poppler/merged_md.py` を直接使う場合は `--input result/<入力ファイル名>` を指定する。
