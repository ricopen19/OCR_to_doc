from export_excel_poc import TableCell, write_tables_to_workbook


def test_excel_table_mode_splits_by_structure_and_names_sheets() -> None:
    # 2段ヘッダー + データ（神奈川） + 構造変化行（東京セクションのヘッダー） + データ（東京）
    cells = [
        TableCell(row=1, col=1, text="都道府県", row_span=2),
        TableCell(row=1, col=2, text="学校名", row_span=2),
        TableCell(row=1, col=3, text="募集状況", col_span=2),
        TableCell(row=2, col=3, text="国語"),
        TableCell(row=2, col=4, text="数学"),
        TableCell(row=3, col=1, text="神奈川", row_span=2),
        TableCell(row=3, col=2, text="A校"),
        TableCell(row=3, col=3, text="○"),
        TableCell(row=4, col=2, text="B校"),
        TableCell(row=4, col=4, text="○"),
        # 構造が変わる行（セクション開始の見出し）
        TableCell(row=5, col=1, text="東京", row_span=3),
        TableCell(row=5, col=2, text="採用情報", col_span=3),
        # 東京セクションのデータ
        TableCell(row=6, col=2, text="C校"),
        TableCell(row=6, col=3, text="○"),
        TableCell(row=7, col=2, text="D校"),
        TableCell(row=7, col=4, text="○"),
    ]

    wb = write_tables_to_workbook(
        [cells],
        sheet_prefix="Page",
        review_columns=False,
        auto_format=False,
        excel_mode="table",
    )

    assert wb.sheetnames == ["神奈川", "東京"]

    ws1 = wb["神奈川"]
    assert ws1["A2"].value == "神奈川"
    assert ws1["B2"].value == "A校"
    assert ws1["C2"].value == "○"
    assert ws1["D3"].value == "○"
    assert ws1["C1"].value == "募集状況 / 国語"
    assert ws1["D1"].value == "募集状況 / 数学"
    assert len(ws1.tables) == 1

    ws2 = wb["東京"]
    assert ws2["A2"].value == "東京"
    assert ws2["B2"].value == "C校"
    assert len(ws2.tables) == 1

