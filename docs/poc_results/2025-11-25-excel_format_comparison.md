# 2025-11-25 Excel フォーマット比較メモ

## 対象データ
- PDF: `/Users/tsuchiyaryohei/workspace/OCR_to_doc/応用技術者教科書.pdf`
- ページ: 36（1 ページあたりの情報がほぼ全て表形式）
- 既存成果物: `result/応用技術者教科書_p36-36/応用技術者教科書_p36-36_merged.md`

## Markdown 出力の現状
- `ocr_chanked.py` → YomiToku → `markdown_cleanup.py` の流れで生成。
- `markdown_cleanup.py:327` で `<br>` を改行へ置換しているため、表セル内の改行がそのまま行区切りとして扱われ、22 行目以降で表が崩れる（見出しセルと本文セルが別行扱いになる）。
- 試しに 36 ページをプレビューすると、1 列目の「テクノロジ系」行までは表、2 列目以降は通常段落に見えてしまい、Excel 変換では列幅・セル結合情報が欠落する懸念がある。

### 改善ポイント（Markdownルート）
1. `markdown_cleanup.py` に「表内のみ `<br>` を保持し、段落だけ改行へ変換する」オプションを追加する。
2. マージ済み Markdown からテーブルブロックを抽出する際、`|-|` のパターン行と、その直後の「ヘッダー」行が連続しているかを検証し、崩れている場合は `<br>` 復元→再結合するリペア処理を入れる。

## CSV 出力の仕様（YomiToku）
- 実装: `.venv/lib/python3.12/site-packages/yomitoku/export/export_csv.py`。
- `table_to_csv` が `n_row × n_col` の 2 次元リストを生成し、セル結合は「左上セルのみに内容を入れ、他セルは空白」の形で書き出す。
- 出力ファイルは「表 → 空行 → 段落 → 空行…」という構造で、どの行がどの表に属していたかのメタ情報は入らない。
- Excel 変換では「空行で区切られたブロック」を読み取って Worksheet を分割すれば良い一方、セル結合情報は欠落するため、後段で JSON を参照して補完する必要がある。

## JSON 出力の仕様（YomiToku）
- 実装: `.venv/lib/python3.12/site-packages/yomitoku/export/export_json.py`。
- `inputs.model_dump()` が以下の情報を保持:
  - `tables[].cells[]` に `row`, `col`, `row_span`, `col_span`, `contents`, `box`。
  - `paragraphs[]` に `contents`, `role`, `box`。
  - `figures[]` に座標と段落配列。
- つまりセル結合や段落 / 図の座標を完全に再現できるため、Excel で `merge_cells` を正確に行いたい場合は JSON の参照が最も堅牢。
- 欠点はファイルサイズと目視性。PoC では JSON を Excel 生成ロジックの“真実ソース”とし、可読性優先の Markdown/CSV を補助情報にする方針がよい。

## HTML 出力の仕様（YomiToku）
- 実装: `.venv/lib/python3.12/site-packages/yomitoku/export/export_html.py`。
- `<table>` に `rowspan` / `colspan` を埋め込んだ形で出力。セル内改行は `<br>` へ変換されるため、Markdown のような破綻は発生しない。
- HTML には段落も `<p>` タグで入るので、そのまま BeautifulSoup 等でテーブルのみ抽出することは容易。ブラウザプレビューで確認しやすい点もメリット。
- ただし CSV/JSON と比べると後段パースでのタグ処理コストが高く、Excel に流し込むなら JSON または CSV をベースにする方がシンプル。

## 実行ログ / ボトルネック
- `poetry run yomitoku 応用技術者教科書.pdf --pages 36 --lite -d cpu -f csv -o tmp/yomi_formats/csv` を実行したところ、Hugging Face からモデルをダウンロードできず 120 秒でタイムアウト（オフライン環境）。
- 既存の YomiToku モデルを `.cache/huggingface` などから手動でコピーするか、ローカル LAN 内にミラーを用意しない限り新規フォーマット出力は生成できない。

## 次アクション案
1. ユーザー環境の Hugging Face キャッシュをこのマシンへ同期する（`~/.cache/huggingface/hub` 以下を rsync 等で転送）。
2. 同じコマンドで `-f md/csv/json/html` を順番に実行し、`tmp/yomi_formats/<format>/page_036.*` を取得した後、このメモへ実際のサンプルを追記。
3. Excel PoC の要件シートに「JSON をソース、CSV/Markdown/HTML を補助ビューワ」として利用する旨を明記。

## 追記（2025-11-28）
- `export_excel_poc.py` を拡張し、JSON/CSV/HTML からの xlsx 生成で以下を確認:
  - 結合セルは保持、Table 化は結合がない場合のみ実施して Excel 警告を回避。
  - レビュー用ステータス列（○/×ドロップダウン）を表の右にスペースを空けて追加、Table 範囲は元表のみ。
  - 数値/百分率/日付の自動判定・書式設定をデフォルト ON（`--no-auto-format` で無効化）。
  - メタ情報シートを自動生成（入力パス・フォーマット・生成日時・シートリンク）。
  - CSV では `--csv-tables-only` で段落行を除外して表のみ抽出可能。
- 現状の結論: 精度差は小さいが、カスタマイズ性とセル結合保持の観点で JSON を真実ソース、CSV/HTML は補助とする方針で固める。

## 実装方針メモ（2025-11-25）
- PDF を直接 `yomitoku` へ渡すと Poppler レンダリング + 4 モデル推論が同時に走り、macOS ではメモリ不足で `zsh: killed` になるケースがあった。`ocr_chanked.py` のように **事前に `page_images/*.png` を生成し、PNG を 1 ページずつ YomiToku に渡す** 流れへ統一する。
- フォーマット抽出の推奨フロー:
  1. `ocr_chanked.py` で対象ページを処理し、`result/<name>/page_images/page_###.png` を得る。
  2. `page_###.png` を入力に `yomitoku page_###.png -f csv/json/html -o tmp/yomi_formats/<format>` を実行（PDF を指定しない）。
  3. 1 ページずつ呼ぶと CLI の起動コストが高いので、専用ラッパースクリプトでページリストを一括処理し、必要に応じてチャンクごとにスリープを入れる。
- Excel 生成では JSON を真実ソースとして使い、CSV/HTML/Markdown は確認・比較用に位置付ける。PNG 経由で処理すればフォーマットごとの負荷差が縮まり、安定した比較が可能になる。
