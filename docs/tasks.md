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
  → 検証: `cargo test --lib`（22 passed, 2 ignored、多行復元／セル数不揃いのパディング／空セル保持の新規テスト3件を含む）で確認済み。実機での glm-ocr 出力を通した動作確認は済み（`reocr_pdf_manual`、`yomitoku_ocr_table_sample_v1.pdf` で複数行テーブル復元を確認）
- [x] ハルシネーション対策のギャップを解消。まず表の行反復（見出し行の再掲・区切り行の有無で間隔が不揃いになるケース）に対応する `find_repeating_rows_cut` を新設し、`html_table_to_markdown`／`extract_table_markdown` に適用（実機検証で `yomitoku_ocr_table_sample_v1.pdf` Page3の反復解消を確認）。この過程で `truncate_runaway_repetition` の数行ブロック拡張（`detect_block_repeat_cut`）は「表以外での実例がない」と判断していったん単一行判定に戻したが、直後にGUIでの実機テスト（`1周間SPI_模擬4.pdf`、ユーザー実施）で、**表とは無関係なページ本文中に3行ブロックが ``` フェンスに包まれながら数十〜100回以上反復し、途中から無関係な中国語文献名を生成する暴走**（1ページが本来数十行のところ1000行超に膨張）を確認し、この判断が誤りだったと判明。`detect_block_repeat_cut`／`blocks_similar`（結合テキストのbigram類似度ではなく対応位置の行を1行ずつ比較、全行一致のみ「反復」と判定）／ブロック長に応じた出現回数閾値の緩和を `truncate_runaway_repetition` に復元した
  → 検証: `cargo test --lib`（32 passed, 2 ignored、実データ2件（表の行反復・ページ本文の暴走）を模した回帰テストを含む）。実機再検証: (1) `yomitoku_ocr_table_sample_v1.pdf` 全5ページで表の反復解消を確認、(2) `1周間SPI_模擬4.pdf` p2-4（実際に暴走が発生した箇所）を `reocr_pdf_manual` で再実行し、暴走が解消され131行（従来この範囲だけで1000行超）に収まることを確認
- [x] （撤回・下記「Unlimited OCR 撤去」参照）実機検証中に別件で発見していた `unlimited_ocr_to_markdown` のトークン変換漏れは、Unlimited OCR 自体の撤去により該当コードごと消滅したため対応不要になった

## Unlimited OCR 撤去・glm-ocr 単体構成への回帰

上記「Unlimited OCR 表対応」で構築したハイブリッド構成（本文=Unlimited OCR、表=glm-ocr）は、GUI での実機テスト（`1周間SPI_模擬4.pdf`）で、表とは無関係な**ページ本文そのものの暴走生成**（3行ブロックが数十〜100回反復し、1ページが1000行超に膨張）を引き起こすことが判明した。`detect_block_repeat_cut` 等での後追い検知を重ねてきたが、Web検索で Unlimited-OCR-GGUF 自体が「R-SWA（限定KVキャッシュ＋スライディングウィンドウ注意機構）による長文生成時のコンテキストアンカー逸脱」「`no_repeat_ngram_size` 等の従来手法では検知できない、少しずつ変化しながら反復するハルシネーション」という**アーキテクチャ由来の既知の欠陥**を持つことが確認できた（GitHub Issue含む複数の第三者報告）。後付けの文字列検知で追いかけ続けるのは構造的に不利と判断し、過去に精度面で問題のなかった glm-ocr 単体構成に戻すことにした（速度面の退行は許容）。

- [x] Unlimited OCR 関連コードを撤去し、`OCR_MODEL` を glm-ocr 単体に一本化。削除: `TableRegion` / `unlimited_ocr_to_markdown` / `is_unlimited_ocr_format` / `ocr_prompt_for` / `parse_table_bbox` / `PendingTable` / `stage_table_placeholders` / `table_content_confidence` / `TABLE_REOCR_CONFIDENCE_THRESHOLD` / `crop_table_region` / `encode_table_crop_for_ocr` / `reocr_table_image` / `resolve_pending_tables` / `flatten_pending_tables` / `TABLE_REOCR_CONCURRENCY`、および対応する表クロップ・整形用ヘルパー（`html_table_to_markdown` / `extract_table_markdown` / `find_repeating_rows_cut` / `parse_pipe_row` / `split_table_rows` / `extract_row_cells` / `strip_html_tags`。呼び出し元が消えたため dead code として削除。表クロップをやめ、ページ全体を glm-ocr に渡してそのまま Markdown として書き出す構成になったため）。`enableTableReocr` 設定・GUI トグルも撤去。ページ間モデル入れ替え（Phase1/Phase2、`unload_model`）も不要になったため撤去。`truncate_thought_leak` / `truncate_runaway_repetition`（ブロック単位反復検知含む）は glm-ocr でも暴走が起きうる保険として維持
  → 検証: `cargo build --tests`（warning 0件）、`cargo test --lib`（7 passed, 1 ignored）で確認済み。実機での glm-ocr 単体運用（表を含むページの品質・速度）の再検証は未実施
- [x] （撤回）`OllamaClient::unload_model` のタイムアウト調査は、モデル入れ替え自体が撤去されたため不要になった

## pdf-inspector 統合（テキストPDFのOCRスキップ）

- [x] `pdf-inspector`（crates.io、MIT、純Rust・lopdf依存）を導入し、埋め込みテキストPDFでOllama OCR呼び出しをスキップできるオプションを追加。新規モジュール `ocr/pdf_text.rs`（`classify_pdf`: 文書全体の高速分類、`extract_page_texts`: ページ単位の抽出＋`needs_ocr`判定）。GUI トグル `useEmbeddedText`（デフォルト OFF、一括選択）は選択中PDFが `TextBased`/`Mixed` の場合のみ表示。ページ画像生成・図表抽出（YOLOv8x）は `enable_figure` が有効な場合のみ引き続き実行し、表抽出パイプラインとの依存関係を壊さないようにした。安全網として、pdf-inspector 側が `needs_ocr=true`（エンコード崩れ・置換文字等による garbled text 検出）と判定したページは埋め込みテキストを使わず通常の OCR にフォールバックする
  → 検証: `cargo test --lib`（22 passed, 2 ignored）、`cargo build`、フロント `npx tsc --noEmit`・`npm run build` で確認済み。実機検証: (1) `yomitoku_ocr_table_sample_v1.pdf`（文書全体は `TextBased` 判定だが全ページ `suspected_garbled_text` で `needs_ocr=true`）で `REOCR_EMBEDDED_TEXT=1` を指定しても全ページ通常OCR経路にフォールバックすることを確認、(2) `cupsfilter` で生成した英語テキストPDF（`needs_ocr=false`）で `REOCR_EMBEDDED_TEXT=1` 時に「埋め込みテキスト使用中」ログとともにOllama呼び出しなし・0.17秒で完了し、出力Markdownが元テキストと一致することを確認。GUI トグル表示・操作の目視確認は未実施
- [ ] 「一度低品質OCRがかけられた既存の検索可能PDF」（スキャナ内蔵OCR等）のケースは、pdf-inspector の `needs_ocr` 判定（エンコード崩れ検出）では捕捉できない。文字コードとしては正しくデコードできるが認識自体が誤っている場合、現状の安全網はすり抜ける。ユーザーが明示的にトグルを選ぶ設計により実務上のリスクは緩和しているが、品質を積極的に検知する手段は未実装
  → 検証: 対応するかどうかも含め方針未確定。対応する場合は実際にそうしたPDF（スキャナのおまかせOCR済みPDF等）を用意し、既存の安全網で誤ってOCRスキップされないことを確認する

## 表の再OCR 高速化（/review-loop での分析・実装）

- [x] `resolve_pending_tables`（ocr/pipeline.rs）が表領域を1件ずつ逐次 `await` していた構造を、`tokio::task::JoinSet` + `Semaphore` によるリクエストの並行実行に再設計。Ollama への `chat_vision` 呼び出しのみを並行化し、md_path への読み込み・置換・書き込みは全リクエストの結果が出揃ってから1回ずつ行うことで、旧実装が持っていた「同一ファイルへの並行読み書きレースコンディション」リスクを構造ごと排除した。`OllamaClient` に `Clone` を追加（reqwest::Client は内部Arcで安価にクローン可能）
  → 検証: `cargo test --lib`（22 passed, 2 ignored）で確認済み。実機検証（`reocr_pdf_manual`、`yomitoku_ocr_table_sample_v1.pdf`）で **並列度3は並列度1より大幅に遅い**（6表で536秒 vs 5表で83秒）ことを確認。このマシンの Ollama（`OLLAMA_MLX=1`、Apple Silicon MLXバックエンド）では同一モデルへの同時リクエストが真の並列処理にならず、GPU/メモリ資源の奪い合いで逐次実行より悪化するため、`TABLE_REOCR_CONCURRENCY` は 1（実質逐次）に設定して運用している。並列化によるI/O構造の安全性向上は活かしつつ、速度面の当初仮説（並列化で高速化）は本環境では成立しなかった
  → 検証: 複数GPU環境やOllamaサーバーの並列度を明示的に上げた環境で `TABLE_REOCR_CONCURRENCY` を上げる効果を再検証する余地はあるが、未着手

## 表の内容妥当性スコアリング（/adhd での発散・収束後の実装、ログのみ）

`/adhd`で発散的に検討した結果、③（Tesseract等の古典OCRとのハイブリッド化）は実データ検証（男性列の数字8個中約半分を誤読、Tesseractのconfidenceも誤読箇所で中〜高スコアを出すなど）で投資対効果が見合わないと判断し保留。①（内容妥当性ベースの信頼度スコアリング）に絞って着手した。

- [x] `table_content_confidence`（ocr/pipeline.rs）を追加。Unlimited OCRが返す表の生HTML/テキストに対し、置換文字混入率・かな漢字数字以外の文字比率・固定候補周期（4/6/8/10/12/16文字）での反復検出を合成したスコア（0.0〜1.0）を算出する。Unlimited OCRは正常時も異常時も`<tr>`/`<td>`を伴わないフラットなHTMLを返す仕様（ADR-013）のため、HTML構造ではなく中身の文字統計で判定する設計にした。既知の限界: 「はい」→「是い」のような統計的に自然に見える文字の誤認識は検知できない。現時点では`stage_table_placeholders`内でログ出力のみ行い、実際の再OCR要否判定にはまだ使わない
  → 検証: `cargo test --lib`（27 passed, 2 ignored、スコアリングの新規テスト5件を含む）で確認済み。実機検証（`reocr_pdf_manual`、`1周間SPI_模擬4.pdf` p1-3、`env_logger`をdev-dependencyに追加してテスト内でログ出力可能にした）で、正常な表(page1の集計表)はconfidence=1.00、実際に選択肢が欠落・破綻した壊れた表2件（page2のサイコロ問題・確率問題の選択肢表）はconfidence=0.90〜0.91で「かな/漢字/数字以外の文字比率が高い」フラグが立つことを確認。誤検知ではなく実際に壊れた表を正しく識別できていた
- [x] ログで得たスコア分布（正常1.00 vs 破綻0.90〜0.91）を踏まえ、「閾値未満の表だけ再OCRする」本番分岐ロジックに切り替え。`stage_table_placeholders`（ocr/pipeline.rs）に `TABLE_REOCR_CONFIDENCE_THRESHOLD = 0.95` を追加し、confidenceが閾値以上ならクロップ自体を試みず`html_table_to_markdown`で即座に平坦変換、閾値未満のみ従来通りクロップして`PendingTable`として再OCR保留する。閾値を跨いだどちらの判定もログに残し（スキップ／再OCR対象の別を明記）、後から目視で見逃しを洗い出せるようにした
  → 検証: `cargo test --lib`（31 passed, 2 ignored）。実画像を使った新規テスト2件で、低confidenceの表はクロップ画像が実際に`PendingTable`へ積まれること、高confidenceの表は画像が開けてもクロップを試みず即座にスキップされることを確認済み。⚠️ 閾値0.95は実データ3件（正常1件・破綻2件）のみに基づく暫定値であり、実際に23箇所前後の表を含むPDFで「再OCR対象が本当に減ったか」「見逃し（本当は壊れているのに閾値以上と判定され再OCRされなかった表）がないか」を目視確認するのは未実施

## メモリ超過（2モデル同時常駐）の診断・修正

ユーザー報告（実機のOCR実行中にアクティビティモニタで確認）: llama-serverプロセスが11.59GBを消費し、23箇所の表OCRで30分かかる。「非力なPCでもなるべく動く」という設計前提が崩れている疑いがあったため、`ollama ps`とプロセスメモリを実機で監視して原因を診断した。

- [x] 診断の結果、2つの事実が判明した。(1) Unlimited-OCR-GGUFは`docs/design.md`等に記載の「0.9B」ではなく、実際は`ollama show`で**2.93Bパラメータ**（deepseek2-ocrアーキテクチャ、CLIPビジョンプロジェクタ401M込み）と判明。想定の3倍以上のサイズで、GPUメモリロード時は5.4GB（glm-ocrは4.0GB）。(2) Phase1（Unlimited OCR）終了直後にPhase2（glm-ocr）が呼ばれる際、Unlimited OCRの`keep_alive`（3分）がまだ切れておらず、明示的にアンロードしない限り**2モデルが完全に同時常駐**していた（5.4GB+4.0GB=9.4GB+プロセスオーバーヘッド ≈ ユーザー報告の11.59GBとほぼ一致）。この状態で表の再OCRが実際に失敗する事象も観測した
  → 検証: `ollama ps`を6秒間隔で監視し、修正前は Unlimited-OCR-GGUF と glm-ocr が同時に「100% GPU」表示される時間帯が実測で確認できた
- [x] Phase2開始直前（`resolve_pending_tables`呼び出し前）に`client.unload_model(OCR_MODEL)`を明示的に呼び、Unlimited OCRを即座にアンロードするよう修正（`run_ocr_pipeline`、ocr/pipeline.rs）
  → 検証: `cargo test --lib`（27 passed, 2 ignored）で確認済み。実機検証（クリーンな状態から`ollama ps`を8秒間隔で監視）で、Phase1中はUnlimited-OCR-GGUFのみ、Phase2移行後はglm-ocrのみが表示され、**2モデル同時常駐が解消**されたことを確認
- [ ] `OllamaClient::unload_model`のタイムアウト（3秒）が短すぎる可能性がある副作用として発見: 複数の検証用バックグラウンドプロセスが同時にOllamaへ接続している状況で`ollama stop`が90秒以上「Stopping...」のまま進まない現象を観測した（通常の単一プロセス運用では問題なし）。実運用でジョブを連続実行した際に同様の遅延が起きないか、別途確認が必要
  → 検証: 実運用に近い形（GUIから複数ジョブを短時間に連続実行等）でアンロードが詰まらないか確認する
