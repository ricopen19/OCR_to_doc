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
- [ ] lib.rs をモジュール分割 — ollama/, ocr/, markdown/ 済。job.rs, settings.rs, export/ が lib.rs に残存

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
- [x] 案 C（VLM で bbox 検出）実装済み — 精度の実地検証は未実施
- [x] crop + figures/ 保存を実装
- [ ] 案 C の精度を実データで検証し、不十分なら案 D（DocLayout-YOLO ONNX）を検討

#### 1-6. エクスポート・環境チェック
- [x] check_environment を Ollama 対応に変更（ollama_running, ocr_model_ready）
- [x] docx エクスポート（Python subprocess 呼び出し）の E2E 動作確認

### Phase 2: エクスポート + 校正
- [x] Markdown テーブル → xlsx エクスポート（load_tables_from_markdown 追加）
- [x] run_job_ollama から xlsx エクスポートを呼び出し
- [x] csv エクスポート対応（--csv-dir オプション、xlsx と同時出力可能）
- [x] docx エクスポートの E2E 動作確認
- [ ] LLM 校正フローの実装（OCR 精度次第でオプション扱い）

### Phase 3: セットアップ + GUI
- [ ] 初回セットアップフロー（Ollama 検出 → モデル pull → 起動）
- [ ] Ollama 接続設定の UI
- [ ] 環境チェック画面の刷新

### Phase 4: Python 依存の縮小
- [ ] Python パイプライン（dispatcher/ocr/postprocess）を廃止
- [ ] Python は export のみに縮小
- [ ] 配布パッケージのサイズ検証
