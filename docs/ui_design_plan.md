# UI 設計プラン（現状実装反映 / Tauri + React + Mantine）

確定した入出力仕様（対応拡張子/出力物/ディレクトリ構成）は `docs/spec.md` を参照してください。ここでは UI 設計（画面構成/状態管理/コマンド設計）に集中します。  
設定画面の候補は `docs/cli_commands.md` を参照してください。

## 1. 画面構成（実装ベース）
- **共通レイアウト**: Header（Burger + タイトル + バージョン表示） + Sidebar（ホーム/実行/結果/設定） + Main。Footer は現状なし。
- **Home（ホーム）**:
  - クイック開始（「新規処理を開始」）
  - 最近の結果（`result/` を走査して表示）
  - 環境チェック（`dispatcher.py` / `result/` / Python 実行バイナリの解決結果）
- **Run（OCR 実行）**:
  - 入力: クリック/ドラッグ＆ドロップで複数選択（現状 UI は `.pdf/.heic/.jpg/.jpeg/.png`）
  - 入力ごとの設定: PDF の開始/終了ページ、トリミング（CropModal）
  - 実行: 「処理を実行」ボタン（1ジョブのみ、キャンセルは未実装）
  - オプション: 出力形式（md/docx/xlsx）、処理モード（lite/full）、PDF DPI（今回のみ上書き）、画像をPDF化、図表抽出
  - 進捗/ログ: 進捗率、現在メッセージ、残り時間（推定）、ログ末尾表示
- **Result（結果プレビュー）**:
  - 生成ファイル一覧（ファイル名を Badge で選択）
  - テキストプレビュー（Markdown の内容を表示）とコピー
  - 「結果フォルダを開く / ファイルを開く / 保存（別名保存）」を提供
- **Settings（設定）**:
  - `configs/settings.json` に保存（起動時/Run 画面遷移時に再読込）
  - ウィンドウサイズ、デフォルト出力形式、画像PDF化/図表抽出、GPU 利用、PDF DPI、チャンクサイズ、休憩（有効/秒）
  - `outputRoot` は UI 上で保存できるが、現状 `dispatcher.py --output-root` には未接続（未反映）

## 2. コマンド/API 設計（現状）
UI は Tauri の `invoke` を通じて Rust 側のコマンドを呼び出します（実装は `ui/src-tauri/src/lib.rs`）。

- `run_job(paths, options) -> {jobId}`
- `get_progress(jobId) -> {status, progress, log, currentMessage, pageCurrent, pageTotal, etaSeconds, error}`
- `get_result(jobId) -> {outputs, preview}`
- `save_file(jobId, filename, destPath)`
- `open_output(jobId, filename)`
- `open_output_dir(jobId)`
- `list_recent_results(limit)`
- `open_result_dir(dirName)`
- `open_result_file(dirName)`
- `check_environment()`
- `load_settings()` / `save_settings(settings)`
- `render_preview(path, page?, crop?, maxLongEdge?)`（トリミング用プレビュー）

## 3. データフロー（実装）
- `load_settings` で設定を読み込み、Run の `options` に反映
- `run_job` でジョブ開始 → `jobId` を保持
- `get_progress` を約 800ms 間隔でポーリングし、進捗/ログ/推定残り時間を更新
- `status=done` になったら `get_result` を呼び、`outputs`（ファイル名一覧）と `preview`（Markdown内容）を取得
- トリミングのプレビューは `render_preview`（内部で `ui_preview.py` を実行）を利用
- Home の履歴/環境チェックは `list_recent_results` / `check_environment` を利用

## 4. ディレクトリ構成（実装）
- `ui/src/App.tsx`: 画面遷移、ジョブ状態（filePaths/options/progress/log 等）、ポーリング
- `ui/src/pages/*`: `Home.tsx` / `RunJob.tsx` / `Result.tsx` / `Settings.tsx`
- `ui/src/api/*`: Tauri invoke ラッパ（Tauri 外ではモック応答にフォールバック）
- `ui/src/components/*`: CropModal 等
- `ui/src-tauri/src/lib.rs`: Tauri commands（Python 呼び出し/履歴/設定/ファイル操作/プレビュー）
- `ui_preview.py`: PDF/画像のプレビュー画像生成（トリミング適用可）

## 5. `dispatcher.py` への落とし込み（実装）
Tauri 側は入力ファイルごとに `dispatcher.py` を逐次実行します（並列実行は未対応）。

- `dispatcher.py <path> --formats ... --figure/--no-figure --device ... --mode ...`
- ファイルごとのトリミング: `dispatcher.py --crop left,top,width,height`
- PDF 固有の追加引数: `dispatcher.py <pdf> -- --start/--end/--dpi/--chunk-size/--enable-rest/--rest-seconds`（`ocr_chanked.py` に透過）
- 進捗推定は標準出力のマーカーを解析して更新します（例: `--- Page x/y ---` / `--- Done x/y ---` / `--- merged_md.py を実行 ---` / `[dispatcher] Converting to docx`）。

### 補足: PDF のページ範囲は「ファイルごと」
- 入力一覧の各 PDF に `start/end`（任意）を持たせ、未指定の場合は全ページを処理する。
- CLI への落とし込みは `dispatcher.py <pdf> -- --start N --end M` の透過引数で表現する（PDFごとに異なる場合はファイル単位で実行）。

## 6. テスト（現状）
- Playwright による Vite dev（モック）E2E: `ui/tests/upload.spec.ts`

## 7. 未実装 / 改善候補
- キャンセル（`cancel_job`）未実装
- `outputRoot` の実処理反映（`dispatcher.py --output-root` への接続）
- docx/xlsx のプレビュー拡充（現状は Markdown テキストのみ）
- 入力拡張子/フォルダドロップ（現状 UI は限定的）

## 8. 実装状況
- 実装済み機能/確定仕様: `docs/spec.md`
- 変更履歴: `docs/change_log.md`
