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
    text = text.replace("<br>", "\n")
    return text


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
