def test_rename_figure_assets_overwrites_existing(tmp_path, monkeypatch):
    import ocr

    output_dir = tmp_path / "result"
    figure_dir = output_dir / "figures"
    figure_dir.mkdir(parents=True)

    # 既存の正規化済みファイルがある状態（Windows だと rename が FileExistsError になる）
    stale = figure_dir / "fig_page001_01.png"
    stale.write_text("stale", encoding="utf-8")

    raw = figure_dir / "page_images_page_001_p1_figure_0.png"
    raw.write_text("new", encoding="utf-8")

    md_path = output_dir / "page_001.md"
    md_path.write_text(
        '<img src="figures/page_images_page_001_p1_figure_0.png">',
        encoding="utf-8",
    )

    monkeypatch.setattr(ocr, "remove_icon_figures", lambda *args, **kwargs: None)

    ocr.rename_figure_assets(output_dir, 1, icon_config=None, page_metrics=None)

    assert not raw.exists()
    assert (figure_dir / "fig_page001_01.png").read_text(encoding="utf-8") == "new"
    assert "./figures/fig_page001_01.png" in md_path.read_text(encoding="utf-8")

