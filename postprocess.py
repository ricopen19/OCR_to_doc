"""ページ単位の Markdown を結合するユーティリティ。"""

from __future__ import annotations

import argparse
import csv
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List

from markdown_cleanup import clean_file

INPUT_DIR = Path("result")
DEFAULT_OUTPUT = Path("merged.md")
PATTERN = re.compile(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md$")
FRACTION_KEYWORDS = ("比率", "割合", "分数", "比率", "率", "比")
FRACTION_SYMBOLS = ("/", "÷", "×", "%", "％")


@dataclass(order=True)
class PageFile:
    page: int
    part: int
    path: Path


@dataclass
class MathIssue:
    page: int
    line: int
    reason: str
    text: str


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


def write_merged_md(
    files: Iterable[PageFile],
    output_path: Path,
    add_page_heading: bool = True,
) -> List[MathIssue]:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    issues: List[MathIssue] = []
    with output_path.open("w", encoding="utf-8") as out:
        current_page: int | None = None
        page_chunks: list[str] = []
        first_section = True

        def flush_page(page: int | None) -> None:
            nonlocal first_section
            if page is None:
                return
            page_text = "\n\n".join(chunk for chunk in page_chunks if chunk.strip())
            page_chunks.clear()
            if not page_text.strip():
                return
            if add_page_heading:
                if not first_section:
                    out.write("\n")
                out.write(f"# Page {page}\n\n")
                first_section = False
            out.write(page_text + "\n\n")
            detected = detect_math_issues(page_text, page)
            if detected:
                issues.extend(detected)

        for entry in files:
            if current_page is None:
                current_page = entry.page
            elif entry.page != current_page:
                flush_page(current_page)
                current_page = entry.page
            page_chunks.append(entry.path.read_text(encoding="utf-8").strip())

        flush_page(current_page)

    return issues


def cleanup(files: Iterable[PageFile]) -> None:
    for entry in files:
        try:
            entry.path.unlink()
        except FileNotFoundError:
            pass


def detect_math_issues(text: str, page: int) -> List[MathIssue]:
    issues: List[MathIssue] = []
    for idx, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        reason = None
        if looks_like_fraction(stripped):
            reason = "fraction_like"
        elif noisy_dollar(stripped):
            reason = "noisy_dollar"
        if reason:
            issues.append(MathIssue(page=page, line=idx, reason=reason, text=stripped))
    return issues


def looks_like_fraction(text: str) -> bool:
    if not any(sym in text for sym in ("=", "≒", "≠")):
        return False
    if not any(sym in text for sym in FRACTION_SYMBOLS):
        return False
    return any(keyword in text for keyword in FRACTION_KEYWORDS)


def noisy_dollar(text: str) -> bool:
    if text.count("$") < 2:
        return False
    return any("\u3040" <= ch <= "\u9FFF" for ch in text)


def inject_page_image(out_stream, page: int, image_dir: Path) -> None:
    # 互換のため空関数を維持（将来の再利用時に差し込み位置を明確化するため）。
    return


def write_math_review_log(log_path: Path, issues: List[MathIssue]) -> None:
    if not issues:
        if log_path.exists():
            log_path.unlink()
        return
    with log_path.open("w", encoding="utf-8", newline="") as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(["page", "line", "reason", "text"])
        for issue in issues:
            writer.writerow([issue.page, issue.line, issue.reason, issue.text])


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="result 配下の md を結合する")
    parser.add_argument("--input", default=str(INPUT_DIR), help="ページ Markdown のディレクトリ")
    parser.add_argument("--output", default=None, help="出力ファイルパス (省略時は <base-name>_merged.md)")
    parser.add_argument("--base-name", default="merged", help="出力ベース名。--output 指定時は無視")
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

    issues = write_merged_md(
        files,
        output_path,
        add_page_heading=not args.no_heading,
    )
    write_math_review_log(input_dir / "math_review.csv", issues)
    clean_file(output_path, inplace=True)
    clean_file(output_path, inplace=True)  # run twice to ensure backrefs are cleared after layout injection

    cleanup(files)
    print("ページ単位の md ファイルを削除しました（デフォルト動作）。")

    print(f"結合完了: {output_path}")


if __name__ == "__main__":
    main()
