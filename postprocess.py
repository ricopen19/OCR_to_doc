"""ページ単位の Markdown を結合するユーティリティ。"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List

INPUT_DIR = Path("result")
DEFAULT_OUTPUT = Path("merged.md")
PATTERN = re.compile(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md$")


@dataclass(order=True)
class PageFile:
    page: int
    part: int
    path: Path


def collect_md_files(input_dir: Path = INPUT_DIR) -> List[PageFile]:
    files: List[PageFile] = []
    for md in input_dir.glob("*.md"):
        match = PATTERN.match(md.name)
        if not match:
            continue
        page = int(match.group(1))
        part = int(match.group(2)) if match.group(2) else 0
        files.append(PageFile(page, part, md))
    files.sort()
    return files


def write_merged_md(files: Iterable[PageFile], output_path: Path, add_page_heading: bool = True) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as out:
        current_page = None
        first_section = True
        for entry in files:
            text = entry.path.read_text(encoding="utf-8").strip()
            if not text:
                continue
            if add_page_heading and current_page != entry.page:
                if not first_section:
                    out.write("\n")
                out.write(f"# Page {entry.page}\n\n")
                current_page = entry.page
                first_section = False
            out.write(text + "\n\n")


def cleanup(files: Iterable[PageFile]) -> None:
    for entry in files:
        try:
            entry.path.unlink()
        except FileNotFoundError:
            pass


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="result 配下の md を結合する")
    parser.add_argument("--input", default=str(INPUT_DIR), help="ページ Markdown のディレクトリ")
    parser.add_argument("--output", default=None, help="出力ファイルパス (省略時は <base-name>_merged.md)")
    parser.add_argument("--base-name", default="merged", help="出力ベース名。--output 指定時は無視")
    parser.add_argument("--keep-pages", action="store_true", help="ページ Markdown を削除せずに残す")
    parser.add_argument("--no-heading", action="store_true", help="ページ見出しを挿入しない")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    input_dir = Path(args.input)
    output_path = (
        Path(args.output)
        if args.output
        else Path(input_dir) / f"{args.base_name}_merged.md"
    )

    files = collect_md_files(input_dir)
    if not files:
        raise SystemExit(f"結合対象の md ファイルが見つかりません: {input_dir}")

    write_merged_md(files, output_path, add_page_heading=not args.no_heading)

    if args.keep_pages:
        print("ページ単位の md ファイルを保持しました (--keep-pages)")
    else:
        cleanup(files)
        print("ページ単位の md ファイルを削除しました。")

    print(f"結合完了: {output_path}")


if __name__ == "__main__":
    main()
