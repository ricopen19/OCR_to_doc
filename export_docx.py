import re
import sys
from pathlib import Path

from docx import Document

"""
使い方:
    poetry run python export_docx.py          # merged.md -> merged.docx
    poetry run python export_docx.py foo.md   # foo.md   -> foo.docx
"""

TABLE_RULE = re.compile(r"^:?-{3,}:?$")


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


def convert_markdown(document: Document, lines: list[str]) -> None:
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
    convert_markdown(document, lines)
    document.save(docx_path)

    print(f"Word ファイルを出力しました: {docx_path}")


if __name__ == "__main__":
    main()
