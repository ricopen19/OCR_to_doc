"""Utilities to sanitize OCR-generated Markdown before export."""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Match

UNESCAPE_PATTERN = re.compile(r"\\([\-\+\=\{\}\(\)\[\]<>\$\\])")
EXTRA_BACKSLASH_PATTERN = re.compile(r"\\\\")
TAG_PATTERN = re.compile(r"<[^>]+>")
MEDIA_PATH_PATTERN = re.compile(r"(\./(?:(?:figures)|(?:page[_$]*images))/[^)\s]+)")
URL_PATTERN = re.compile(r"https?://\S+|www\.\S+")
PAGE_TAIL_PATTERN = re.compile(r"\.{3}\s*(\d+)")
BULLET_PATTERN = re.compile(r"^(\s*)[・●○◆■◇□▶▷]\s*", re.MULTILINE)
SECTION_ITEM_PATTERN = re.compile(r"^(?P<prefix>\s*[-*])\s*(?:[□■◯○●◆◇▶▷・\-]?\s*)?\$(?P<num>\d+(?:-\d+)+)\$\s*(?P<title>.*)$")
STRAY_MARKER_BEFORE_MEDIA = re.compile(r"(?:\\g(?:<\d+>)?|\$\d+)\s*(?=(?:<)?img\b|<br>|\bbr\b)", re.IGNORECASE)
STRAY_MARKER_AFTER_MEDIA = re.compile(r"((?:<)?img[^>]*>|<br>|\bbr\b)\s*(?:\\g(?:<\d+>)?|\$\d+)", re.IGNORECASE)
BACKREF_TOKEN_PATTERN = re.compile(r"\s*\\g(?:<\d+>)?\s*")
IMG_MISSING_BRACKETS_PATTERN = re.compile(r"(?<!<)(img\s+src=\"[^\"]+\"[^>\n]*)", re.IGNORECASE)
BARE_BR_PATTERN = re.compile(r"(?<!\w)br(?!\w)")
BARE_TAGS = ("details", "/details", "summary", "/summary")
TEX_INLINE_PATTERN = re.compile(r"\$(?P<body>[^$]+)\$")
TEX_BLOCK_INLINE_PATTERN = re.compile(r"\$\$(?P<body>[\s\S]+?)\$\$")
TEX_TEXT_COMMAND_PATTERN = re.compile(r"\\text\{([^}]*)\}")
TEX_COMMAND_PATTERN = re.compile(r"\\[A-Za-z]+")
TEX_FRACTION_PATTERN = re.compile(r"\\frac\{([^{}]+)\}\{([^{}]+)\}")
TEX_SUB_SUP_PATTERN = re.compile(r"([A-Za-z]+)\s*[_^]\s*\{?(\d+)\}?")
TABLE_RULE_PATTERN = re.compile(r"^:?-{1,}:?$")
DIVIDER_CELL_PATTERN = re.compile(r"^(?P<left>:)?(?P<dashes>-+)(?P<right>:)?$")


def clean_text(line: str) -> str:
    text = line.rstrip("\n")
    stripped = text.strip()
    if stripped == "$$":
        return ""
    text = UNESCAPE_PATTERN.sub(lambda m: m.group(1), text)
    text = EXTRA_BACKSLASH_PATTERN.sub(r"\\", text)
    text = text.replace("’", "'")
    contains_url = bool(URL_PATTERN.search(text))
    if contains_url:
        cleaned = text.replace("<br>", "").replace("$", "")
        return sanitize_media_paths(cleaned)

    text = strip_tex_math_delimiters(text)
    text = apply_formatting_templates(text)
    text = normalize_headings(text)
    text = normalize_layout_marks(text)
    text = cleanup_stray_markers(text)
    text = recover_html_tokens(text)
    text = sanitize_media_paths(text)
    text = strip_backrefs(text)
    return text


MEDIA_TOKENS = ("://", "./", ".png", ".jpg", ".jpeg", ".gif", "figures/", "page_images/")


