# UI 設計プラン (Tauri + React + Mantine 前提)

確定した入出力仕様（対応拡張子/出力物/ディレクトリ構成）は `docs/spec.md` を参照してください。ここでは UI 設計（画面構成/状態管理/コマンド設計）に集中します。  
設定画面の候補は `docs/cli_commands.md` を参照してください。

## 1. 情報設計 / 画面ラフ
- **共通レイアウト**: Header (ロゴ + ナビ: Home/Run/Settings) / Main / Footer (バージョン・コピーライト)。
- **Home (Dashboard)**: 「新規処理」ボタン、最近の処理一覧（ファイル名・種別・ステータス・所要時間・出力リンク）。
- **Run (処理画面)**:
  - 左: 入力パネル（ドロップ/選択、入力一覧テーブル、削除/並べ替え）。
  - 右: オプション（言語=ja固定/将来多言語、ページ範囲（PDFごと・未指定=全ページ）、画質補正ON、出力形式: md/docx/xlsx、出力先パス表示）。
  - 下: 実行ボタン、進捗バー、現在ページ、キャンセル、簡易ログトグル。
- **Result (詳細/閲覧)**:
  - プレビュータブ: Markdownテキスト表示、Docx/Xlsxのサマリ、(将来) PDF。
  - ダウンロードボタン: md/docx/xlsx、フォルダを開く。
  - メタ: 処理時間、ページ数、エラー有無。
- **Settings**:
  - 出力先ディレクトリ設定
  - ログレベル (info/debug)
  - オフラインモード（モデル再DL禁止）
  - GPU オプション (フラグのみ)
  - 初回モデルDL許可
  - ログビュー（折り畳み）

## 2. コンポーネント粒度 (atoms/molecules/organisms)
- Atoms: Button variants, TextInput, FileDropArea, Badge(Status), ProgressBar, Toggle, Select, NumberInput, Tooltip, Toast.
- Molecules: FileListTable, OptionCard (各オプションのカード), LogPanel, ResultPreviewTabs, StatsSummary。
- Organisms: RunPanel (入力+オプション+実行), DashboardRecentList, SettingsForm, ResultDetail。

## 3. 状態管理 / データフロー
- **Zustand**: UIローカル状態（選択ファイル、オプション、進捗、ログ表示フラグ）。
- **React Query**: 実行ジョブの開始/進捗ポーリング/結果取得 API をラップ。リトライやキャッシュ制御を任せる。
- **Tauri コマンド**: バックエンド（Rustまたは既存Python呼び出し）とやりとり。実行、キャンセル、ログ取得、設定保存/読込。

## 4. ルーティング & ディレクトリ構成 (提案)
- `src/ui/` (React):
  - `pages/`: `Home.tsx`, `Run.tsx`, `Settings.tsx`, `Result.tsx`
  - `components/atoms/*`, `components/molecules/*`, `components/organisms/*`
  - `hooks/` (useRunJob, useSettings, useResults)
  - `store/` (Zustand)
  - `api/` (React Query client, Tauri invoke wrappers)
  - `styles/` (Mantine + Tailwind reset/utility)
- `src-tauri/`:
  - `src/commands.rs` (run_job, cancel_job, list_recent, read_log, save_config)
  - `resources/` に poppler など同梱
  - `config.json`, `logs/app.log`

## 5. API / コマンド設計（フロント⇔バック）
- `run_job(inputs: FileMeta[], options: RunOptions) -> {jobId}`
- `get_progress(jobId) -> {status, progress, currentPage, eta, logSnippet}`
- `get_result(jobId) -> {outputs: {md, docx, xlsx}, meta}`
- `cancel_job(jobId)`
- `list_recent(limit=20)`
- `save_config(config)`, `load_config()`

### 補足: PDF のページ範囲は「ファイルごと」
- 入力一覧の各 PDF に `start/end`（任意）を持たせ、未指定の場合は全ページを処理する。
- CLI への落とし込みは `dispatcher.py <pdf> -- --start N --end M` の透過引数で表現する（PDFごとに異なる場合はファイル単位で実行）。

## 6. 初回モデルDLフロー（UI）
- 実行前に: poppler 同梱チェック → OK なら続行
- モデルDLが必要な場合のみダイアログ表示  
  - 文言: 「追加モデルをダウンロードします（推定 XX MB）。続行しますか？」  
  - プログレス表示、キャンセル可、失敗時はリトライ導線

## 7. テスト（Playwright/E2E）
- 主要シナリオ: ファイルドロップ → オプション変更 → 実行 → 進捗表示 → 結果ダウンロード。
- レスポンシブ: 360px 幅でフォーム/リスト崩れないかスナップショット。
- エラー表示: poppler 未同梱をモックしてエラーダイアログ確認。

## 8. 実装状況
- 実装済み機能/確定仕様: `docs/spec.md`
- 変更履歴: `docs/change_log.md`

## 9. 次のステップ（機能改善・追加）
- [ ] 設定画面の実装：出力先、コマンドオプション等の設定UIとバックエンド連携。
- [ ] ホーム画面（履歴）の実装：最近の処理一覧の表示と再実行/結果確認へのリンク。
- [ ] 実行画面の改善：ステータスバーの滑らかな進捗表示、ログ詳細表示。
- [ ] 結果画面の改善：Markdown 以外のテキストプレビュー、Excel 変換結果の表示。
- [ ] 画像入力対応：PDF 以外の画像ファイルからの変換フロー追加。
