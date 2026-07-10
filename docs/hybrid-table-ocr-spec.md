# 引き継ぎ書: ハイブリッド表再OCR（Unlimited OCR + glm-ocr）

Sonnet セッション向けの実装指示書。実装前に全体を読むこと。

## 背景（検証済みの事実）

- 現行 OCR エンジン `hf.co/sahilchachra/Unlimited-OCR-GGUF:Q4_K_M` は、表を
  `table [x1, y1, x2, y2]<table>セル1セル2...</table>` の形式で出力する。
  **`<tr>` / `<td>` タグは一切含まれず**、セル内容が区切りなしで連結される。
  これはモデルの学習仕様であり、プロンプト変更では直らない（3種のプロンプトで検証済み）。
- そのため `html_table_to_markdown()`（`ui/src-tauri/src/ocr/pipeline.rs:41`）は
  `<td>` を見つけられず空文字を返し、**表が Markdown 出力から丸ごと消えている**（現行バグ）。
- `table [x1, y1, x2, y2]` の座標は **0-1000 に正規化された値**で、元ページ画像に対して
  `px = 座標 * 画像幅(高さ) / 1000` で正確にマッピングできる（実験で確認済み）。
- 表領域を切り出して旧エンジン `glm-ocr`（Ollama、2.2GB）に投げると、
  約15秒で 11行×6列の表をほぼ完璧な Markdown テーブルとして復元できた（実験で確認済み）。
- glm-ocr は同じ表を2回出力する癖がある（プレーン出力の後に ```table フェンス付きで再出力）。
  重複除去が必要。

## 仕様

GUI に「表の高精度再OCR」トグル `enableTableReocr` を追加する。**デフォルト OFF**。

| 設定 | 表の出力 | 備考 |
|---|---|---|
| ON | 表領域を glm-ocr で再OCR し Markdown テーブルとして出力 | 表があるページのみ +15秒程度 |
| OFF | セル内容を平坦テキストとして出力（構造なし・内容は保持） | 現状の「表が消える」バグも直る |

- ON でも `glm-ocr` モデルが未インストール／再OCR 失敗の場合は、エラーで止めず
  **平坦テキストにフォールバック**する（進捗コールバックで警告を1回流す）。
- Unlimited OCR 以外のモデル使用時（`is_unlimited_ocr_format` が false の経路）の挙動は一切変えない。

## 実装手順

### 1. Rust: 設定の追加

- `ui/src-tauri/src/settings.rs` の `AppSettings` に `enable_table_reocr: bool` を追加
  （`#[serde(default)]`、camelCase 変換により JSON キーは `enableTableReocr`）。
- `ui/src-tauri/src/ocr/pipeline.rs` の `OcrOptions` に `enable_table_reocr: bool` を追加
  （`Default` 実装は `false`）。
- `ui/src-tauri/src/lib.rs` で `AppSettings` → `OcrOptions` に値を渡す。
  **`enable_figure` / `enable_rest` が通っている経路を grep してそのまま並べる**こと。

### 2. Rust: pipeline.rs の変換処理

`pipeline.rs` に定数を追加:

```rust
const TABLE_OCR_MODEL: &str = "glm-ocr";
```

#### 2-a. 平坦テキストフォールバック（OFF 時・失敗時共通）

`html_table_to_markdown()` を修正: セルが1つも取れなかった場合、空文字ではなく
`strip_html_tags(html)` の結果（trim 済み）を平坦テキストとして返す。
これで OFF 時でも表の内容が消えなくなる。
（`<td>` が取れた場合の既存ロジックは互換性のため残す）

#### 2-b. 表領域の収集と再OCR

`unlimited_ocr_to_markdown()` は現在 `table` 行を即変換している。これを次の構造に変える:

1. パース時に `table` 要素の bbox（`[x1, y1, x2, y2]` の4値）と本文を
   `Vec<TableRegion>` として収集し、Markdown 側にはプレースホルダ
   `<!--TABLE_REOCR_{index}-->` を挿入する。
   bbox がパースできない行は従来どおり 2-a のフォールバック変換で埋める。
   関数シグネチャは `(String, Vec<TableRegion>)` を返す形に変更（同期のまま）。
