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
- [ ] Rust から Ollama API で GLM-OCR を呼び出す
- [ ] PDF → 画像化を Rust で実装
- [ ] Markdown 出力 + マージを Rust で実装
- [ ] 図表抽出（DocLayout-YOLO ONNX or VLM 検証）
- [ ] docx エクスポート（Python 呼び出し維持）の動作確認

### Phase 2: エクスポート + 校正
- [ ] GLM-OCR JSON → xlsx/csv 変換の対応
- [ ] LLM 校正フローの実装

### Phase 3: セットアップ + GUI
- [ ] 初回セットアップフロー（Ollama 検出 → モデル pull → 起動）
- [ ] Ollama 接続設定の UI
- [ ] 環境チェック画面の刷新

### Phase 4: Python 依存の縮小
- [ ] dispatcher.py / ocr.py 等を廃止
- [ ] Python は export のみに縮小
- [ ] 配布パッケージのサイズ検証
