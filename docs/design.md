# 検討中の仕様

実装が終わったセクションは削除する。

---

## GLM-OCR 移行設計

### 背景

- YomiToku は無償利用が個人用途のみ（商用・組織利用は別途ライセンスが必要）。参考: https://kotaro-kinoshita.github.io/yomitoku/commercial_use_guideline/
- 現在の OCR エンジン YomiToku は配布が難しい（torch 依存で重い、ランタイム同梱が必要）
- GLM-OCR（0.9B）は Ollama 対応で `ollama pull glm-ocr` だけで導入可能。MIT ライセンスで配布制約なし
- Ollama ベースにすれば OCR + LLM 校正を統一基盤で提供できる

### ゴール

- Ollama さえ入れればアプリが動く状態にする
- OCR 精度は YomiToku 同等以上を維持
- 既存のエクスポートパイプライン（docx/xlsx/csv）は維持

### 方針（確定）

- **Ollama API 直接呼び出し**（HTTP `localhost:11434`）。完全ローカル通信。
- **ハイブリッド構成**: OCR / MD 処理 / セットアップは Rust（Tauri）、エクスポート（docx/xlsx）は Python を維持
- **図表抽出は Phase 1 から組み込む**
- **初回セットアップは GUI から自動化**（ターミナル不要）

### アーキテクチャ

```
┌─────────────────────────────────────────────┐
│  Tauri (Rust)                               │
│  ├─ Ollama API 呼び出し (OCR + LLM 校正)    │  ← reqwest で直接
│  ├─ Markdown マージ・クリーンアップ           │  ← Rust で直接
│  ├─ 図表抽出 (レイアウト検出 + crop)          │  ← 要検討
│  ├─ 初回セットアップ (モデル pull + 起動)     │  ← Rust で直接
│  └─ docx / xlsx エクスポート                 │  ← Python に委任
│       └─ python-docx / openpyxl             │
└─────────────────────────────────────────────┘
         │ HTTP (localhost:11434)
┌────────▼────────────────────────────────────┐
│  Ollama (ローカルプロセス)                    │
│  ├─ glm-ocr     (OCR)                      │
│  └─ gemma3 等   (校正 LLM、オプション)       │
└─────────────────────────────────────────────┘
```

### 初回セットアップフロー

```
アプリ初回起動
  → Ollama インストール確認
    ├─ 未 → ダイアログ「Ollamaが必要です」+ OS別ダウンロードリンクを開く
    │       → インストール完了後「再確認」ボタン
    └─ 済
      → Ollama プロセス起動確認
        ├─ 未起動 → 自動起動 (ollama serve)
        └─ 起動済み
          → glm-ocr モデル確認 (ollama list)
            ├─ 未取得 → 「初回セットアップ中...」+ プログレス表示で自動 pull
            └─ 取得済み → 通常起動
```

- Ollama のインストールだけはユーザー操作（OS のインストーラー）が必要
- それ以外（起動・モデル取得）はアプリが自動で行う
- 校正 LLM は設定画面から任意で追加（`ollama pull <model>`）

### 変更対象と影響範囲

| コンポーネント | 変更 | 担当 |
|---|---|---|
| Tauri (lib.rs) | Ollama API 呼び出し、セットアップ、MD マージを Rust 実装 | Rust |
| `ocr.py` | 廃止（Rust に移行） | - |
| `ocr_chanked.py` | 廃止（PDF→画像化 + OCR を Rust 側に移行） | - |
| `dispatcher.py` | 廃止（Tauri が直接オーケストレーション） | - |
| `postprocess.py` | 廃止（MD マージを Rust に移行） | - |
| `markdown_cleanup.py` | 廃止（Rust に移行） | - |
| `export_docx.py` | **維持**（Tauri から Python を呼び出し） | Python |
| `export_excel_poc.py` | **維持**（JSON 構造の変換は必要） | Python |
| GUI (React) | Ollama 設定 UI、セットアップ画面を追加 | React |

### 図表抽出の方針

GLM-OCR は figure 抽出機能を持たない。代替案の比較：

| 案 | 方式 | Ollama で完結 | 精度 | 導入コスト |
|---|---|---|---|---|
| A. PP-DocLayout | PaddlePaddle ベース | No（Python 依存） | 高（23カテゴリ） | 重い |
| B. DocLayout-YOLO | YOLO + ONNX | No（ONNX Runtime 必要） | 高 | 中 |
| C. VLM で検出 | Ollama 上の VLM に bbox を出力させる | Yes | 未検証 | 軽い |
| D. Rust + ONNX | DocLayout-YOLO を ONNX Runtime Rust バインディングで実行 | No | 高 | 中 |

**案 D（Rust + ONNX）が有力**: Python 依存を増やさず、Rust バイナリに統合できる。ONNX モデルファイル（数十MB）はアプリに同梱 or 初回ダウンロード。

**案 C は並行検証**: Ollama で完結できれば最もシンプル。精度次第。

### LLM 校正フロー

```
GLM-OCR 出力（Markdown）
  → Ollama LLM に校正プロンプトを送信
  → 校正済み Markdown を受け取り
  → page_###.md として保存
```

- **オプション機能**（`--refine` or GUI トグル）
- 校正用モデルは設定で指定可能（既定: gemma3）
- 用途: 誤字修正、テーブル構造の補正、読み順の修正

### JSON 出力の互換性

xlsx/csv エクスポートは yomi_formats/json に依存。GLM-OCR の JSON は構造が異なる：

```
YomiToku: tables / paragraphs / figures に分かれた構造
GLM-OCR:  index / label / content / bbox_2d のフラットなリスト
```

→ エクスポート側を GLM-OCR JSON ネイティブ対応に書き直す（アダプターより直接対応が保守しやすい）

### 実装フェーズ

#### Phase 1: Rust + Ollama で OCR 動作
- [ ] Rust から Ollama API で GLM-OCR を呼び出す
- [ ] PDF → 画像化を Rust で実装（pdfium or Poppler バインディング）
- [ ] Markdown 出力 + マージを Rust で実装
- [ ] 図表抽出（DocLayout-YOLO ONNX + Rust or VLM 検証）
- [ ] docx エクスポート（Python 呼び出し維持）の動作確認

#### Phase 2: エクスポート + 校正
- [ ] GLM-OCR JSON → xlsx/csv 変換の対応（Python 側を修正）
- [ ] LLM 校正フローの実装（Rust → Ollama）

#### Phase 3: セットアップ + GUI
- [ ] 初回セットアップフロー（Ollama 検出 → モデル pull → 起動）
- [ ] Ollama 接続設定の UI
- [ ] 環境チェック画面の刷新

#### Phase 4: Python 依存の縮小
- [ ] dispatcher.py / ocr.py / ocr_chanked.py / postprocess.py を廃止
- [ ] Python は export_docx.py + export_excel_poc.py のみに縮小
- [ ] 配布パッケージのサイズ検証
