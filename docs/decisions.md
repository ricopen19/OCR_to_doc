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

## ADR-010: 図表抽出に YOLOv8x-DocLayNet を採用

- **日付**: 2026-03-24
- **決定**: glm-ocr の VLM bbox 検出を断念し、YOLOv8x-DocLayNet（Python ultralytics 経由）で図表を検出する
- **理由**: glm-ocr は OCR 専用 VLM であり、bbox 検出プロンプトに対して GGML_ASSERT エラーを返す（1ページ ~3分のタイムアウト）。YOLOv8x-DocLayNet は DocLayNet データセットで学習済みの軽量モデルで、数学教材（幾何図形・グラフ含む）に対して実用的な精度を確認。conf=0.35 + 最小サイズフィルタ 150x100px をデフォルトとする。
- **却下案**: 別 VLM（LLaVA 等）で bbox 検出（数 GB の追加モデルが必要）、Rust + ONNX Runtime（Python がハイブリッド構成で残るため不要な複雑さ）
- **検証データ**: 数的処理_7days P17-28（12ページ）で検証。筆算集中ページ以外はほぼ全図を検出