2. `ocr_image_to_md()`（`pipeline.rs:325` 付近）で変換後に:
   - `enable_table_reocr` が ON かつ `TableRegion` が1件以上ある場合、
     各領域についてページ画像から切り出し → glm-ocr で再OCR → プレースホルダを結果で置換。
   - OFF・glm-ocr 不在・再OCR 失敗の場合、プレースホルダを平坦テキスト
     （2-a と同じ `strip_html_tags` 結果）で置換。
   - **プレースホルダを出力に残さないこと**（全経路で必ず置換する）。

ページ画像の削除（`run_ocr_pipeline` 内の `fs::remove_file`）は `ocr_image_to_md` の
戻り後なので、`ocr_image_to_md` 内で再OCR を完結させれば削除タイミングの変更は不要。

#### 2-c. 画像の切り出し

`image` crate（既に依存に入っている、v0.25）を使用:

- 座標変換: `px_x1 = x1 * width / 1000`（y も同様）。
- パディング: 各辺に 1%（座標値で ±10、画像端でクランプ）。実験でこの値で罫線まで収まった。
- 切り出した画像は PNG エンコードして base64 化し、`client.chat_vision(TABLE_OCR_MODEL, "OCR", &b64)` に渡す。
  既存の `encode_image_for_ocr` にリサイズ処理があるなら流用を検討（glm-ocr の
  リサイズ上限 1792・28の倍数アライメントの経緯は git log の `fix(glm-ocr)` コミット参照）。

#### 2-d. glm-ocr の存在チェックと出力の重複除去

- `run_ocr_pipeline` の冒頭（既存の `has_model` チェックの近く）で、
  `enable_table_reocr` が ON なら `client.has_model(TABLE_OCR_MODEL)` を1回だけ確認。
  不在なら進捗コールバックで「glm-ocr が見つからないため表は平坦テキストで出力します」と
  警告し、以降フォールバック動作にする（エラーにしない）。
- glm-ocr 出力の重複除去: 出力に ` ```table ` フェンスがあればフェンス内のみ採用
  （フェンス記号は除去）。なければ出力全体から最初の Markdown テーブルブロック
  （`|` 始まりの連続行）を抽出。それも無ければ出力全体をそのまま使う。

### 3. GUI: トグルの追加

- `ui/src/api/settings.ts` の設定型に `enableTableReocr: boolean` を追加（デフォルト `false`）。
- `ui/src/pages/Settings.tsx` に既存の `enableFigure` トグルと同じ UI パターンでスイッチを追加。
  ラベル: 「表を高精度で再OCR（glm-ocr 使用）」
  補足文: 「表の行・列構造を復元します。メモリ 8GB 以下の PC では OFF 推奨」

### 4. テスト

`pipeline.rs` の既存テスト（あれば）に倣い、ユニットテストを追加:

- タグなし `<table>テキスト</table>` → 平坦テキストが返る（空文字にならない）
- `table [1,2,3,4]<table>...</table>` 行 → プレースホルダ挿入 + `TableRegion` 収集
- bbox なし `table` 行 → フォールバック変換
- glm-ocr 出力の重複除去（```table フェンスあり／なし両ケース）

## 動作確認手順（完了条件）

1. `cd ui/src-tauri && cargo build` が警告なしで通る
2. `cargo test` が通る
3. サンプル `/Users/ricopen/workspace/OCR_to_doc/yomitoku_ocr_table_sample_v1.pdf` の
   2ページ目を GUI から OCR し:
   - トグル OFF: 表の内容が平坦テキストとして page_002.md に含まれる（表が消えない）
   - トグル ON: page_002.md に `| ID | カテゴリ（JP) | ...` 形式の Markdown テーブル
     （11行×6列相当）が出力され、プレースホルダ `<!--TABLE_REOCR_` が残っていない

## 制約

- Surgical changes: 変更は上記ファイルのみ。無関係なリファクタ・フォーマット変更はしない
- Unlimited OCR 以外のモデル経路（`is_unlimited_ocr_format` false）の挙動を変えない
- コミットメッセージは既存の規約（`feat(ocr): ...` 形式・日本語）に合わせる
