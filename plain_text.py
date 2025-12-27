from __future__ import annotations

import re

IMG_MD_PATTERN = re.compile(r"!\[[^\]]*\]\((?P<url>[^)]+)\)")
IMG_HTML_PATTERN = re.compile(r"<img[^>]*src=\"(?P<url>[^\"]+)\"[^>]*>", re.IGNORECASE)
LINK_MD_PATTERN = re.compile(r"\[(?P<text>[^\]]+)\]\([^)]+\)")

CODE_FENCE_OPEN_PATTERN = re.compile(r"^```[^\n]*$", re.MULTILINE)
CODE_FENCE_CLOSE_PATTERN = re.compile(r"^```$", re.MULTILINE)

HEADING_PATTERN = re.compile(r"^\s{0,3}#{1,6}\s+", re.MULTILINE)
BLOCKQUOTE_PATTERN = re.compile(r"^\s{0,3}>\s?", re.MULTILINE)
HR_PATTERN = re.compile(r"^\s{0,3}(-{3,}|\*{3,}|_{3,})\s*$", re.MULTILINE)

INLINE_CODE_PATTERN = re.compile(r"`([^`]+)`")
EM_STRONG_PATTERN = re.compile(r"\*\*([^*]+)\*\*")
EM_STRONG_UNDER_PATTERN = re.compile(r"__([^_]+)__")
EM_PATTERN = re.compile(r"\*([^*]+)\*")
EM_UNDER_PATTERN = re.compile(r"_([^_]+)_")

TABLE_DIVIDER_PATTERN = re.compile(r"^\s*\|?(?:\s*:?[-]{2,}:?\s*\|)+\s*:?[-]{2,}:?\s*\|?\s*$")
TABLE_ROW_PATTERN = re.compile(r"^\s*\|.+\|\s*$")

TEX_BLOCK_INLINE_PATTERN = re.compile(r"\$\$(?P<body>[\s\S]+?)\$\$")
TEX_INLINE_PATTERN = re.compile(r"\$(?P<body>[^$]+)\$")
TEX_PAREN_PATTERN = re.compile(r"\\\((?P<body>[\s\S]+?)\\\)")
TEX_BRACKET_PATTERN = re.compile(r"\\\[(?P<body>[\s\S]+?)\\\]")


def _strip_math_delimiters(text: str) -> str:
    if not text:
        return ""

    prev = None
    while prev != text:
        prev = text
        text = TEX_BLOCK_INLINE_PATTERN.sub(lambda m: m.group("body").strip(), text)
        text = TEX_PAREN_PATTERN.sub(lambda m: m.group("body").strip(), text)
        text = TEX_BRACKET_PATTERN.sub(lambda m: m.group("body").strip(), text)
        text = TEX_INLINE_PATTERN.sub(lambda m: m.group("body").strip(), text)
    return text


def _md_table_row_to_tsv(line: str) -> str:
    stripped = line.strip()
    if stripped.startswith("|"):
        stripped = stripped[1:]
    if stripped.endswith("|"):
        stripped = stripped[:-1]
    cells = [c.strip() for c in stripped.split("|")]
    return "\t".join(cells).rstrip()


def to_plain_text(md: str) -> str:
    """Markdown-ish text -> plain text (preview/xlsx/csv 用).

    - 画像: `[画像: path]`
    - 表: `|a|b|` -> `a\\tb`（区切り行は除去）
    - 数式: `$...$` / `$$...$$` / `\\(...\\)` / `\\[...\\]` は囲いだけ除去
    """

    text = md or ""

    # normalize <br> early
    text = text.replace("<br>", "\n")

    # code fences: keep contents, drop markers
    text = CODE_FENCE_OPEN_PATTERN.sub("", text)
    text = CODE_FENCE_CLOSE_PATTERN.sub("", text)

    # images: markdown/html -> [画像: url]
    text = IMG_HTML_PATTERN.sub(lambda m: f"[画像: {m.group('url')}]", text)
    text = IMG_MD_PATTERN.sub(lambda m: f"[画像: {m.group('url')}]", text)

    # links: [text](url) -> text
    text = LINK_MD_PATTERN.sub(lambda m: m.group("text"), text)

    # headings / blockquotes / hr
    text = HEADING_PATTERN.sub("", text)
    text = BLOCKQUOTE_PATTERN.sub("", text)
    text = HR_PATTERN.sub("", text)

    # tables: convert row lines to TSV (drop divider)
    lines = text.splitlines()
    converted: list[str] = []
    for line in lines:
        if TABLE_DIVIDER_PATTERN.match(line):
            continue
        if TABLE_ROW_PATTERN.match(line) and line.count("|") >= 2:
            converted.append(_md_table_row_to_tsv(line))
            continue
        converted.append(line)
    text = "\n".join(converted)

    # inline code & emphasis markers
    text = INLINE_CODE_PATTERN.sub(lambda m: m.group(1), text)
    text = EM_STRONG_PATTERN.sub(lambda m: m.group(1), text)
    text = EM_STRONG_UNDER_PATTERN.sub(lambda m: m.group(1), text)
    text = EM_PATTERN.sub(lambda m: m.group(1), text)
    text = EM_UNDER_PATTERN.sub(lambda m: m.group(1), text)

    # strip math delimiters last (after link/image handling)
    text = _strip_math_delimiters(text)

    # drop remaining html tags
    text = re.sub(r"</?[^>]+>", "", text)

    # normalize blank lines
    text = re.sub(r"\n{3,}", "\n\n", text).strip()
    return text

