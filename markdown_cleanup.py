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
BLOCK_TRAILING_DIGIT_PATTERN = re.compile(r"(\$\$\s*[\s\S]+?\s*\$\$)(?:\s*<br>)?\s*\b2\b", re.MULTILINE)
BLOCK_NESTED_DOLLAR_PATTERN = re.compile(r"\$\$\s*\$+([\s\S]*?)\$+\s*\$\$", re.MULTILINE)
STRAY_MARKER_BEFORE_MEDIA = re.compile(r"(?:\\g(?:<\d+>)?|\$\d+)\s*(?=(?:<)?img\b|<br>|\bbr\b)", re.IGNORECASE)
STRAY_MARKER_AFTER_MEDIA = re.compile(r"((?:<)?img[^>]*>|<br>|\bbr\b)\s*(?:\\g(?:<\d+>)?|\$\d+)", re.IGNORECASE)
BACKREF_TOKEN_PATTERN = re.compile(r"\s*\\g(?:<\d+>)?\s*")
IMG_MISSING_BRACKETS_PATTERN = re.compile(r"(?<!<)(img\s+src=\"[^\"]+\"[^>\n]*)", re.IGNORECASE)
BARE_BR_PATTERN = re.compile(r"(?<!\w)br(?!\w)")
BARE_TAGS = ("details", "/details", "summary", "/summary")
BIT_TEXT_NESTED_PATTERN = re.compile(r"\\text\s*{\s*\\text\s*{\s*\\$\\text\s*{ビット}\\$}\s*}")
BIT_TEXT_INNER_PATTERN = re.compile(r"\\text\s*{\s*\\$\\text\s*{ビット}\\$\\s*}")
BIT_TEXT_SIMPLE_PATTERN = re.compile(r"\\text\s*{\s*ビット\\s*}")
UNIT_TOKENS = ["回/秒", "ビット/回", "ビット", "バイト", "kバイト", "秒", "[ビット/バイト]"]
UNIT_TOKEN_PATTERN = re.compile("|".join(re.escape(token) for token in UNIT_TOKENS))
UNIT_TOKEN_OUTSIDE_PATTERN = re.compile(
    r"(?<!\$)(?P<unit>" + "|".join(re.escape(token) for token in UNIT_TOKENS) + r")(?!\$)"
)
LOG_PATTERN_INLINE = re.compile(r"\$log\^\{?2\}?\s*n\$", re.IGNORECASE)
LOG_PATTERN_BARE = re.compile(r"(?<![\\\w])log\^\{?2\}?\s*n(?![\w}])", re.IGNORECASE)


def clean_text(line: str) -> str:
    text = line.rstrip("\n")
    stripped = text.strip()
    if stripped.startswith("$$") and stripped.endswith("$$") and stripped.count("$$") >= 2:
        inner = stripped.strip("$").strip()
        inner = inner.replace("$", " ")
        inner = re.sub(r"\s+", " ", inner).strip()
        if not inner:
            return "$$ $$"
        return f"$$ {inner} $$"
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
    text = apply_formula_templates(text)
    text = apply_formatting_templates(text)
    text = normalize_fragmented_math(text)
    text = format_inline_math(text)
    text = normalize_headings(text)
    text = normalize_layout_marks(text)
    text = cleanup_stray_markers(text)
    text = recover_html_tokens(text)
    text = wrap_units_outside_math(text)
    text = wrap_units_inside_math(text)
    text = normalize_bit_unit_notation(text)
    text = normalize_log_notation(text)
    text = sanitize_media_paths(text)
    text = strip_backrefs(text)

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


def wrap_units_outside_math(text: str) -> str:
    def repl(match: Match[str]) -> str:
        unit = match.group("unit")
        return f"$\\text{{{unit}}}$"

    return UNIT_TOKEN_OUTSIDE_PATTERN.sub(repl, text)


def wrap_units_inside_math(text: str) -> str:
    def repl(match: Match[str]) -> str:
        expr = match.group("expr")
        if "\\text{" in expr:
            return match.group(0)
        expr = UNIT_TOKEN_PATTERN.sub(lambda m: f"\\text{{{m.group(0)}}}", expr)
        return f"${expr}$"

    return INLINE_ANY_PATTERN.sub(repl, text)


def normalize_bit_unit_notation(text: str) -> str:
    """崩れた \\text{\\text{$\\text{ビット}$}} などをプレーンな『ビット』に統一する。"""

    replacements = (
        r"\text{\text{\text{\text{$\text{ビット}$}}}}",
        r"\text{\text{\text{$\text{ビット}$}}}",
        r"\text{\text{$\text{ビット}$}}",
        r"\text{$\text{ビット}$}",
        r"$\text{ビット}$",
        r"\text{ビット}",
    )
    prev = None
    while prev != text:
        prev = text
        for pat in replacements:
            text = text.replace(pat, "ビット")
        text = BIT_TEXT_NESTED_PATTERN.sub("ビット", text)
        text = BIT_TEXT_INNER_PATTERN.sub("ビット", text)
        text = BIT_TEXT_SIMPLE_PATTERN.sub("ビット", text)
    return text