def strip_tex_math_delimiters(text: str) -> str:
    """LaTeX/TeX の数式デリミタやコマンドを「表示用の素の文字列」に寄せる。"""

    def strip_inline(match: Match[str]) -> str:
        return match.group("body")

    text = text.replace("\\[", "").replace("\\]", "")
    text = text.replace("\\(", "").replace("\\)", "")
    text = TEX_BLOCK_INLINE_PATTERN.sub(lambda m: m.group("body").strip(), text)
    prev = None
    while prev != text:
        prev = text
        text = TEX_INLINE_PATTERN.sub(strip_inline, text)

    text = TEX_TEXT_COMMAND_PATTERN.sub(lambda m: m.group(1), text)
    text = TEX_FRACTION_PATTERN.sub(lambda m: f"({m.group(1)})/({m.group(2)})", text)
    text = TEX_SUB_SUP_PATTERN.sub(lambda m: f"{m.group(1)}_{m.group(2)}", text)
    text = text.replace("{", "").replace("}", "")
    text = TEX_COMMAND_PATTERN.sub("", text)
    return text


def apply_formatting_templates(text: str) -> str:
    templates = load_formatting_templates()
    if not templates:
        return text
    return _apply_templates_to_segment(text, templates)


def sanitize_media_paths(text: str) -> str:
    def repl(match: Match[str]) -> str:
        segment = match.group(1)
        return segment.replace("$", "")

    return MEDIA_PATH_PATTERN.sub(repl, text)


def _is_md_table_line(line: str) -> bool:
    stripped = line.strip()
    return stripped.startswith("|") and stripped.count("|") >= 1


def _split_md_table_line(line: str) -> list[str]:
    stripped = line.strip()
    if not stripped:
        return []
    if stripped.startswith("|"):
        stripped = stripped[1:]
    if stripped.endswith("|"):
        stripped = stripped[:-1]
    return [cell.strip() for cell in stripped.split("|")]


def _is_table_divider_line(line: str) -> bool:
    if not _is_md_table_line(line):
        return False
    cells = _split_md_table_line(line)
    return len(cells) >= 2 and all(TABLE_RULE_PATTERN.match(cell.replace(" ", "")) for cell in cells)


def _normalize_table_row(row_text: str, expected_cols: int) -> str:
    working = row_text.strip()
    if not working.startswith("|"):
        working = "|" + working
    if not working.endswith("|"):
        working = working + "|"

    cells = _split_md_table_line(working)
    if len(cells) < expected_cols:
        cells = cells + [""] * (expected_cols - len(cells))
    elif len(cells) > expected_cols:
        merged_tail = "|".join(cells[expected_cols - 1 :])
        cells = cells[: expected_cols - 1] + [merged_tail]
    cells = [cell.strip() for cell in cells]
    return "|" + "|".join(cells) + "|"


def _normalize_divider_line(line: str, expected_cols: int) -> str:
    cells = _split_md_table_line(line)
    if len(cells) < expected_cols:
        cells = cells + ["-"] * (expected_cols - len(cells))
    elif len(cells) > expected_cols:
        cells = cells[:expected_cols]

    normalized: list[str] = []
    for cell in cells:
        compact = cell.replace(" ", "")
        match = DIVIDER_CELL_PATTERN.match(compact)
        if not match:
            normalized.append("---")
            continue
        left = ":" if match.group("left") else ""
        right = ":" if match.group("right") else ""
        dashes = "-" * max(3, len(match.group("dashes")))
        normalized.append(f"{left}{dashes}{right}")
    return "|" + "|".join(normalized) + "|"


def _row_is_flushable(row_text: str, expected_cols: int) -> bool:
    working = row_text.strip()
    if not working.endswith("|"):
        return False
    cells = _split_md_table_line(working)
    return len(cells) >= expected_cols


