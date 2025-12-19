# 要件定義シート（Excel 表出力 PoC）

## 1. 基本情報
- **機能名 / バージョン**: Excel 表出力 PoC v0.1
- **作成日 / 作成者**: 2025-11-25 Codex（草案）

## 2. 目的・背景
- Markdown ベースのドキュメントを Word で整えるだけでなく、表主体のページを Excel でも再利用できるようにしたい。
- 既存パイプラインでは `ocr_chanked.py` → Markdown → docx まで整備済み。Excel 変換は未整備のため、PoC で「表構造の再現性」「セル結合・数式の維持」が満たせるか確認する。
- 将来的な GUI では Word/Excel の両方へ出力できることが成功指標（`docs/context_engineering.md:4`）。

## 3. 成功条件（定量）
- **セル構造**: JSON に記載された `row_span` / `col_span` を 100% 反映し、Excel 上で手動調整不要な状態を目指す。最低合格ラインは 95%。
- **セル種別**: 数値／パーセント／テキストの自動判定。誤判定率 5% 未満。
- **処理時間**: 1 ページ（1 table sheet）あたり 40 秒以内（CPU only）。
- **安定性**: CLI 1 回で 10 ページ連続処理してもクラッシュしない（PNG 入力 + チャンク処理で担保）。

## 4. 入力データ・制約
- 対象: `/Users/tsuchiyaryohei/workspace/OCR_to_doc/応用技術者教科書.pdf` の表主体ページ（例: p36）。
- 事前に `ocr_chanked.py --start 36 --end 36` で `page_images/page_036.png` を生成済み。
- `yomitoku page_036.png -f json/csv/html` で取得した各フォーマットを比較。JSON を真実ソース、CSV/HTML は補助データ。
- オフライン＆ローカル完結。Hugging Face モデルは `~/.cache/huggingface/hub` に事前配置。

## 5. リスク・懸念事項
- JSON から Excel へ変換する際、複合セルや縦書きなど特殊レイアウトが崩れるリスク。
- YomiToku JSON が巨大（数 MB）になると変換処理が重くなる可能性。
- openpyxl での画像挿入やセル結合が複雑になり、将来 GUI での編集性に影響する懸念。
- CSV/HTML と JSON の差異管理をどう自動化するか未決定。

## 6. PoC 計画
- **検証内容**: (a) PNG → YomiToku JSON → openpyxl でセル結合付きの表を再現できるか、(b) 同一データを CSV/HTML ベースでも再現できるか比較する。
- **手順**:
  1. `tmp/yomi_formats/json/page_036.json` を入力に簡易スクリプトで Excel を生成。
  2. 生成された xlsx を目視で Markdown 版と比較（セル結合・行数・列幅）。
  3. CSV/HTML から生成した場合と差分を記録し、JSON 採用の妥当性を確認。
  4. 実行ログ・処理時間を `docs/poc_results/` に追記。
- **担当 / 期限**: t_ryohei（レビュー: Codex）、2025-11-27 までに完了目標。

## 7. 実装判断
- PoC 実施結果: JSON/HTML/CSV いずれも表再現は可能だが、セル結合保持・カスタマイズ性から JSON を真実ソースとする方針で Go。CSV/HTML は補助。
- 実装済み: `export_excel_poc.py` による xlsx 生成（結合セル保持、メタシート、数値/日付/百分率の自動書式、レビュー用ステータス列オプション、CSV の表のみ抽出オプション）。
- 残課題: テーマ/スタイル切替、画像埋め込みは優先度低で後続タスクへ回す。

## 8. メモ / 次アクション
- `tmp/yomi_formats/` に揃った CSV/JSON/HTML をバージョン管理対象外にしつつ、サンプルをドキュメントへ引用する仕組みを検討。
- Excel 生成 CLI（仮: `export_excel.py`）では、複数ページを処理する際も PNG 入力→フォーマット抽出→Excel のステップを統一し、`ocr_chanked.py` と同じチャンク制御を導入する。
- PoC 成果を `docs/poc_results/2025-11-25-excel_format_comparison.md` に追記し、Go/No-Go の判断材料を残す。
