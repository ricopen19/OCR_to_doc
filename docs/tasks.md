# タスク一覧

## 完了

- [x] OCR パイプライン構築（YomiToku + Poppler）
- [x] Markdown マージ + クリーンアップ
- [x] docx エクスポート
- [x] xlsx エクスポート（PoC）
- [x] csv エクスポート
- [x] GUI（Tauri + React）基本実装
- [x] 図表抽出 + アイコンフィルタリング
- [x] Windows portable ビルド（GitHub Actions）
- [x] 数式 OCR 検証 → 見送り（ADR-002）
- [x] ドキュメント整理（CLAUDE.md 方針準拠）

## GLM-OCR 移行（feature/glm-ocr ブランチ）

### Phase 1: Rust + Ollama で OCR 動作

#### 1-1. Rust 基盤整備
- [x] Cargo.toml に依存追加（reqwest, base64, tokio, image, serde_json, regex）
- [x] lib.rs をモジュール分割 — ollama/, ocr/, markdown/, settings.rs, job.rs, paths.rs, cli.rs, results.rs, environment.rs 済（2,394→1,230行）。残存: run_job/run_job_ollama（Phase 4 で廃止予定のため見送り）

#### 1-2. Ollama クライアント
- [x] health_check（GET /）
- [x] list_models / has_model（GET /api/tags）
- [x] chat_vision（POST /api/chat + base64 画像）
- [x] pull_model（POST /api/pull、非ストリーミング）
- [x] chat_text（LLM 校正用テキストチャット）

#### 1-3. OCR パイプライン
- [x] PDF → 画像化（Poppler pdftoppm 方式で動作）
- [x] 画像 → base64 → Ollama → Markdown の基本フロー
- [x] page_###.md の保存・命名
- [x] run_job を Rust OCR パイプラインに切り替え（run_job_ollama コマンド）
- [x] 進捗管理を Rust 内部更新に変更（ProgressCallback で直接 JobInfo 更新）

#### 1-4. Markdown 処理
- [x] page_*.md のソート + マージ（# Page n 挿入）
- [x] basic_cleanup 実装（GLM-OCR では最小限で十分、方針確定済み）

#### 1-5. 図表抽出
- [x] 案 C（VLM で bbox 検出）実装・検証 → glm-ocr は bbox 非対応で却下
- [x] YOLOv8x-DocLayNet で検証 → 数学教材で良好な精度（conf=0.35 で実用的）
- [x] YOLOv8x-DocLayNet を OCR パイプラインに組み込み（Python スクリプト + Rust 呼び出し）

#### 1-6. エクスポート・環境チェック
- [x] check_environment を Ollama 対応に変更（ollama_running, ocr_model_ready）
- [x] docx エクスポート（Python subprocess 呼び出し）の E2E 動作確認

### Phase 2: エクスポート + 校正
- [x] Markdown テーブル → xlsx エクスポート（load_tables_from_markdown 追加）
- [x] run_job_ollama から xlsx エクスポートを呼び出し
- [x] csv エクスポート対応（--csv-dir オプション、xlsx と同時出力可能）
- [x] docx エクスポートの E2E 動作確認
- [ ] LLM 校正フローの実装（glm-ocr の精度が高いためオプション扱い。必要に応じて実装）

### Phase 3: セットアップ + GUI
- [ ] 初回セットアップフロー（Ollama 検出 → モデル pull → 起動）
- [ ] Ollama 接続設定の UI
- [ ] 環境チェック画面の刷新

### Phase 4: Python 依存の縮小
- [ ] Python パイプライン（dispatcher/ocr/postprocess）を廃止
- [ ] Python は export のみに縮小
- [ ] 配布パッケージのサイズ検証

## Mac 対応（進行中）

ADR-011 の保留方針を撤回。Ollama バージョンダウンで glm-ocr が M5 Metal で動作確認済み。DMG 配布を開始。

- [x] macOS 環境チェック（Python3・Poppler の検出を macOS に対応）
- [x] PDF ページ単位変換（pdftoppm 1ページずつ、CPU バースト分散）
- [x] デフォルト DPI 150（リサイズ処理スキップ、44% ピクセル数削減）
- [x] Ollama モデル自動アンロード（keep_alive 3分 + アプリ終了時即時解放）
- [x] ページ間休止 UI（デフォルト ON / 5秒）
- [x] 図表抽出デフォルト OFF
- [x] DMG ビルド（aarch64）
- [ ] 環境チェック画面で Python / Poppler の NG 表示が残っている（動作はしているが表示上の問題）
- [ ] Python プロセスの孤児化対策（AppState で PID 追跡 + 終了時 kill）

## デッドコード整理・機能ギャップ修正（Ollama 移行の取りこぼし対応）

