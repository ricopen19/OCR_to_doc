# 設計判断の記録（軽量 ADR）

## ADR-001: OCR エンジンに YomiToku を採用

- **日付**: 2025-11
- **決定**: YomiToku を OCR エンジンとして採用
- **理由**: 日本語 OCR 精度が高く、Markdown / JSON / CSV の複数出力に対応。図表抽出機能あり。ローカル完結で動作。
- **却下案**: Tesseract（日本語精度が不十分）、クラウド OCR（ローカル完結の要件に反する）
- **備考**: pytesseract はフォールバック用途で併用

## ADR-002: 数式 OCR（LaTeX → Word 数式）を見送り

- **日付**: 2025-11
- **決定**: PDF → 画像 → Markdown における LaTeX 数式変換は見送り、プレーンテキストで対応
- **理由**: pix2text による LaTeX 変換精度が低い（特に分数式）。PoC で成功条件を満たせなかった。
- **却下案**: pix2text による自動変換

## ADR-003: ローカル完結・クラウド送信なし

- **日付**: 2025-11
- **決定**: すべての処理をローカル PC で完結させる
- **理由**: 教育現場の文書を扱うためセキュリティ上クラウド送信は避ける。職場 PC のネットワーク制約も考慮。

## ADR-004: GUI に Tauri + React を採用

- **日付**: 2025-11
- **決定**: GUI フレームワークとして Tauri（Rust） + React を採用
- **理由**: 軽量バイナリで配布しやすい。Web 技術で UI を構築でき開発効率が高い。

## ADR-005: CPU 前提の設計（lite モード推奨）

- **日付**: 2025-11
- **決定**: 既定は CPU + lite モードで運用
- **理由**: 配布先の職場 PC（i5-8500 / 16GB / GPU なし）でフルモード実行時に BSOD（MEMORY_MANAGEMENT 等）が発生した実績がある。チャンク処理 + スリープで安定化。

## ADR-006: OCR エンジンを GLM-OCR (Ollama) に移行

- **日付**: 2026-03
- **決定**: YomiToku → GLM-OCR に移行。Ollama 経由でローカル実行。
- **理由**: YomiToku は無償利用が個人用途のみ（商用・組織利用は別途ライセンスが必要。参考: https://kotaro-kinoshita.github.io/yomitoku/commercial_use_guideline/ ）。加えて torch 依存で配布が困難（ランタイム同梱で 100MB 超）。GLM-OCR は MIT ライセンスで配布制約なし、0.9B と軽量で Ollama 対応、`ollama pull` だけで導入可能。日本語精度も十分（検証済み）。
- **却下案**: クラウド OCR（ローカル完結に反する）、glmocr パッケージ直接利用（Python 依存が増える）

## ADR-007: ハイブリッド構成（Rust + Python）

- **日付**: 2026-03
- **決定**: OCR / MD 処理 / セットアップは Rust（Tauri）に移行し、エクスポート（docx/xlsx）は Python を維持
- **理由**: Rust 統一は docx 変換ライブラリ（docx-rs）の成熟度不足がボトルネック。xlsx は rust_xlsxwriter で対応可能だが、python-docx との機能差が大きい。段階的に Python 依存を縮小する方針。
- **却下案**: 全 Rust 統一（docx-rs の機能不足）、全 Python 維持（配布の手軽さが改善しない）

## ADR-008: 初回セットアップの自動化

- **日付**: 2026-03
- **決定**: Ollama のプロセス起動・モデル取得をアプリ初回起動時に自動実行。Ollama 本体のインストールのみユーザー操作。
- **理由**: ターゲットユーザー（教職員）はターミナル操作に不慣れ。GUI 完結のセットアップが必須。

## ADR-009: lib.rs モジュール分割の方針