def normalize_broken_markdown_tables(text: str, *, max_header_lines: int = 12) -> str:
    """Detect Markdown table blocks and join broken multi-line rows using <br>.

    This runs on full text and only touches regions that look like tables
    (a header row followed by a divider line).
    """

    lines = text.splitlines()
    out: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if not _is_md_table_line(line) or _is_table_divider_line(line):
            out.append(line)
            i += 1
            continue

        header_buf = line.strip()
        divider_index = None
        j = i + 1
        consumed = 0
        while j < len(lines) and consumed < max_header_lines:
            candidate = lines[j]
            if candidate.strip() == "":
                break
            if _is_table_divider_line(candidate):
                divider_index = j
                break
            header_buf = header_buf + "<br>" + candidate.strip()
            j += 1
            consumed += 1

        if divider_index is None:
            out.append(line)
            i += 1
            continue

        expected_cols = len(_split_md_table_line(lines[divider_index]))
        if expected_cols < 2:
            out.append(line)
            i += 1
            continue

        header_line = _normalize_table_row(header_buf, expected_cols)
        divider_line = _normalize_divider_line(lines[divider_index], expected_cols)
        body_lines: list[str] = []

        current_row: str | None = None
        k = divider_index + 1
        aborted = False
        while k < len(lines):
            candidate = lines[k]
            stripped = candidate.strip()
            if stripped == "":
                if current_row is not None:
                    if not _row_is_flushable(current_row, expected_cols):
                        aborted = True
                        break
                    body_lines.append(_normalize_table_row(current_row, expected_cols))
                    current_row = None
                break

            if current_row is None:
                if _is_md_table_line(candidate):
                    current_row = candidate.strip()
                    k += 1
                    continue
                break

            if _row_is_flushable(current_row, expected_cols):
                if _is_md_table_line(candidate):
                    body_lines.append(_normalize_table_row(current_row, expected_cols))
                    current_row = candidate.strip()
                    k += 1
                    continue
                body_lines.append(_normalize_table_row(current_row, expected_cols))
                current_row = None
                break

            current_row = current_row + "<br>" + stripped
            k += 1

        if aborted:
            out.append(line)
            i += 1
            continue

        out.append(header_line)
        out.append(divider_line)
        out.extend(body_lines)
        i = k

    return "\n".join(out)


def cleanup_stray_markers(text: str) -> str:
    """Remove stray regex backreferences left around media tags and <br>."""

    text = STRAY_MARKER_BEFORE_MEDIA.sub("", text)
    text = STRAY_MARKER_AFTER_MEDIA.sub(r"\1", text)
    return text


def strip_backrefs(text: str) -> str:
    """Remove lingering \\g or \\g<1> tokens that survived other cleaners."""

    return BACKREF_TOKEN_PATTERN.sub(" ", text)


def recover_html_tokens(text: str) -> str:
    """Re-wrap img/details/summary/br that lost angle brackets."""

    # img 行に < > を補う
    def repl_img(match: Match[str]) -> str:
        body = match.group(1).strip()
        return f"<{body}>"

    text = IMG_MISSING_BRACKETS_PATTERN.sub(repl_img, text)

    # br をタグ化（単語中の br は避ける）
    text = BARE_BR_PATTERN.sub("<br>", text)

    # details / summary タグ
    for tag in BARE_TAGS:
        text = re.sub(rf"(?<!<){tag}(?!>)", f"<{tag}>", text, flags=re.IGNORECASE)

    # img タグに紛れ込んだ <br> を外に出す
    text = re.sub(r"<img([^>]*?)<br>[^>]*>", r"<img\1><br>", text, flags=re.IGNORECASE)
    # img の閉じ > を保証
    text = re.sub(r"(<img[^>\n]*)(?<!/)>?", r"\1>", text, flags=re.IGNORECASE)
    # details/summary の閉じタグを修正
    text = re.sub(r"\$\$\s*/details\s*\$\$", "</details>", text, flags=re.IGNORECASE)
    text = re.sub(r"(?<!<)/details(?!>)", "</details>", text, flags=re.IGNORECASE)
    text = re.sub(r"(?<!<)details(?!>)", "<details>", text, flags=re.IGNORECASE)
    text = re.sub(r"(?<!<)/summary(?!>)", "</summary>", text, flags=re.IGNORECASE)
    text = re.sub(r"<summary>([^<]*?)/<summary>", r"<summary>\1</summary>", text, flags=re.IGNORECASE)

    return text