def normalize_log_notation(text: str) -> str:
    text = LOG_PATTERN_INLINE.sub(r"$\\log_2 n$", text)
    text = LOG_PATTERN_BARE.sub(r"$\\log_2 n$", text)
    return text


def apply_formula_templates(text: str) -> str:
    templates = load_formula_templates()
    if not templates:
        return text
    block_match = re.fullmatch(r"\s*\$\$\s*(.*?)\s*\$\$\s*", text, re.DOTALL)
    inline_match = re.fullmatch(r"\s*\$(.*?)\$\s*", text)

    if block_match:
        inner = block_match.group(1)
        inner = _apply_templates_to_segment(inner, templates)
        return f"$$ {inner.strip()} $$"
    if inline_match:
        inner = inline_match.group(1)
        inner = _apply_templates_to_segment(inner, templates)
        return f"${inner.strip()}$"
    return _apply_templates_to_segment(text, templates)


def _apply_templates_to_segment(segment: str, templates: list[FormulaTemplate]) -> str:
    for template in templates:
        def repl(match: Match[str]) -> str:
            groups = {key: (match.group(key) or "").strip() for key in match.re.groupindex}
            try:
                return template.replacement.format(**groups)
            except KeyError:
                return match.group(0)

        segment = template.pattern.sub(repl, segment)
    return segment


def apply_formatting_templates(text: str) -> str:
    templates = load_formatting_templates()
    if not templates:
        return text
    return _apply_templates_to_segment(text, templates)


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


def cleanup_nested_block_dollars(text: str) -> str:
    def repl(match: Match[str]) -> str:
        body = match.group(1).replace("$", " ")
        body = re.sub(r"\s+", " ", body).strip()
        if not body:
            return "$$ $$"
        return f"$$ {body} $$"

    return BLOCK_NESTED_DOLLAR_PATTERN.sub(repl, text)


def cleanup_block_trailing_digits(text: str) -> str:
    return BLOCK_TRAILING_DIGIT_PATTERN.sub(r"\1", text)


def cleanup_stray_markers(text: str) -> str:
    """Remove stray regex backreferences left around media tags and <br>."""

    text = STRAY_MARKER_BEFORE_MEDIA.sub("", text)
    text = STRAY_MARKER_AFTER_MEDIA.sub(r"\1", text)
    return text


def strip_backrefs(text: str) -> str:
    """Remove lingering \g or \g<1> tokens that survived other cleaners."""

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
    cleaned = cleanup_nested_block_dollars(cleaned)
    cleaned = cleanup_block_trailing_digits(cleaned)
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
FORMULA_TEMPLATE_PATH = Path(__file__).with_name("formula_templates.json")
FORMATTING_TEMPLATE_PATH = Path(__file__).with_name("formatting_templates.json")


@dataclass
class FormulaTemplate:
    name: str
    pattern: re.Pattern[str]
    replacement: str


_FORMULA_TEMPLATES: list[FormulaTemplate] | None = None
_FORMATTING_TEMPLATES: list[FormulaTemplate] | None = None


def load_formula_templates() -> list[FormulaTemplate]:
    global _FORMULA_TEMPLATES
    if _FORMULA_TEMPLATES is not None:
        return _FORMULA_TEMPLATES

    templates: list[FormulaTemplate] = []
    try:
        data = json.loads(FORMULA_TEMPLATE_PATH.read_text(encoding="utf-8"))
    except FileNotFoundError:
        _FORMULA_TEMPLATES = []
        return _FORMULA_TEMPLATES
    except json.JSONDecodeError:
        _FORMULA_TEMPLATES = []
        return _FORMULA_TEMPLATES

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
        templates.append(FormulaTemplate(entry.get("name", ""), pattern, replacement))

    _FORMULA_TEMPLATES = templates
    return _FORMULA_TEMPLATES


def load_formatting_templates() -> list[FormulaTemplate]:
    global _FORMATTING_TEMPLATES
    if _FORMATTING_TEMPLATES is not None:
        return _FORMATTING_TEMPLATES

    templates: list[FormulaTemplate] = []
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
        templates.append(FormulaTemplate(entry.get("name", ""), pattern, replacement))

    _FORMATTING_TEMPLATES = templates
    return _FORMATTING_TEMPLATES


if __name__ == "__main__":
    main()
