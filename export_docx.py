from __future__ import annotations

import re
import sys
from pathlib import Path

from docx import Document
from docx.shared import Inches, Cm, Mm
from docx.oxml import OxmlElement
from latex2mathml.converter import convert as latex_to_mathml
from lxml import etree

"""
使い方:
    poetry run python export_docx.py          # merged.md -> merged.docx
    poetry run python export_docx.py foo.md   # foo.md   -> foo.docx
"""

TABLE_RULE = re.compile(r"^:?-{3,}:?$")
IMG_HTML_PATTERN = re.compile(r"<img[^>]*src=\"([^\"]+)\"[^>]*>")
IMG_MD_PATTERN = re.compile(r"!\[[^\]]*\]\(([^)]+)\)")
WIDTH_PATTERN = re.compile(r"width\s*=\s*\"?([0-9]+(?:\.[0-9]+)?)(px|cm|mm)?\"?")
INLINE_LATEX_PATTERN = re.compile(r"\\\(|\\\)|\\\[|\\\]")

OMML_NS = "http://schemas.openxmlformats.org/officeDocument/2006/math"
XML_NS = "http://www.w3.org/XML/1998/namespace"
MATHML_NS = "http://www.w3.org/1998/Math/MathML"

ALLOWED_FORMULA_CHARS = set("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+-*/=×÷().,^_:%$ {}\\")
ESCAPED_SYMBOLS = {
    r"\-": "-",
    r"\+": "+",
    r"\=": "=",
    r"\(": "(",
    r"\)": ")",
    r"\{": "{",
    r"\}": "}",
}
SIMPLE_LATEX_REPLACEMENTS = {
    r"\times": "×",
    r"\cdot": "·",
    r"\div": "÷",
    r"\pm": "±",
    r"\le": "≤",
    r"\ge": "≥",
    r"\neq": "≠",
}
TEXT_CMD_PATTERN = re.compile(r"\\text\{([^}]*)\}")
PLAIN_FRAC_PATTERN = re.compile(r"\\frac\{([^{}]+)\}\{([^{}]+)\}")


def normalize_math_markers(text: str) -> str:
    if not text:
        return ""
    result = INLINE_LATEX_PATTERN.sub(
        lambda m: "$$" if m.group(0) in {r"\[", r"\]"} else "$",
        text,
    )
    result = re.sub(r"\$\s+\$", "$$", result)
    result = re.sub(r"\$\$\s+\$\$", "$$ $$", result)
    return result


def extract_inline_block(text: str) -> str | None:
    stripped = text.strip()
    if not stripped.startswith("$$") or not stripped.endswith("$$"):
        return None
    inner = stripped[2:-2].strip()
    return inner if inner else None


