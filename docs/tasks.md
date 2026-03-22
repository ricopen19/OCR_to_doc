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

## 未着手（GLM-OCR 移行 / feature/glm-ocr ブランチ）

### Phase 1: Rust + Ollama で OCR 動作

#### 1-1. Rust 基盤整備
- [ ] lib.rs をモジュール分割（ollama/, ocr/, markdown/, export/, job.rs, settings.rs）
- [ ] Cargo.toml に依存追加（reqwest, base64, tokio）

#### 1-2. Ollama クライアント
- [ ] health_check（GET /）
- [ ] list_models / has_model（GET /api/tags）
- [ ] chat_vision（POST /api/chat + base64 画像）
- [ ] pull_model（POST /api/pull + ストリーミング進捗）

#### 1-3. OCR パイプライン
- [ ] PDF → 画像化（pdfium-render を検証、ダメなら Poppler バインディング）
- [ ] 画像 → base64 → Ollama → Markdown の基本フロー
- [ ] page_###.md の保存・命名
- [ ] run_job を Rust OCR パイプラインに切り替え
- [ ] 進捗管理を Rust 内部更新に変更（stdout パース廃止）

#### 1-4. Markdown 処理
- [ ] page_*.md のソート + マージ（# Page n 挿入）
- [ ] markdown_cleanup 相当の整形を Rust で実装

#### 1-5. 図表抽出
- [ ] 案 C（VLM で bbox 検出）の精度検証
- [ ] 案 D（DocLayout-YOLO ONNX + Rust）の導入検証
- [ ] 採用した方式で crop + figures/ 保存を実装

#### 1-6. エクスポート・環境チェック
- [ ] docx エクスポート（Python subprocess 呼び出し）の動作確認
- [ ] check_environment を Ollama 対応に変更

### Phase 2: エクスポート + 校正
- [ ] GLM-OCR JSON → xlsx/csv 変換の対応
- [ ] LLM 校正フローの実装

### Phase 3: セットアップ + GUI
- [ ] 初回セットアップフロー（Ollama 検出 → モデル pull → 起動）
- [ ] Ollama 接続設定の UI
- [ ] 環境チェック画面の刷新

### Phase 4: Python 依存の縮小
- [ ] Python パイプライン（dispatcher/ocr/postprocess）を廃止
- [ ] Python は export のみに縮小
- [ ] 配布パッケージのサイズ検証
