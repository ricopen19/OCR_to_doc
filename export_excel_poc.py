from __future__ import annotations

import argparse
import csv
import json
import re
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterable, List

from lxml import html
from openpyxl import Workbook
from openpyxl.styles import Alignment, Border, Side
from openpyxl.utils import get_column_letter
from openpyxl.worksheet.table import Table, TableStyleInfo
from openpyxl.worksheet.datavalidation import DataValidation


@dataclass
class TableCell:
    row: int
    col: int
    text: str = ""
    row_span: int = 1
    col_span: int = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Convert YomiToku outputs to Excel")
    parser.add_argument("input", type=Path, help="入力ファイル (json/csv/html)")
    parser.add_argument("output", type=Path, help="出力する xlsx パス")
    parser.add_argument(
        "--format",
        choices=["json", "csv", "html"],
        required=True,
        help="入力フォーマット",
    )
    parser.add_argument(
        "--csv-tables-only",
        action="store_true",
        help="CSV の場合、1 列だけの段落ブロックを除外し、複数列ブロックのみを表として扱う",
    )
    parser.add_argument(
        "--sheet-prefix",
        default="table",
        help="シート名のプレフィックス (default: table)",
    )
    parser.add_argument(
        "--meta",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="メタ情報シートを出力するか (default: true)",
    )
    parser.add_argument(
        "--review-columns",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="表の右隣にレビュー用列（確認メモ/ステータス）を追加する",
    )
    parser.add_argument(
        "--auto-format",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="数値/日付/百分率を自動判定して書式設定する (default: true)",
    )
    return parser.parse_args()


