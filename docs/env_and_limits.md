# 実行環境と制約

## 1. 想定環境(職場)

- OS: Windows 11
- CPU: Intel i5-8500 クラス
- メモリ: 16GB
- GPU: なし
- Python: 3.10〜3.12
- Poetry を用いた仮想環境管理
- 外部ツール:
  - Poppler for Windows（プロジェクト内 `poppler/Library/bin` に配置）
  - Pandoc（Word 出力に利用）

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

## 5. ????????

- `results_pages/` ?? `page_001.md` ?????????????????? Markdown ?????????
- `ocr_per_page.py` / `ocr_chanked.py` ????? `poppler/merged_md.py` ????????? PDF ???? `<PDF?????>_merged.md` ???????
- ?????????????? Markdown ??????????????? `python poppler/merged_md.py --keep-pages --base-name <??>` ??????????
