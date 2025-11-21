"""テキスト埋め込み PDF を markitdown で Markdown 化するユーティリティ。"""

from __future__ import annotations

import subprocess
from pathlib import Path


class TextPdfError(RuntimeError):
    pass


def convert_with_markitdown(pdf_path: Path, output_path: Path | None = None) -> Path:
    if not pdf_path.exists():
        raise TextPdfError(f"PDF ファイルが見つかりません: {pdf_path}")

    output_path = output_path or pdf_path.with_suffix(".md")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    cmd = [
        "markitdown",
        str(pdf_path),
        "-o",
        str(output_path),
    ]

    try:
        subprocess.run(cmd, check=True)
    except subprocess.CalledProcessError as exc:  # pragma: no cover
        raise TextPdfError(f"markitdown の実行に失敗しました: {exc}") from exc

    return output_path


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="テキストPDFを markitdown で Markdown 化します")
    parser.add_argument("pdf", help="入力 PDF ファイル")
    parser.add_argument("--output", help="出力 Markdown ファイル (省略時は <PDF名>.md)")
    args = parser.parse_args()

    pdf_path = Path(args.pdf)
    out_path = Path(args.output) if args.output else None
    try:
        result = convert_with_markitdown(pdf_path, out_path)
    except TextPdfError as exc:  # pragma: no cover
        print(str(exc))
        raise SystemExit(1)

    print(f"変換完了: {result}")
