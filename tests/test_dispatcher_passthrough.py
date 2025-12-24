import sys


def test_dispatcher_splits_passthrough_args(monkeypatch):
    import dispatcher

    monkeypatch.setattr(
        sys,
        "argv",
        [
            "dispatcher.py",
            "sample.pdf",
            "--mode",
            "full",
            "--",
            "--chunk-size",
            "10",
            "--end",
            "3",
        ],
    )
    args = dispatcher.parse_args()
    assert args.mode == "full"
    assert args.input_path == "sample.pdf"
    assert args.extra == ["--chunk-size", "10", "--end", "3"]


def test_dispatcher_no_passthrough(monkeypatch):
    import dispatcher

    monkeypatch.setattr(sys, "argv", ["dispatcher.py", "sample.pdf"])
    args = dispatcher.parse_args()
    assert args.extra == []


def test_dispatcher_infers_pdf_output_dir_with_label(tmp_path):
    import dispatcher

    output_root = tmp_path / "result"
    output_root.mkdir()
    expected = output_root / "bosyuu_p3-9"
    expected.mkdir()

    inferred = dispatcher._infer_pdf_output_dir(
        dispatcher.Path("bosyuu.pdf"),
        output_root=output_root,
        extra_args=["--label", "p3-9"],
    )
    assert inferred == expected


def test_dispatcher_infers_pdf_output_dir_with_start_end(tmp_path):
    import dispatcher

    output_root = tmp_path / "result"
    output_root.mkdir()
    expected = output_root / "bosyuu_p9-9"
    expected.mkdir()

    inferred = dispatcher._infer_pdf_output_dir(
        dispatcher.Path("bosyuu.pdf"),
        output_root=output_root,
        extra_args=["--start", "9", "--end", "9"],
    )
    assert inferred == expected
