# アーキテクチャ / モジュール構成

## 1. 全体像

このプロジェクトは、以下のモジュールで構成することを想定する。

- `ingest.py` : 入力ファイル（画像 / PDF）の受け取りとページ単位への分解
- `ocr.py` : YomiToku を用いた OCR 実行
- `postprocess.py` : ページ単位 Markdown の結合・整形
- `figures.py` : 図表画像の抽出と命名
- `export.py` : Markdown から Word / Excel への変換

現在はスクリプト群（`ocr_per_page.py`, `ocr_chunked.py`, `merge_md.py`, `export_docx.py`）として分かれており、今後これらをモジュール化する方針。

## 2. OCR モジュール方針

- YomiToku の CLI をラップする形で利用
- CPU 環境を前提とし、`--lite -d cpu` を基本設定とする
- PDF → ページ画像 → YomiToku（ファイル単位）というフローを維持
- 長時間連続負荷による BSOD 回避のため、「チャンク処理（例: 10ページ単位）」と「スリープ」を組み合わせる

## 3. 図表抽出の方針

- 第一段階：
  - Poppler（`pdftoppm` 等）を使って PDF から画像だけを抽出
  - ページ番号と画像番号をもとに命名規則を決める（例: `page-001-fig-01.png`）
- 第二段階（余力があれば）:
  - YomiToku の `--figure` 機能を 1 ページずつ慎重に検証
  - レイアウト情報と組み合わせて、Markdown 内のより適切な位置に画像を挿入する

## 4. エクスポート（Word / Excel）

- Word:
  - 当面は Pandoc を利用して `merged.md → .docx` に変換
  - 将来的に `python-docx` ベースの純 Python 実装も検討可能

- Excel:
  - 初期実装：表っぽいテキストブロックを抽出して CSV / .xlsx に出力
  - 将来的には YomiToku のテーブル構造出力（もしあれば）を利用し、セル構造を忠実に反映させる