- **日付**: 2026-03
- **決定**: lib.rs（2,400行→2,161行）を段階的にモジュール分割する。Phase 4（Python 廃止）で不要になる `run_job()` は分割対象外とし、長期的に残るコードを優先。
- **理由**: 設計を意識せず機能追加を重ねた結果、1ファイルに8カテゴリの責務が混在。最大の関数 `run_job()` は533行で、引数組み立て・subprocess管理・stdoutパース・進捗計算・ファイル収集が全て1関数に入っている。

### 肥大化の内訳

| カテゴリ | 行数 | 主な問題 |
|---|---|---|
| CLI + Self-Test | ~290 | CLI/self-test の2モードが1関数に混在 |
| パス解決 | ~213 | 14関数がファイル全体に散在、プラットフォーム分岐の重複 |
| ジョブ実行 | ~733 | `run_job()` 533行、`run_job_ollama()` 200行。最大の問題 |
| ファイル操作 | ~152 | open_output 系でパス解決が重複 |
| 結果一覧 | ~174 | pick_best_file_in_dir のファイル拡張子優先度がハードコード |
| ユーティリティ | ~211 | check_environment 94行にプラットフォーム分岐が集中 |
| 出力ファイル検索 | ~175 | collect_output_files の命名規則がハードコード |

### 分割方針（優先度順）

1. **済**: settings.rs（AppSettings + load/save）、job.rs（型定義）
2. **高**: paths.rs（resolve_* 系14関数、~213行）
3. **高**: cli.rs（run_cli_if_requested、~290行）
4. **中**: results.rs（結果一覧・ファイル探索、~174行）
5. **中**: environment.rs（check_environment、~94行）
6. **見送り**: run_job() の内部分解（Phase 4 で Python パイプライン自体を廃止予定のため）

- **却下案**: run_job() を job_manager / python_subprocess / progress_parser に分解（Phase 4 で廃止予定のコードに対してリファクタリングコストが見合わない）

## ADR-011: 配布対象を当面 Windows のみとする（Mac 対応は保留）

- **日付**: 2026-04-19
- **決定**: 配布対象は Windows のみ。Mac 版は作らず、将来 (a) Ollama の M5 Metal 修正を待つ、または (b) MLX 版 glm-ocr に差し替える、のどちらかで再開する
- **理由**: Apple M5 では Ollama (llama.cpp Metal バックエンド) が GGML_ASSERT で異常終了し、glm-ocr がロードできない。OS 分岐で MLX 経路を用意する案もあるが、(1) 実務利用は Windows シェアが圧倒的、(2) Mac 経路を足すと pipeline.rs のバッチ化・進捗 JSONL・環境チェック分岐・mlx-vlm 配布方法の検討が必要でコスト大、のため当面見送り
- **却下案**:
  - OS 分岐で Mac も MLX 対応（コスト対効果が合わない。Win シェアが圧倒的）
  - Ollama に CPU-only で動かす（速度が実用域外）
- **影響**:
  - 開発者（M5 MBA）はローカルで OCR E2E 検証ができない。Windows GitHub Actions の `windows_portable.yml` または Win 実機で検証する
  - Python 側（docx/xlsx export・markdown cleanup）は Mac でも開発・単体テスト可能
- **参考**: Ollama issue #15541 / MLX PoC は `/tmp/glm-ocr-poc` で 112 tok/s 動作確認済み

## ADR-012: macOS 対応を再開・MBA 発熱対策を実装