def load_tables_from_json(path: Path) -> list[list[TableCell]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    tables = data.get("tables") or []
    result: list[list[TableCell]] = []
    for table in tables:
        cells: list[TableCell] = []
        for cell in table.get("cells", []):
            cells.append(
                TableCell(
                    row=int(cell.get("row", 1)),
                    col=int(cell.get("col", 1)),
                    text=(cell.get("contents") or "").strip(),
                    row_span=max(1, int(cell.get("row_span", 1))),
                    col_span=max(1, int(cell.get("col_span", 1))),
                )
            )
        if cells:
            result.append(cells)
    if not result:
        raise ValueError("JSON 内に table/cell が見つかりません")
    return result


def load_tables_from_csv(path: Path, *, tables_only: bool = False) -> list[list[TableCell]]:
    tables: list[list[TableCell]] = []
    current: list[list[str]] = []
    with path.open(encoding="utf-8") as fp:
        reader = csv.reader(fp)
        for row in reader:
            if not any(cell.strip() for cell in row):
                if current:
                    tables.append(current)
                    current = []
                continue
            current.append(row)
    if current:
        tables.append(current)

    if tables_only:
        # 1 列だけのブロック（段落とみなす）を除外し、複数列を含むブロックだけ残す
        filtered = []
        for block in tables:
            max_cols = max((len(row) for row in block), default=0)
            if max_cols <= 1:
                continue
            filtered.append(block)
        tables = filtered

    result: list[list[TableCell]] = []
    for table in tables:
        cells: list[TableCell] = []
        for r_idx, row in enumerate(table, start=1):
            for c_idx, value in enumerate(row, start=1):
                cells.append(TableCell(row=r_idx, col=c_idx, text=value.strip()))
        if cells:
            result.append(cells)

    if not result:
        raise ValueError("CSV 内に表データが見つかりません")
    return result


def load_tables_from_html(path: Path) -> list[list[TableCell]]:
    content = path.read_text(encoding="utf-8")
    doc = html.fromstring(content or "<html></html>")
    table_elements = doc.findall(".//table")
    if not table_elements:
        raise ValueError("HTML 内に <table> が見つかりません")

    tables: list[list[TableCell]] = []
    for table_el in table_elements:
        cells: list[TableCell] = []
        occupied: dict[tuple[int, int], bool] = {}
        row_index = 0
        for tr in table_el.findall(".//tr"):
            row_index += 1
            col_index = 1
            # スパンで埋まっている座標をスキップ
            while (row_index, col_index) in occupied:
                col_index += 1
            for cell_el in tr.findall(".//th") + tr.findall(".//td"):
                while (row_index, col_index) in occupied:
                    col_index += 1
                text = cell_el.text_content().strip()
                row_span = int(cell_el.get("rowspan", "1") or 1)
                col_span = int(cell_el.get("colspan", "1") or 1)
                cells.append(
                    TableCell(
                        row=row_index,
                        col=col_index,
                        text=text,
                        row_span=max(1, row_span),
                        col_span=max(1, col_span),
                    )
                )
                for r in range(row_index, row_index + row_span):
                    for c in range(col_index, col_index + col_span):
                        occupied[(r, c)] = True
                col_index += col_span
        if cells:
            tables.append(cells)

    return tables


def auto_adjust_columns(ws) -> None:
    """列幅を簡易調整しつつ折り返しを活かす。

    - 短文は固定幅広め（読みやすさ優先）
    - 長文は幅を抑え、wrap_text で折り返す
    """

    for column_cells in ws.columns:
        values = [len(str(cell.value)) for cell in column_cells if cell.value]
        if not values:
            continue
        letter = get_column_letter(column_cells[0].column)
        max_len = max(values)
        if max_len <= 8:
            width = 10
        elif max_len <= 20:
            width = 18
        elif max_len <= 40:
            width = 28
        else:
            width = 40  # 長文は折り返し前提
        ws.column_dimensions[letter].width = width


def split_text_to_paragraphs(text: str) -> list[str]:
    """空行区切りで段落配列にする（Markdown/プレーンテキスト想定）。"""

    paragraphs: list[str] = []
    buffer: list[str] = []
    for line in (text or "").splitlines():
        if line.strip():
            buffer.append(line.rstrip())
            continue
        if buffer:
            paragraphs.append("\n".join(buffer).strip())
            buffer = []
    if buffer:
        paragraphs.append("\n".join(buffer).strip())
    return paragraphs


def write_text_to_workbook(
    paragraphs: list[str],
    *,
    sheet_name: str = "本文",
) -> Workbook:
    wb = Workbook()
    ws = wb.active
    ws.title = sheet_name

    ws.append(["本文"])
    for para in paragraphs:
        ws.append([para])

    ws.freeze_panes = "A2"
    ws.column_dimensions["A"].width = 80
    align = Alignment(vertical="top", wrap_text=True)
    for row in ws.iter_rows(min_row=1, max_col=1):
        row[0].alignment = align

    return wb


PERCENT_RE = re.compile(r"^\s*-?\d+[\d,\.]*\s*%\s*$")
INT_RE = re.compile(r"^\s*-?\d{1,3}(?:,\d{3})*\s*$")
FLOAT_RE = re.compile(r"^\s*-?\d*[\.,]?\d+\s*$")
DATE_RE = [
    "%Y-%m-%d",
    "%Y/%m/%d",
    "%Y.%m.%d",
]


def apply_auto_format(target_cell, text: str, *, enable: bool) -> bool:
    """値を数値/日付/百分率に変換し、number_format を付与する。

    戻り値: True=値をセット済み、False=元の文字列を使うこと。
    """

    if not enable or not isinstance(text, str):
        return False

    raw = text.strip()
    if not raw:
        return False

    # percentage
    if PERCENT_RE.match(raw):
        num_str = raw.replace("%", "").replace(",", "")
        num_str = num_str.replace("％", "")
        try:
            value = float(num_str) / 100.0
        except ValueError:
            return False
        target_cell.value = value
        target_cell.number_format = "0.0%" if "." in num_str else "0%"
        return True

    # integer
    if INT_RE.match(raw):
        try:
            value = int(raw.replace(",", ""))
        except ValueError:
            return False
        target_cell.value = value
        target_cell.number_format = "#,##0"
        return True

    # float
    if FLOAT_RE.match(raw):
        try:
            value = float(raw.replace(",", ""))
        except ValueError:
            return False
        target_cell.value = value
        target_cell.number_format = "#,##0.00"
        return True

    # date
    for fmt in DATE_RE:
        try:
            dt = datetime.strptime(raw, fmt).date()
            target_cell.value = dt
            target_cell.number_format = "yyyy/mm/dd"
            return True
        except ValueError:
            continue

    return False


def write_tables_to_workbook(
    tables: list[list[TableCell]],
    *,
    sheet_prefix: str,
    review_columns: bool,
    auto_format: bool,
) -> Workbook:
    wb = Workbook()
    wb.remove(wb.active)
    align = Alignment(vertical="top", wrap_text=True)
    align_center = Alignment(vertical="center", horizontal="center", wrap_text=True)
    thin = Border(
        left=Side(style="thin"),
        right=Side(style="thin"),
        top=Side(style="thin"),
        bottom=Side(style="thin"),
    )
    for idx, cells in enumerate(tables, start=1):
        ws = wb.create_sheet(title=f"{sheet_prefix}_{idx}")
        max_row = 0
        max_col = 0
        for cell in cells:
            target = ws.cell(row=cell.row, column=cell.col)
            if not apply_auto_format(target, cell.text, enable=auto_format):
                target.value = cell.text
            target.alignment = align
            if cell.row_span > 1 or cell.col_span > 1:
                ws.merge_cells(
                    start_row=cell.row,
                    start_column=cell.col,
                    end_row=cell.row + cell.row_span - 1,
                    end_column=cell.col + cell.col_span - 1,
                )
            max_row = max(max_row, cell.row + cell.row_span - 1)
            max_col = max(max_col, cell.col + cell.col_span - 1)

        table_max_col = max_col  # Table 範囲は元の表のみ

        # レビュー列（表から2列あけてステータスのみ、○/× ドロップダウン）
        if review_columns and max_row > 0:
            status_col = table_max_col + 3  # 2 列スペースを空ける
            ws.cell(row=1, column=status_col).value = "ステータス"
            dv = DataValidation(type="list", formula1='"○,×"', allow_blank=True)
            ws.add_data_validation(dv)
            status_range = f"{get_column_letter(status_col)}2:{get_column_letter(status_col)}{max_row}"
            dv.add(status_range)
            ws.cell(row=1, column=status_col).alignment = align_center
            for r in range(2, max_row + 1):
                ws.cell(row=r, column=status_col).alignment = align_center
            max_col = status_col
        status_col_opt = status_col if review_columns and max_row > 0 else None


        # 罫線: 表本体 (1..table_max_col) とステータス列だけ。間の空列は塗らない。
        border_cols = list(range(1, table_max_col + 1))
        if status_col_opt:
            border_cols.append(status_col_opt)

        for r in range(1, max_row + 1):
            for c in border_cols:
                ws.cell(row=r, column=c).border = thin

        # Excel テーブル化（先頭行をヘッダーと見なす）
        # Excel の Table は結合セルを含められないため、結合セルが存在する場合はスキップする。
        has_merges = bool(ws.merged_cells.ranges)
        if not has_merges and max_row >= 1 and table_max_col >= 1:
            ref = f"A1:{get_column_letter(table_max_col)}{max_row}"
            table = Table(
                displayName=f"{sheet_prefix}_{idx}_tbl",
                ref=ref,
                tableStyleInfo=TableStyleInfo(
                    name="TableStyleMedium2",
                    showFirstColumn=False,
                    showLastColumn=False,
                    showRowStripes=True,
                    showColumnStripes=False,
                ),
            )
            ws.add_table(table)

        auto_adjust_columns(ws)
    return wb


def add_meta_sheet(wb: Workbook, *, args: argparse.Namespace, sheet_names: list[str]) -> None:
    ws = wb.create_sheet(title="meta", index=0)
    ws.append(["項目", "値"])
    ws.append(["入力ファイル", str(args.input)])
    ws.append(["入力フォーマット", args.format])
    ws.append(["出力ファイル", str(args.output)])
    ws.append(["生成日時", datetime.now().strftime("%Y-%m-%d %H:%M:%S")])
    ws.append(["シート数", len(sheet_names)])

    ws.append([])
    ws.append(["シートリンク", ""])
    for name in sheet_names:
        row = ws.max_row + 1
        cell = ws.cell(row=row, column=2)
        cell.value = name
        cell.hyperlink = f"#{name}!A1"
        cell.style = "Hyperlink"

    auto_adjust_columns(ws)


def main() -> None:
    args = parse_args()
    if args.format == "json":
        tables = load_tables_from_json(args.input)
    elif args.format == "csv":
        tables = load_tables_from_csv(args.input, tables_only=args.csv_tables_only)
    else:
        tables = load_tables_from_html(args.input)

    workbook = write_tables_to_workbook(
        tables,
        sheet_prefix=args.sheet_prefix,
        review_columns=args.review_columns,
        auto_format=args.auto_format,
    )
    sheet_names = workbook.sheetnames.copy()
    if args.meta:
        add_meta_sheet(workbook, args=args, sheet_names=sheet_names)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    workbook.save(args.output)
    print(f"Saved {args.output}")


if __name__ == "__main__":
    main()
