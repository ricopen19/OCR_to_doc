"""Utilities to sanitize OCR-generated Markdown before export."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from typing import Match

UNESCAPE_PATTERN = re.compile(r"\\([\-\+\=\{\}\(\)\[\]<>\$\\])")
EXTRA_BACKSLASH_PATTERN = re.compile(r"\\\\")
DOUBLE_DOLLAR_PATTERN = re.compile(r"\$\s+\$")
INLINE_MATH_GAP_PATTERN = re.compile(r"\$\s+([^$]+?)\s+\$")
FORMULA_LINE_PATTERN = re.compile(r"^[=\+\-*/0-9A-Za-z\\{}^_.,×÷ \$]+$")
MATH_OPERATORS = set("=+-×÷*/")
INLINE_MATH_PATTERN = re.compile(
    r"(?<!\$)(?P<expr>(?:[πA-Za-z0-9]+(?:\s+[πA-Za-z0-9]+)*)"
    r"(?:[×÷·⋅:\+\-*/=](?:[πA-Za-z0-9]+(?:\s+[πA-Za-z0-9]+)*))+)(?!\$)"
)
INLINE_SIMPLE_PATTERN = re.compile(
    r"(?<!\$)(?P<prefix>[=:]\s*)?(?P<body>(?:π|[A-Za-z]+)[A-Za-z0-9]*\d+)(?!\$)"
)
INLINE_UNIT_PATTERN = re.compile(
    r"(?<!\$)(?P<body>\d+\s*(?:cm|mm|m|km)\d+)(?!\$)"
)
SUPERSCRIPT_PATTERN = re.compile(r"([A-Za-z\\]+)(\d+)")
TAG_PATTERN = re.compile(r"<[^>]+>")
INLINE_ANY_PATTERN = re.compile(r"\$(?P<expr>[^$]{0,80})\$")
EMBEDDED_BLOCK_PATTERN = re.compile(r"\$\$\s*\$+")
TRAILING_BLOCK_PATTERN = re.compile(r"\$+\s*\$\$")
MEDIA_PATH_PATTERN = re.compile(r"(\./(?:(?:figures)|(?:page[_$]*images))/[^)\s]+)")
URL_PATTERN = re.compile(r"https?://\S+|www\.\S+")
PAGE_TAIL_PATTERN = re.compile(r"\.{3}\s*(\d+)")
BULLET_PATTERN = re.compile(r"^(\s*)[・●○◆■◇□▶▷]\s*", re.MULTILINE)
SECTION_ITEM_PATTERN = re.compile(r"^(?P<prefix>\s*[-*])\s*(?:[□■◯○●◆◇▶▷・\-]?\s*)?\$(?P<num>\d+(?:-\d+)+)\$\s*(?P<title>.*)$")


def clean_text(line: str) -> str:
    text = line.rstrip("\n")
    text = UNESCAPE_PATTERN.sub(lambda m: m.group(1), text)
    text = EXTRA_BACKSLASH_PATTERN.sub(r"\\", text)
    text = text.replace("’", "'")
    contains_url = bool(URL_PATTERN.search(text))
    if contains_url:
        cleaned = text.replace("<br>", "").replace("$", "")
        return sanitize_media_paths(cleaned)

    text = text.replace("\\[", "$$").replace("\\]", "$$")
    text = text.replace("\\(", "$").replace("\\)", "$")
    text = DOUBLE_DOLLAR_PATTERN.sub("$$", text)
    text = INLINE_MATH_GAP_PATTERN.sub(lambda m: f"${m.group(1)}$", text)
    text = collapse_block_dollars(text)
    text = cleanup_dangling_dollar(text)
    text = strip_invalid_inline_segments(text)
    text = normalize_fragmented_math(text)
    text = format_inline_math(text)
    text = normalize_headings(text)
    text = normalize_layout_marks(text)
    text = sanitize_media_paths(text)
    
    if needs_block_math(text):
        stripped = text.strip()
        if not stripped.startswith("$$"):
            return f"$$ {stripped} $$"
    return text


def needs_block_math(text: str) -> bool:
    stripped = text.strip()
    if not stripped:
        return False
    if stripped.startswith("|") or stripped.startswith("#"):
        return False
    if stripped.count("$$"):
        return False
    if stripped.count("$") >= 2:
        return False
    if not any(op in stripped for op in MATH_OPERATORS):
        return False
    # avoid bullet / image lines
    if stripped.startswith(("- ", "* ", "<", "!")):
        return False
    return bool(FORMULA_LINE_PATTERN.match(stripped))


def normalize_fragmented_math(text: str) -> str:
    stripped = text.strip()
    if stripped.count("$") >= 2:
        candidate = stripped.replace("$", "")
        if looks_like_formula_line(candidate):
            return f"$$ {candidate} $$"
    return text


def looks_like_formula_line(line: str) -> bool:
    stripped = line.strip()
    if not stripped:
        return False
    if stripped.startswith(("- ", "* ", "#", "<", "!")):
        return False
    if not any(op in stripped for op in MATH_OPERATORS):
        return False
    return bool(FORMULA_LINE_PATTERN.match(stripped))


MEDIA_TOKENS = ("://", "./", ".png", ".jpg", ".jpeg", ".gif", "figures/", "page_images/")


def _format_inline_expression(expr: str) -> tuple[str, bool]:
    expr = expr.strip()
    if not expr:
        return expr, True
    if any(token in expr for token in MEDIA_TOKENS):
        return expr, True
    if "<" in expr or ">" in expr:
        return expr, True
    replacements = {
        "π": r"\\pi",
        "×": r" \\times ",
        "÷": r" \\div ",
        "·": r" \\cdot ",
        "⋅": r" \\cdot ",
    }
    for src, dst in replacements.items():
        expr = expr.replace(src, dst)
    expr = re.sub(r"\btimes\b", r" \\times ", expr)
    expr = re.sub(r"(?<!\\)\bpi\b", r"\\pi", expr)
    expr = re.sub(r"(\\pi)(?=[A-Za-z0-9])", r"\1 ", expr)
    expr = SUPERSCRIPT_PATTERN.sub(lambda m: f"{m.group(1)}^{{{m.group(2)}}}", expr)
    expr = re.sub(r"\s+", " ", expr).strip()
    expr = expr.replace("\\\\", "\\")
    return expr, False


def _apply_inline_pattern(segment: str) -> str:
    def repl(match: Match[str]) -> str:
        formatted, skip = _format_inline_expression(match.group("expr"))
        if skip:
            return match.group("expr")
        return f"${formatted}$"

    segment = INLINE_MATH_PATTERN.sub(repl, segment)

    def repl_simple(match: Match[str]) -> str:
        prefix = match.group("prefix") or ""
        body = match.group("body")
        lower = body.lower()
        if any(token in lower for token in ("fig", "page", "http", "https", "img")):
            return prefix + body
        formatted, skip = _format_inline_expression(body)
        if skip:
            return prefix + body
        return f"{prefix}${formatted}$"

    segment = INLINE_SIMPLE_PATTERN.sub(repl_simple, segment)

    def repl_unit(match: Match[str]) -> str:
        body = match.group("body")
        formatted, skip = _format_inline_expression(body)
        if skip:
            return body
        return f"${formatted}$"

    segment = INLINE_UNIT_PATTERN.sub(repl_unit, segment)
    return segment


def format_inline_math(text: str) -> str:
    result: list[str] = []
    last = 0
    for tag in TAG_PATTERN.finditer(text):
        segment = text[last:tag.start()]
        if segment:
            result.append(_apply_inline_pattern(segment))
        result.append(tag.group(0))
        last = tag.end()
    if last < len(text):
        result.append(_apply_inline_pattern(text[last:]))
    return "".join(result)


def contains_cjk(text: str) -> bool:
    return any("\u3040" <= ch <= "\u9FFF" for ch in text)


def collapse_block_dollars(text: str) -> str:
    text = EMBEDDED_BLOCK_PATTERN.sub("$$ ", text)
    text = TRAILING_BLOCK_PATTERN.sub(" $$", text)
    return text


def cleanup_dangling_dollar(text: str) -> str:
    replacements = {
        "$-": "-",
        "-$": "-",
        "$+": "+",
        "+$": "+",
        "$=": "=",
        "=$": "=",
    }
    for src, dst in replacements.items():
        text = text.replace(src, dst)
    text = re.sub(r"(?<=\w)\$(?=\d)", "", text)
    text = re.sub(r"(?<=\d)\$(?=\w)", "", text)
    text = re.sub(r"(?<=\d)\$(?=\d)", "", text)
    return text


def strip_invalid_inline_segments(text: str) -> str:
    def repl(match: Match[str]) -> str:
        expr = match.group("expr")
        trimmed = expr.strip()
        if not trimmed:
            return trimmed
        if contains_cjk(trimmed):
            return trimmed
        if len(trimmed) <= 2 and not any(ch.isalnum() for ch in trimmed):
            return trimmed
        return match.group(0)

    return INLINE_ANY_PATTERN.sub(repl, text)


def sanitize_media_paths(text: str) -> str:
    def repl(match: Match[str]) -> str:
        segment = match.group(1)
        return segment.replace("$", "")

    return MEDIA_PATH_PATTERN.sub(repl, text)


def normalize_layout_marks(text: str) -> str:
    text = re.sub(r"\s*<br>\s*", "\n", text)
    text = re.sub(r"(<img[^>]+>)\s*\n+", r"\1\n", text)
    text = PAGE_TAIL_PATTERN.sub(lambda m: f"（p.{m.group(1)}）", text)
    text = BULLET_PATTERN.sub(lambda m: f"{m.group(1)}- ", text)
    text = SECTION_ITEM_PATTERN.sub(lambda m: format_section_item(m), text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text


def normalize_headings(text: str) -> str:
    stripped = text.strip()
    match = re.match(r"^(#+)\s+\$(\d+(?:-\d+)+)\$\s*(.*)$", stripped)
    if not match:
        return text
    level = min(6, max(1, len(match.group(2).split("-"))))
    title = match.group(3).replace("<br>", " ").strip()
    heading = f"{'#'*level} {match.group(2)}"
    if title:
        heading += f" {title}"
    return heading


def format_section_item(match: Match[str]) -> str:
    title = match.group("title").strip()
    if title:
        return f"- {match.group('num')} {title}"
    return f"- {match.group('num')}"


def clean_file(path: Path, inplace: bool = True) -> Path:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    cleaned_lines: list[str] = []
    in_block = False
    for line in lines:
        stripped = line.strip()
        if stripped == "$$":
            in_block = not in_block
            cleaned_lines.append("$$")
            continue
        if in_block:
            cleaned_lines.append(line)
            continue
        cleaned_lines.append(clean_text(line))
    cleaned = "\n".join(cleaned_lines)
    if inplace:
        path.write_text(cleaned, encoding="utf-8")
        return path
    out_path = path.with_suffix(path.suffix + ".cleaned")
    out_path.write_text(cleaned, encoding="utf-8")
    return out_path


def main() -> None:
    parser = argparse.ArgumentParser(description="Sanitize OCR Markdown (latex escapes, etc.)")
    parser.add_argument("markdown", help="入力 Markdown ファイル")
    parser.add_argument("--output", help="別ファイルへ書き出す場合のパス")
    args = parser.parse_args()

    md_path = Path(args.markdown)
    if not md_path.exists():
        raise SystemExit(f"Markdown ファイルが見つかりません: {md_path}")

    if args.output:
        text = md_path.read_text(encoding="utf-8")
        cleaned = "\n".join(clean_text(line) for line in text.splitlines())
        Path(args.output).write_text(cleaned, encoding="utf-8")
    else:
        clean_file(md_path, inplace=True)


if __name__ == "__main__":
    main()
