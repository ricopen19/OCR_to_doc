import re
import sys
from pathlib import Path

from docx import Document
from docx.shared import Inches, Cm, Mm

"""
使い方:
    poetry run python export_docx.py          # merged.md -> merged.docx
    poetry run python export_docx.py foo.md   # foo.md   -> foo.docx
"""

TABLE_RULE = re.compile(r"^:?-{3,}:?$")
IMG_HTML_PATTERN = re.compile(r"<img[^>]*src=\"([^\"]+)\"[^>]*>")
IMG_MD_PATTERN = re.compile(r"!\[[^\]]*\]\(([^)]+)\)")
WIDTH_PATTERN = re.compile(r"width\s*=\s*\"?([0-9]+(?:\.[0-9]+)?)(px|cm|mm)?\"?")


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


def add_table(document: Document, table_lines: list[str]) -> None:
    rows = [split_table_line(line) for line in table_lines if line.strip()]
    if not rows:
        return

    cleaned = []
    for row in rows:
        if looks_like_divider(row):
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
            table.rows[r_idx].cells[c_idx].text = value.replace("<br>", "\n")


def flush_paragraph(document: Document, buffer: list[str]) -> None:
    if not buffer:
        return
    text = " ".join(buffer).strip()
    buffer.clear()
    if not text:
        return
    text = text.replace("<br>", "\n")
    document.add_paragraph(text)


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


def convert_markdown(document: Document, lines: list[str], base_dir: Path) -> None:
    paragraph_buffer: list[str] = []
    i = 0

    while i < len(lines):
        line = lines[i].rstrip()
        stripped = line.strip()

        if not stripped:
            flush_paragraph(document, paragraph_buffer)
            i += 1
            continue

        if stripped.startswith("#"):
            flush_paragraph(document, paragraph_buffer)
            level = len(stripped) - len(stripped.lstrip("#"))
            heading_text = stripped[level:].strip()
            document.add_heading(heading_text or " ", level=min(level, 4))
            i += 1
            continue

        if is_table_line(line):
            flush_paragraph(document, paragraph_buffer)
            table_block = []
            while i < len(lines) and is_table_line(lines[i]):
                table_block.append(lines[i])
                i += 1
            add_table(document, table_block)
            continue

        if stripped.startswith("- ") or stripped.startswith("* "):
            flush_paragraph(document, paragraph_buffer)
            document.add_paragraph(stripped[2:].strip(), style="List Bullet")
            i += 1
            continue

        ordered_match = re.match(r"(\d+)[\.\)]\s+(.*)", stripped)
        if ordered_match:
            flush_paragraph(document, paragraph_buffer)
            document.add_paragraph(ordered_match.group(2).strip(), style="List Number")
            i += 1
            continue

        if "<img" in stripped or "![" in stripped:
            flush_paragraph(document, paragraph_buffer)
            handled = False
            for match in IMG_HTML_PATTERN.finditer(stripped):
                add_image(document, base_dir, match.group(1), match.group(0))
                handled = True
            stripped_no_html = IMG_HTML_PATTERN.sub("", stripped)
            for match in IMG_MD_PATTERN.finditer(stripped_no_html):
                add_image(document, base_dir, match.group(1))
                handled = True
            remainder = IMG_MD_PATTERN.sub("", stripped_no_html).strip()
            if remainder:
                paragraph_buffer.append(remainder)
            if handled:
                i += 1
                continue

        paragraph_buffer.append(stripped)
        i += 1

    flush_paragraph(document, paragraph_buffer)


def main():
    if len(sys.argv) >= 2:
        md_path = Path(sys.argv[1])
    else:
        md_path = Path("merged.md")

    if not md_path.exists():
        print(f"エラー: Markdown ファイルが見つかりません: {md_path}")
        sys.exit(1)

    docx_path = md_path.with_suffix(".docx")

    document = Document()
    lines = read_markdown(md_path)
    convert_markdown(document, lines, base_dir=md_path.parent)
    document.save(docx_path)

    print(f"Word ファイルを出力しました: {docx_path}")


if __name__ == "__main__":
    main()
