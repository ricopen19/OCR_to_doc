# OCR to Doc（スマホスキャン OCR → Markdown / Word / Excel 変換）

スマホで撮った画像やスキャン PDF を入力に、ローカル OCR（Ollama + glm-ocr）で Markdown 化し、
必要に応じて Word（docx）/ Excel（xlsx）/ CSV へ変換する macOS 向けデスクトップアプリです。
Tauri（Rust + React）製で、OCR 処理・エクスポートともにローカルで完結します（データを外部送信しません）。

## できること
- 入力: PDF / HEIC / JPG / PNG
- 出力: Markdown（常に生成）+ 任意で Word（docx）/ Excel（xlsx）/ CSV
- 出力先: `result/<入力名>/`（アプリと同じフォルダ配下、変更可）
- 表領域の再 OCR（`enableTableReocr`）、図表検出（YOLOv8x-DocLayNet、`enableFigure`）に対応
- すべてローカル処理

## 動作環境
- macOS（Apple Silicon）
- [Ollama](https://ollama.com/)（`glm-ocr` モデルを使用）
- Poppler（PDF → 画像変換に使用）

配布物（DMG）には Ollama / Poppler / glm-ocr モデル本体は同梱されません。初回起動時にアプリの
「ホーム」画面で不足コンポーネントを検出し、`brew install poppler` / `brew install ollama` /
`ollama pull glm-ocr:latest` などのコマンドを案内します。

Windows 版は GLM-OCR 移行前（YomiToku 構成）のビルドが CI 上に残っていますが、現状は配布対象外です。

## 使い方
### 1) DMG を入手して起動
配布された `.dmg` を開き、アプリを `Applications` にインストールして起動します。

### 2) 環境チェック
ホーム画面で Poppler / Ollama / glm-ocr モデルの状態を確認し、不足があれば表示されたコマンドを
ターミナルで実行してから再チェックします。

### 3) ファイルを追加して実行
1. 画面の枠へドラッグ&ドロップ、またはクリックしてファイル選択
2. 出力形式（docx / xlsx / csv、複数選択可）、トリミング、DPI などを設定
3. 「処理を実行」
4. 結果は `result/<入力名>/` 配下に出力されます（アプリの「結果」からも参照可能）

### 4) 初回実行について
初回は glm-ocr モデルのロードに時間がかかることがあります。ファンレス機での発熱を抑えるため、
ページ間に休止を挟む設定（デフォルト ON）や DPI 調整（省エネ 150 / 標準 200 / 高精細 300）が
利用できます。

## 処理の流れ（概要）
```
入力(PDF/画像)
  → [PDF] Poppler で1ページずつ画像化
  → glm-ocr (Ollama) でページ OCR
       → 表領域は必要に応じて glm-ocr で再OCR（enableTableReocr ON 時）
  → page_###.md → マージ → *_merged.md
  → 選択した形式へエクスポート（docx / xlsx / csv、Python subprocess）
```

## 注意点
- OCR 結果には誤認識が含まれる可能性があります。最終成果物は必ず目視で確認してください。
- レイアウトや表は入力によって崩れることがあります。必要に応じて手動調整してください。
- 数式・LaTeX まわりは glm-ocr 自体の生成揺らぎで崩れることがあります。

## ライセンス / 利用条件（暫定）
- 本アプリは **非営利（教育/研究/個人利用）向けに限定公開**します。
- 金銭授受が発生する利用（有償PoC/受託/コンサル/業務利用など）は不可です。
- 社内業務の効率化・コスト削減を目的とした利用は商用扱いの可能性が高いです。
- 出力結果（OCRテキストや加工済みファイル）の外部提供は商用扱いとなる可能性があります。
