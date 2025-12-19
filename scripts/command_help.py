"""プロジェクト内主要コマンドの簡易ヘルプを一覧表示する。

使い方:
    poetry run python scripts/command_help.py
"""

from __future__ import annotations


def main() -> None:
    sections = {
        "セットアップ": [
            "poetry shell",
            "poetry install",
        ],
        "入口 (dispatcher)": [
            "poetry run python dispatcher.py sample.heic --mode full --figure --fallback-tesseract",
            "poetry run python dispatcher.py sample.heic --image-as-pdf --image-dpi 300 --mode full --figure --fallback-tesseract",
            "poetry run python dispatcher.py sample.pdf -- --start 11 --end 20",
        ],
        "直接OCR (ocr_chanked)": [
            "poetry run python ocr_chanked.py input.pdf",
            "poetry run python ocr_chanked.py input.pdf --start 11 --end 20 --mode full",
            "poetry run python ocr_chanked.py input.pdf --emit-json on",
            "poetry run python ocr_chanked.py input.pdf --emit-json auto",
            "poetry run python ocr_chanked.py input.pdf --math-refiner",
            "poetry run python ocr_chanked.py input.pdf --drop-page-images",
        ],
        "マージ / エクスポート": [
            "poetry run python poppler/merged_md.py --input result/<name> --base-name <name>",
            "poetry run python export_docx.py <name>_merged.md",
            "poetry run python export_yomi_json.py --input result/<name> --mode lite",
            "poetry run python math_snippet_extractor.py --input result/<name> --base-name <name> --min-ops 0 --min-score 0.5",
        ],
        "テスト": [
            "python -m unittest tests/test_markdown_cleanup.py",
        ],
    }

    print("=== OCR_to_doc コマンド早見表 ===")
    for title, commands in sections.items():
        print(f"\n[{title}]")
        for cmd in commands:
            print(f"  - {cmd}")


if __name__ == "__main__":
    main()
