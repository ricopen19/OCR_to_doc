# スマホスキャン OCR → Markdown / Word 変換ツール

## 概要

スマホで撮った画像や、iOS の「書類をスキャン」で作った PDF を入力にして、

- OCR（日本語・数的処理系）
- 図表を含むレイアウトをできるだけ保持
- Markdown / Word / 将来的に Excel へ変換

するためのツール群。

詳細仕様・設計は `docs/` 以下を参照。

## 主な機能

- PDF / 画像からページ単位で画像化
- YomiToku（lite, CPU前提）で OCR 実行
- ページごとの Markdown を結合して 1 ファイル化
- （オプション）Pandoc を使った Word(docx) 出力

## 想定入力

- スマホで撮影した問題集・プリント（jpg, png）
- iOS「書類をスキャン」で生成した PDF（1〜数十ページ）

## ざっくり使い方

```bash
# 仮想環境に入る
poetry shell

# 依存インストール（初回だけ）
poetry add yomitoku pdf2image

# PDF をチャンク処理で OCR
poetry run python ocr_chunked.py input.pdf

# Markdown を結合
poetry run python merge_md.py

# （任意）Word に変換
poetry run python export_docx.py

より詳しい仕様は docs/spec.md、モジュール構成は docs/architecture.md を参照。


---

## 2. `docs/spec.md`（旧「1. プロジェクトの目的（ざっくり仕様）」をここに）

```md
# プロジェクト仕様

## 1. プロジェクトの目的（ざっくり仕様）

### やりたいこと

**入力**

- スマホで撮った画像（jpg, png）
- iOS の「書類をスキャン」で作った PDF（1枚〜数十枚）

**やりたい処理**

- 日本語＋数的処理系問題文の OCR
- 可能な範囲でレイアウト（段組・見出し）や図表を保持
- 図・表の画像切り出し（将来的には位置情報も扱えるようにする）

**出力**

- Markdown（テキスト＋画像リンク）
- Word（.docx）  
- 将来的には、表部分を Excel（.xlsx）で出力できるようにする

---

## 2. パイプライン概要

1. ファイル投入（画像 / PDF）
2. PDF の場合はページ単位に画像化（`pdf2image + poppler`）
3. ページごとに YomiToku（lite, CPU）で OCR
4. ページごとの Markdown ファイルを結合（`merged.md`）
5. 画像リンクを埋め込んだ Markdown を Word / Excel に変換（段階的に対応）

---

## 3. 想定ユースケース

- 問題集・プリントのデジタル化
- 解説プリントの再構成（Word ベース）
- 将来的には、出題パターンごとの再利用（Excel で問題リスト管理）など



---

## 2. `docs/spec.md`（旧「1. プロジェクトの目的（ざっくり仕様）」をここに）

```md
# プロジェクト仕様

## 1. プロジェクトの目的（ざっくり仕様）

### やりたいこと

**入力**

- スマホで撮った画像（jpg, png）
- iOS の「書類をスキャン」で作った PDF（1枚〜数十枚）

**やりたい処理**

- 日本語＋数的処理系問題文の OCR
- 可能な範囲でレイアウト（段組・見出し）や図表を保持
- 図・表の画像切り出し（将来的には位置情報も扱えるようにする）

**出力**

- Markdown（テキスト＋画像リンク）
- Word（.docx）  
- 将来的には、表部分を Excel（.xlsx）で出力できるようにする

---

## 2. パイプライン概要

1. ファイル投入（画像 / PDF）
2. PDF の場合はページ単位に画像化（`pdf2image + poppler`）
3. ページごとに YomiToku（lite, CPU）で OCR
4. ページごとの Markdown ファイルを結合（`merged.md`）
5. 画像リンクを埋め込んだ Markdown を Word / Excel に変換（段階的に対応）

---

## 3. 想定ユースケース

- 問題集・プリントのデジタル化
- 解説プリントの再構成（Word ベース）
- 将来的には、出題パターンごとの再利用（Excel で問題リスト管理）など