def finalize_html_tokens(text: str) -> str:
    """Fix remaining placeholders after full-line pass."""

    replacements = {
        r"\$\$\s*/details\s*\$\$": "</details>",
        r"\$\$\s*details\s*\$\$": "<details>",
        r"\$\$\s*/summary\s*\$\$": "</summary>",
        r"\$\$\s*summary\s*\$\$": "<summary>",
    }
    for pattern, repl in replacements.items():
        text = re.sub(pattern, repl, text, flags=re.IGNORECASE)
    return text


def normalize_layout_marks(text: str) -> str:
    text = re.sub(r"\s*<br>\s*", "<br>", text)
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


PAGE_HEADING_PATTERN = re.compile(r"^#\s+Page\s+\d+\s*$")
H1_PATTERN = re.compile(r"^(?P<prefix>\s*)#\s+(?P<title>.+)$")


def demote_inner_headings_between_pages(text: str) -> str:
    """# Page n で区切られた範囲内の単独 H1 を H2 に落とす。"""

    lines = text.splitlines()
    in_page = False
    result: list[str] = []
    for line in lines:
        stripped = line.strip()
        if PAGE_HEADING_PATTERN.match(stripped):
            in_page = True
            result.append(line)
            continue
        m = H1_PATTERN.match(line)
        if m and in_page:
            title = m.group("title").strip()
            result.append(f"{m.group('prefix')}## {title}")
        else:
            result.append(line)
    return "\n".join(result)


def clean_file(path: Path, inplace: bool = True) -> Path:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    cleaned_lines: list[str] = []
    for line in lines:
        cleaned = clean_text(line)
        if cleaned == "":
            continue
        cleaned_lines.append(cleaned)
    cleaned = "\n".join(cleaned_lines)
    cleaned = normalize_broken_markdown_tables(cleaned)
    cleaned = demote_inner_headings_between_pages(cleaned)
    cleaned = finalize_html_tokens(cleaned)
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
        clean_file(md_path, inplace=False).rename(args.output)
    else:
        clean_file(md_path, inplace=True)
FORMATTING_TEMPLATE_PATH = Path(__file__).with_name("formatting_templates.json")


@dataclass
class FormattingTemplate:
    name: str
    pattern: re.Pattern[str]
    replacement: str


_FORMATTING_TEMPLATES: list[FormattingTemplate] | None = None


def _apply_templates_to_segment(segment: str, templates: list[FormattingTemplate]) -> str:
    for template in templates:
        def repl(match: Match[str]) -> str:
            groups = {key: (match.group(key) or "").strip() for key in match.re.groupindex}
            try:
                return template.replacement.format(**groups)
            except KeyError:
                return match.group(0)

        segment = template.pattern.sub(repl, segment)
    return segment


def load_formatting_templates() -> list[FormattingTemplate]:
    global _FORMATTING_TEMPLATES
    if _FORMATTING_TEMPLATES is not None:
        return _FORMATTING_TEMPLATES

    templates: list[FormattingTemplate] = []
    try:
        data = json.loads(FORMATTING_TEMPLATE_PATH.read_text(encoding="utf-8"))
    except FileNotFoundError:
        _FORMATTING_TEMPLATES = []
        return _FORMATTING_TEMPLATES
    except json.JSONDecodeError:
        _FORMATTING_TEMPLATES = []
        return _FORMATTING_TEMPLATES

    for entry in data:
        pattern_text = entry.get("pattern")
        replacement = entry.get("replacement")
        if not pattern_text or replacement is None:
            continue
        flags_value = 0
        for flag_name in entry.get("flags", []):
            flag = getattr(re, flag_name, None)
            if isinstance(flag, int):
                flags_value |= flag
        try:
            pattern = re.compile(pattern_text, flags_value)
        except re.error:
            continue
        templates.append(FormattingTemplate(entry.get("name", ""), pattern, replacement))

    _FORMATTING_TEMPLATES = templates
    return _FORMATTING_TEMPLATES


if __name__ == "__main__":
    main()