- [x] Rust 側デッドコード削除。GUI から到達不能だった `run_job`（旧 Python subprocess 経路、535行）と `invoke_handler` 登録を削除。派生して orphan 化した `default_gpu_device` / `collect_output_files`、および `cargo build` が検出していた未使用の Ollama クライアントメソッド・型フィールド・関数（`with_base_url` / `chat_text` / `pull_model` / `PullRequest` / `PullProgress` / 未読フィールド群 / `pdf_to_page_images_range` / `basic_cleanup`）を削除
  → 検証: `cargo build`（warning 17→0件）、`cargo test --lib`（19 passed, 2 ignored）で確認済み
- [x] Phase1/Phase2 移行（YomiToku Python 版 → Ollama Rust 版）で欠落していた RunOptions の機能ギャップを解消。`crop`（per-file トリミング）を `OcrOptions` に追加し OCR 前にページ画像を実際に切り出すよう修正（`crop_page_image_to_temp`、`ui_preview.py` の `apply_crop` と同じ正規化座標仕様）。`excelMode` / `excelMetaSheet` を `export_excel_poc.py` 呼び出しに `--excel-mode` / `--meta` / `--no-meta` として渡すよう修正。YomiToku 専用で glm-ocr パイプラインには適用先がなかった `useGpu` / `imageAsPdf` / `chunkSize` / `mode`（lite/full）と、CLI 引数化されておらず渡しようがなかった `excelSymbolFallback` は型・UI ごと削除（Rust: `job.rs` / `settings.rs`、フロント: `api/runJob.ts` / `api/settings.ts` / `App.tsx` / `pages/RunJob.tsx` / `pages/Settings.tsx`）
  → 検証: `cargo build` / `cargo test --lib`（19 passed, 2 ignored）、フロント `npx tsc --noEmit`・`npm run build` で確認済み。実機での「トリミング適用後のOCR結果」「Excel出力モード切替」の目視確認は未実施

## Unlimited OCR 表対応

- [x] ハイブリッド表再OCR（Unlimited OCR の table bbox で切り出し → glm-ocr で再OCR）+ GUI トグル `enableTableReocr`（デフォルト OFF）。経緯: docs/decisions.md ADR-013
  → 検証: 統合テスト `table_reocr_end_to_end`（opt-in）で OFF=平坦テキスト保持／ON=11行×6列 Markdown テーブル復元を確認済み。GUI トグルの目視確認のみ未実施
- [x] `extract_table_markdown`（ocr/pipeline.rs）のフォールバック漏れを修正。フェンス/パイプ表のどちらも見つからず生 HTML の場合は `html_table_to_markdown()` を通すよう修正
  → 検証: `extract_table_markdown_converts_raw_html_fallback_to_markdown` で `<table` タグが残らないことを確認済み
- [x] `enableTableReocr` ON 時に、ページごとの Unlimited OCR ⇔ glm-ocr モデル入れ替えで処理が実質停止する不具合の根本対策。Phase1/Phase2 方式に刷新: 全ページを Unlimited OCR のみで処理し、検出した表領域は画像を切り出して `PendingTable` として保留（`stage_table_placeholders`、Ollama 呼び出しなし）。全ページ処理後に一括で glm-ocr に切り替え、保留した表をまとめて再OCR（`resolve_pending_tables`）。モデル入れ替えはジョブ全体で最大1回に集約され、ページ単位の頻繁な入れ替えは撤廃。あわせて `truncate_thought_leak`（chain-of-thought 漏れのマーカー検出）と `presence_penalty` を追加し、ループ・ハルシネーション対策を強化。`keep_alive` は co-residency 実験（30m 延長）が不要になったため `3m` に復元
  → 検証: `cargo test --lib`（19 passed, 2 ignored）、`cargo build`（warning のみ、エラーなし）で確認済み。実機での `enableTableReocr` ON 通し実行（表を含む複数ページ）による途中停止・クラッシュなしの確認は未実施
- [x] `html_table_to_markdown`（ocr/pipeline.rs）が `<tr>` を無視し表全体を1行に平坦化していた不具合を修正。`<tr>` ごとに行を分割して複数行 Markdown テーブルとして復元するよう書き換え。`<tr>` を含まない不正 HTML は従来通り単一行にフォールバック。行ごとのセル数が不揃いな場合は最大列数に合わせて空セルで埋める。あわせて、空セルを詰めて捨てていた既存挙動（後続セルが左にずれ列がずれる原因）をやめ、空セルも列位置として保持するよう修正。rowspan/colspan（結合セル）の復元は別タスク
  → 検証: `cargo test --lib`（22 passed, 2 ignored、多行復元／セル数不揃いのパディング／空セル保持の新規テスト3件を含む）で確認済み。実機での glm-ocr 出力を通した動作確認は未実施