def read_markdown(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    return text.replace("\r\n", "\n").splitlines()


def is_table_line(line: str) -> bool:
    stripped = line.strip()
    return stripped.startswith("|") and stripped.count("|") >= 2


def split_table_line(line: str) -> list[str]:
    stripped = line.strip()
    if stripped.startswith("|"):
        stripped = stripped[1:]
    if stripped.endswith("|"):
        stripped = stripped[:-1]
    return [cell.strip() for cell in stripped.split("|")]


def looks_like_divider(row: list[str]) -> bool:
    return row and all(TABLE_RULE.match(cell.replace(" ", "")) for cell in row)


def is_placeholder_row(row: list[str]) -> bool:
    for cell in row:
        stripped = cell.replace("<br>", "").strip()
        if stripped and stripped not in {"-", "–", "—"}:
            return False
    return True


def add_table(document: Document, table_lines: list[str]) -> None:
    rows = [split_table_line(line) for line in table_lines if line.strip()]
    if not rows:
        return

    cleaned = []
    for row in rows:
        if looks_like_divider(row):
            continue
        if is_placeholder_row(row):
            continue
        cleaned.append(row)

    if not cleaned:
        cleaned = rows

    cols = max(len(row) for row in cleaned)
    table = document.add_table(rows=len(cleaned), cols=cols)
    table.style = "Table Grid"

    for r_idx, row in enumerate(cleaned):
        for c_idx in range(cols):
            value = row[c_idx] if c_idx < len(row) else ""
            text = value.replace("<br>", "\n").strip()
            if text == "-":
                text = ""
            table.rows[r_idx].cells[c_idx].text = text


def flush_paragraph(document: Document, buffer: list[str], base_dir: Path | None = None) -> None:
    if not buffer:
        return
    text = " ".join(buffer).strip()
    buffer.clear()
    if not text:
        return
    paragraph = document.add_paragraph()
    render_inline_content(paragraph, text)


def split_inline_math_segments(text: str) -> list[tuple[str, str]]:
    segments: list[tuple[str, str]] = []
    current: list[str] = []
    mode = "text"
    i = 0
    length = len(text)

    def append_segment(kind: str, value: str) -> None:
        if not value:
            return
        if kind == "text" and segments and segments[-1][0] == "text":
            segments[-1] = ("text", segments[-1][1] + value)
        else:
            segments.append((kind, value))

    def flush_text_buffer() -> None:
        nonlocal current
        if current:
            append_segment("text", "".join(current))
            current = []

    while i < length:
        ch = text[i]
        if ch == "$":
            if i + 1 < length and text[i + 1] == "$":
                current.append("$")
                current.append("$")
                i += 2
                continue

            backslashes = 0
            j = i - 1
            while j >= 0 and text[j] == "\\":
                backslashes += 1
                j -= 1
            if backslashes % 2 == 1:
                current.append("$")
                i += 1
                continue

            if mode == "text":
                flush_text_buffer()
                mode = "math"
                current = []
                i += 1
                continue

            latex = "".join(current).strip()
            current = []
            if latex:
                append_segment("math", latex)
            mode = "text"
            i += 1
            continue

        current.append(ch)
        i += 1

    if mode == "math":
        append_segment("text", "$" + "".join(current))
    else:
        flush_text_buffer()

    return segments


def render_inline_content(paragraph, text: str) -> None:
    working = text.replace("<br>", "\n")
    for kind, value in split_inline_math_segments(working):
        if not value:
            continue
        if kind == "text":
            paragraph.add_run(value)
        else:
            if not append_math_element(paragraph, value, inline=True):
                paragraph.add_run(format_plain_latex_text(value))


def to_width(value: str | None):
    if not value:
        return None
    match = WIDTH_PATTERN.search(value)
    if not match:
        return None
    amount = float(match.group(1))
    unit = match.group(2) or "px"
    if unit == "cm":
        return Cm(amount)
    if unit == "mm":
        return Mm(amount)
    # px ベース（96dpi）
    return Inches(amount / 96)


def add_image(document: Document, base_dir: Path, src: str, width_token: str | None = None) -> None:
    src = src.strip()
    if src.startswith("./"):
        src_path = (base_dir / src[2:]).resolve()
    else:
        src_path = (base_dir / src).resolve() if not Path(src).is_absolute() else Path(src)

    if not src_path.exists():
        document.add_paragraph(f"[画像が見つかりません: {src}]")
        return

    paragraph = document.add_paragraph()
    run = paragraph.add_run()
    width = to_width(width_token)
    kwargs = {"width": width} if width else {}
    run.add_picture(str(src_path), **kwargs)


def add_math_block(document: Document, latex: str) -> None:
    latex = latex.strip()
    if not latex:
        return
    paragraph = document.add_paragraph()
    if not append_math_element(paragraph, latex, inline=False):
        paragraph.add_run(format_plain_latex_text(latex))


def append_math_element(paragraph, latex: str, inline: bool) -> bool:
    element = latex_to_omml_element(latex, inline=inline)
    if element is None:
        return False
    if inline:
        run = paragraph.add_run()
        run._r.append(element)
    else:
        paragraph._p.append(element)
    return True


def latex_to_omml_element(latex: str, inline: bool) -> OxmlElement | None:
    latex = latex.strip()
    if not latex:
        return None
    try:
        mathml = latex_to_mathml(latex)
    except Exception:
        return None
    try:
        root = etree.fromstring(mathml.encode("utf-8"))
    except etree.XMLSyntaxError:
        return None

    omml_children = convert_mathml_children(root)
    if not omml_children:
        return None

    math_element = OxmlElement("m:oMath")
    for child in omml_children:
        math_element.append(child)

    if inline:
        return math_element
    math_para = OxmlElement("m:oMathPara")
    math_para.append(math_element)
    return math_para


def convert_mathml_children(node) -> list[OxmlElement]:
    tag = strip_namespace(node.tag)
    if tag == "semantics":
        for child in node:
            if strip_namespace(child.tag).startswith("annotation"):
                continue
            return convert_mathml_children(child)
        return []
    if tag in {"math", "mrow", "mstyle"}:
        elements: list[OxmlElement] = []
        if node.text and node.text.strip():
            elements.extend(text_to_math_runs(node.text.strip()))
        for child in node:
            if strip_namespace(child.tag).startswith("annotation"):
                continue
            elements.extend(convert_mathml_children(child))
            if child.tail and child.tail.strip():
                elements.extend(text_to_math_runs(child.tail.strip()))
        return elements
    return convert_mathml_node(node)


def convert_mathml_node(node) -> list[OxmlElement]:
    tag = strip_namespace(node.tag)

    if tag in {"math", "mrow", "mstyle"}:
        return convert_mathml_children(node)
    if tag == "semantics":
        return convert_mathml_children(node)
    if tag in {"mi", "mn", "mo", "mtext", "mspace"}:
        text = (node.text or "").strip()
        if not text:
            return []
        return text_to_math_runs(text)
    if tag == "mfrac":
        children = list(iter_math_children(node))
        if len(children) < 2:
            return []
        frac = OxmlElement("m:f")
        num = wrap_math_container("m:num", convert_mathml_node(children[0]))
        den = wrap_math_container("m:den", convert_mathml_node(children[1]))
        frac.append(num)
        frac.append(den)
        return [frac]
    if tag == "msup":
        children = list(iter_math_children(node))
        sup = OxmlElement("m:sSup")
        base = wrap_math_container("m:e", convert_mathml_node(children[0]) if len(children) > 0 else [])
        power = wrap_math_container("m:sup", convert_mathml_node(children[1]) if len(children) > 1 else [])
        sup.append(base)
        sup.append(power)
        return [sup]
    if tag == "msub":
        children = list(iter_math_children(node))
        sub = OxmlElement("m:sSub")
        base = wrap_math_container("m:e", convert_mathml_node(children[0]) if len(children) > 0 else [])
        lower = wrap_math_container("m:sub", convert_mathml_node(children[1]) if len(children) > 1 else [])
        sub.append(base)
        sub.append(lower)
        return [sub]
    if tag == "msubsup":
        children = list(iter_math_children(node))
        node_el = OxmlElement("m:sSubSup")
        base = wrap_math_container("m:e", convert_mathml_node(children[0]) if len(children) > 0 else [])
        lower = wrap_math_container("m:sub", convert_mathml_node(children[1]) if len(children) > 1 else [])
        upper = wrap_math_container("m:sup", convert_mathml_node(children[2]) if len(children) > 2 else [])
        node_el.append(base)
        node_el.append(lower)
        node_el.append(upper)
        return [node_el]
    if tag == "msqrt":
        children = list(iter_math_children(node))
        if not children:
            return []
        rad = OxmlElement("m:rad")
        body = wrap_math_container("m:e", convert_mathml_node(children[0]))
        rad.append(body)
        return [rad]
    if tag == "mroot":
        children = list(iter_math_children(node))
        if not children:
            return []
        rad = OxmlElement("m:rad")
        if len(children) > 1:
            degree = wrap_math_container("m:deg", convert_mathml_node(children[1]))
            rad.append(degree)
        body = wrap_math_container("m:e", convert_mathml_node(children[0]))
        rad.append(body)
        return [rad]
    if tag == "mfenced":
        return convert_mfenced(node)

    elements: list[OxmlElement] = []
    for child in iter_math_children(node):
        elements.extend(convert_mathml_node(child))
    return elements


def convert_mfenced(node) -> list[OxmlElement]:
    open_char = node.get("open", "(") or "("
    close_char = node.get("close", ")") or ")"
    separators = (node.get("separators") or ",")
    children = list(iter_math_children(node))
    elements: list[OxmlElement] = []
    if open_char.strip():
        elements.extend(text_to_math_runs(open_char.strip()))
    for idx, child in enumerate(children):
        elements.extend(convert_mathml_node(child))
        if idx < len(children) - 1 and separators:
            elements.extend(text_to_math_runs(separators[0]))
    if close_char.strip():
        elements.extend(text_to_math_runs(close_char.strip()))
    return elements


def iter_math_children(node):
    for child in node:
        if strip_namespace(child.tag).startswith("annotation"):
            continue
        yield child


def strip_namespace(tag: str) -> str:
    return tag.split("}")[-1] if "}" in tag else tag


def wrap_math_container(tag: str, children: list[OxmlElement]) -> OxmlElement:
    element = OxmlElement(tag)
    content = children if children else [create_math_run(" ")]
    for child in content:
        element.append(child)
    return element


def format_plain_latex_text(latex: str) -> str:
    if not latex:
        return ""
    text = TEXT_CMD_PATTERN.sub(lambda m: m.group(1), latex)
    text = PLAIN_FRAC_PATTERN.sub(lambda m: f"({m.group(1)})/({m.group(2)})", text)
    for src, dst in SIMPLE_LATEX_REPLACEMENTS.items():
        text = text.replace(src, dst)
    text = re.sub(r"\\[a-zA-Z]+", "", text)
    text = text.replace("{", "").replace("}", "")
    return text.strip() or latex


def text_to_math_runs(text: str) -> list[OxmlElement]:
    if not text:
        return []
    if not text.strip():
        return []
    return [create_math_run(text)]


def create_math_run(text: str) -> OxmlElement:
    run = OxmlElement("m:r")
    t = OxmlElement("m:t")
    if text.startswith(" ") or text.endswith(" "):
        t.set(f"{{{XML_NS}}}space", "preserve")
    t.text = text
    run.append(t)
    return run


def convert_markdown(document: Document, lines: list[str], base_dir: Path) -> None:
    paragraph_buffer: list[str] = []
    i = 0

    while i < len(lines):
        raw_line = lines[i].rstrip("\n")
        normalized = normalize_math_markers(raw_line.strip())

        if normalized == "$$":
            flush_paragraph(document, paragraph_buffer, base_dir)
            block_lines: list[str] = []
            j = i + 1
            closed = False
            while j < len(lines):
                candidate_raw = lines[j].rstrip("\n")
                candidate_norm = normalize_math_markers(candidate_raw.strip())
                if candidate_norm == "$$":
                    closed = True
                    break
                block_lines.append(candidate_norm)
                j += 1
            if closed:
                latex = "\n".join(block_lines).strip()
                if latex:
                    add_math_block(document, latex)
                i = j + 1
                continue
            else:
                # 閉じ記号が見つからなければテキストとして扱う
                paragraph_buffer.append(normalized)
                i += 1
                continue

        single_line_block = extract_inline_block(normalized)
        if single_line_block is not None:
            flush_paragraph(document, paragraph_buffer, base_dir)
            add_math_block(document, single_line_block)
            i += 1
            continue

        if not normalized:
            flush_paragraph(document, paragraph_buffer, base_dir)
            i += 1
            continue

        if normalized.startswith("#"):
            flush_paragraph(document, paragraph_buffer, base_dir)
            level = len(normalized) - len(normalized.lstrip("#"))
            heading_text = normalized[level:].strip()
            document.add_heading(heading_text or " ", level=min(level, 4))
            i += 1
            continue

        if is_table_line(raw_line):
            flush_paragraph(document, paragraph_buffer, base_dir)
            table_block = []
            while i < len(lines) and is_table_line(lines[i]):
                table_block.append(lines[i])
                i += 1
            add_table(document, table_block)
            continue

        if normalized.startswith("- ") or normalized.startswith("* "):
            flush_paragraph(document, paragraph_buffer, base_dir)
            paragraph = document.add_paragraph(style="List Bullet")
            render_inline_content(paragraph, normalized[2:].strip())
            i += 1
            continue

        ordered_match = re.match(r"(\d+)[\.\)]\s+(.*)", normalized)
        if ordered_match:
            flush_paragraph(document, paragraph_buffer, base_dir)
            number_text = ordered_match.group(1)
            body_text = ordered_match.group(2).strip()
            paragraph = document.add_paragraph()
            paragraph.add_run(f"{number_text}. ")
            render_inline_content(paragraph, body_text)
            i += 1
            continue

        if "<img" in normalized or "![" in normalized:
            flush_paragraph(document, paragraph_buffer, base_dir)
            handled = False
            for match in IMG_HTML_PATTERN.finditer(normalized):
                add_image(document, base_dir, match.group(1), match.group(0))
                handled = True
            stripped_no_html = IMG_HTML_PATTERN.sub("", normalized)
            for match in IMG_MD_PATTERN.finditer(stripped_no_html):
                add_image(document, base_dir, match.group(1))
                handled = True
            remainder = IMG_MD_PATTERN.sub("", stripped_no_html).strip()
            if remainder:
                paragraph_buffer.append(remainder)
            if handled:
                i += 1
                continue

        paragraph_buffer.append(normalized)
        i += 1

    flush_paragraph(document, paragraph_buffer, base_dir)


def convert_file(md_path: Path) -> Path:
    if not md_path.exists():
        raise FileNotFoundError(f"Markdown ファイルが見つかりません: {md_path}")

    docx_path = md_path.with_suffix(".docx")
    document = Document()
    lines = read_markdown(md_path)
    convert_markdown(document, lines, base_dir=md_path.parent)
    document.save(docx_path)
    return docx_path


def main():
    if len(sys.argv) >= 2:
        md_path = Path(sys.argv[1])
    else:
        md_path = Path("merged.md")

    try:
        docx_path = convert_file(md_path)
        print(f"Word ファイルを出力しました: {docx_path}")
    except Exception as e:
        print(f"エラー: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