- **日付**: 2026-05
- **決定**: ADR-011 の保留方針を撤回し、macOS (Apple Silicon) 向け DMG 配布を開始する。発熱対策として以下を実装した。
  1. **PDF ページ単位変換**: pdftoppm で全ページを一括変換するのをやめ、1ページずつ変換→OCR→削除のサイクルに変更。CPU バーストを分散。
  2. **リサイズフィルタ変更**: 画像スケーリングを `Lanczos3` → `Triangle` に変更。OCR 精度への影響は無視できる範囲で CPU 負荷を削減。
  3. **デフォルト DPI を 200 → 150 に変更**: A4 縦 150 DPI = 1240×1754px。`MAX_IMAGE_DIMENSION=1792` 以内に収まり、リサイズ処理自体がスキップされる。ピクセル数は 200 DPI 比で 44% 減。
  4. **ページ間休止 UI**: 1ページごとに CPU を休ませる待機時間をオプションで設定可能（デフォルト ON / 5秒）。
  5. **Ollama モデルの自動アンロード**: `keep_alive: "3m"` を全 OCR リクエストに付与（最終使用から 3 分後に自動解放）。アプリ終了時は `CloseRequested` イベントで `unload_model()` を呼び即時解放。
  6. **図表抽出デフォルト OFF**: YOLOv8x-DocLayNet は CPU 負荷が高いため、MBA 環境での通常利用を考慮しデフォルトを OFF に変更。
- **理由**: Ollama バージョンを下げることで glm-ocr が M5 Metal で動作するようになった（ADR-011 の前提が変化）。ファンレス MBA での実運用で発熱が深刻だったため段階的に対策を追加。
- **却下案**:
  - `nice -n 10` で pdftoppm の優先度を下げる（CPU 時間は変わらず発熱量に差がない）
  - DPI をさらに下げる（100 DPI は細字フォントで精度劣化のリスク）
- **影響**: 150 DPI はデフォルトであり設定 UI から変更可能（省エネ 150 / 標準 200 / 高精細 300）

## ADR-010: 図表抽出に YOLOv8x-DocLayNet を採用

- **日付**: 2026-03-24
- **決定**: glm-ocr の VLM bbox 検出を断念し、YOLOv8x-DocLayNet（Python ultralytics 経由）で図表を検出する
- **理由**: glm-ocr は OCR 専用 VLM であり、bbox 検出プロンプトに対して GGML_ASSERT エラーを返す（1ページ ~3分のタイムアウト）。YOLOv8x-DocLayNet は DocLayNet データセットで学習済みの軽量モデルで、数学教材（幾何図形・グラフ含む）に対して実用的な精度を確認。conf=0.35 + 最小サイズフィルタ 150x100px をデフォルトとする。
- **却下案**: 別 VLM（LLaVA 等）で bbox 検出（数 GB の追加モデルが必要）、Rust + ONNX Runtime（Python がハイブリッド構成で残るため不要な複雑さ）
- **検証データ**: 数的処理_7days P17-28（12ページ）で検証。筆算集中ページ以外はほぼ全図を検出

## ADR-013: 表はハイブリッド再OCR（Unlimited OCR + glm-ocr）

2026-07

**決定**: Unlimited OCR の表出力は使わず、表領域だけ glm-ocr で再OCR する。GUI トグル `enableTableReocr`（デフォルト OFF）で切り替え可能とし、OFF・glm-ocr 不在・失敗時はセル内容を平坦テキストで出力する。

**理由**:
- Unlimited OCR は表を `<table>` 内のタグなし連結テキストとして出力する（学習仕様。3種のプロンプトで検証、"Treat all tabular layout as plain text with spacing." のエコーを確認）。プロンプトでは修正不可能で、行・列の復元も原理的に不可能
- 一方で `table [x1,y1,x2,y2]`（0-1000 正規化）の座標は正確なため、切り出し → glm-ocr 再OCR で表構造をほぼ完全に復元できる（実測: 11行×6列を約15秒）
- デフォルト OFF の理由: 低メモリ PC では 2 モデルの入れ替えロードが重い。実測でも glm-ocr のコールドロードが 60 秒を超えたため、再OCR タイムアウトは 300 秒に設定

**却下案**:
- HTML→Markdown の行パース改善: `<tr>`/`<td>` がそもそも出力されないため不成立
- 表対応モデルへの全面乗り換え: Unlimited OCR の速度メリットを失う
