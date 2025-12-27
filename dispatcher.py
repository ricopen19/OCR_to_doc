"""Entry point that routes inputs (PDF / text PDF / images) to proper handlers."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from ingest import InputKind, IngestError, inspect
from image_normalizer import ImageConversionError, ensure_png_image
from ocr import OcrOptions, run_ocr, export_csv
from export_docx import convert_file
from export_excel_poc import main as export_excel_main, parse_args as parse_excel_args

DEFAULT_OUTPUT_ROOT = Path("result")
CONVERTED_DIR_NAME = "converted"
PREPROCESSED_DIR_NAME = "preprocessed"


def _parse_cli_value(args: list[str] | None, name: str) -> str | None:
    """CLI 引数リストから `--name value` または `--name=value` を抽出する。"""

    if not args:
        return None
    for index, item in enumerate(args):
        if item == name:
            if index + 1 < len(args):
                return args[index + 1]
            return None
        prefix = f"{name}="
        if item.startswith(prefix):
            return item[len(prefix) :]
    return None


def _parse_cli_int(args: list[str] | None, name: str) -> int | None:
    value = _parse_cli_value(args, name)
    if value is None:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def _infer_pdf_output_dir(pdf_path: Path, *, output_root: Path, extra_args: list[str] | None) -> Path:
    """ocr_chanked.py の出力ディレクトリ名ルールに合わせて output_dir を推定する。"""

    stem = pdf_path.stem
    label = _parse_cli_value(extra_args, "--label")
    if label:
        return output_root / f"{stem}_{label}"

    start = _parse_cli_int(extra_args, "--start")
    end = _parse_cli_int(extra_args, "--end")

    candidates: list[Path] = []

    if start is not None or end is not None:
        start_value = start if start is not None else 1
        if end is not None:
            candidates.append(output_root / f"{stem}_p{start_value}-{end}")
        else:
            # end が未指定の場合、ocr_chanked は num_pages を使って suffix を作るため、
            # ここでは実際に作成されたディレクトリを探索して補完する。
            prefix = f"{stem}_p{start_value}-"
            matches = [p for p in output_root.glob(f"{prefix}*") if p.is_dir()]
            if matches:
                matches.sort(key=lambda p: p.stat().st_mtime, reverse=True)
                return matches[0]

    candidates.append(output_root / stem)

    for path in candidates:
        if path.exists():
            return path

    # 最後の保険：stem_ で始まるディレクトリが 1 つでもあれば新しいものを拾う
    matches = [p for p in output_root.glob(f"{stem}_*") if p.is_dir()]
    if matches:
        matches.sort(key=lambda p: p.stat().st_mtime, reverse=True)
        return matches[0]

    return candidates[-1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="入力ファイル自動判定 + OCR 実行")
    parser.add_argument("input_path", help="PDF / 画像ファイル")
    parser.add_argument(
        "--mode",
        choices=["lite", "full"],
        default="lite",
        help="YomiToku の実行モード",
    )
    parser.add_argument("--device", default="cpu", help="YomiToku に渡すデバイス指定")
    parser.add_argument(
        "--output-root",
        type=Path,
        default=DEFAULT_OUTPUT_ROOT,
        help="出力ルート（result/<name>/ にページ Markdown を保存）",
    )
    parser.add_argument(
        "--svg-dpi",
        type=int,
        default=300,
        help="SVG → PNG 変換時の DPI",
    )
    try:
        from image_preprocessor import PROFILE_REGISTRY

        profile_choices = sorted(PROFILE_REGISTRY.keys())
    except Exception:
        # Pillow 未インストールでも PDF ルートだけは動かせるように、既定のプロファイル名だけを許可
        profile_choices = ["ocr_default"]
    parser.add_argument(
        "--ocr-profile",
        choices=profile_choices,
        default="ocr_default",
        help="OCR に渡す前処理プロファイル",
    )
    parser.add_argument(
        "--figure",
        dest="enable_figure",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="YomiToku の --figure オプションを有効/無効",
    )
    parser.add_argument(
        "--image-as-pdf",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="画像入力でも一度 PDF 化してから OCR する（img2pdf 相当、DPI は --image-dpi）",
    )
    parser.add_argument(
        "--image-dpi",
        type=int,
        default=300,
        help="画像→PDF 変換時の DPI（--image-as-pdf 有効時に使用）",
    )
    parser.add_argument(
        "--fallback-tesseract",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="OCR結果が空に近い場合、pytesseract で再OCRするフォールバックを有効化",
    )
    parser.add_argument(
        "--force-tesseract-merge",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="YomiToku 結果に関わらず pytesseract の結果を追記する",
    )
    parser.add_argument(
        "--math-refiner",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="PDF 経路で math refiner を有効化するか",
    )
    parser.add_argument(
        "--formats",
        nargs="+",
        default=["md"],
        help="出力フォーマット (md, docx, etc.)",
    )
    parser.add_argument(
        "--docx-math",
        choices=["text", "image"],
        default="text",
        help="docx 出力時の数式の扱い。text=本文としてそのまま、image=検出した数式領域を画像で貼る",
    )
    parser.add_argument(
        "--excel-mode",
        choices=["layout", "table"],
        default="layout",
        help="表出力モード（xlsx/csv）。layout=レイアウト優先、table=結合解除してテーブル化 (default: layout)",
    )
    parser.add_argument(
        "--excel-meta",
        dest="excel_meta_sheet",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="xlsx 出力時にメタ情報シートを付与する",
    )
    parser.add_argument(
        "--crop",
        help="正規化トリミング範囲（left,top,width,height / 0〜1）。PDF/画像どちらにも適用されます。",
    )
    # `dispatcher.py <input> -- <ocr_chanked.py args...>` の形式で PDF 向け引数を透過させる。
    # argparse の parse_known_args だと区切り `--` 自体も extra に混ざり、
    # そのまま ocr_chanked.py に渡すと argparse がオプション解析を停止してしまうため、
    # 明示的に `--` で分割してから parse_args する。
    argv = sys.argv[1:]
    passthrough: list[str] = []
    if "--" in argv:
        sep_index = argv.index("--")
        known_argv = argv[:sep_index]
        passthrough = argv[sep_index + 1 :]
    else:
        known_argv = argv

    args = parser.parse_args(known_argv)
    args.extra = passthrough
    return args


def _append_force_flags(extra: list[str] | None, fallback: bool, force: bool) -> list[str]:
    extra = list(extra) if extra else []
    if fallback and "--fallback-tesseract" not in extra:
        extra.append("--fallback-tesseract")
    if force and "--force-tesseract-merge" not in extra:
        extra.append("--force-tesseract-merge")
    return extra


def run(
    path: Path,
    *,
    mode: str = "lite",
    device: str = "cpu",
    output_root: Path = DEFAULT_OUTPUT_ROOT,
    svg_dpi: int = 300,
    enable_figure: bool = True,
    use_math_refiner: bool = False,
    extra_pdf_args: list[str] | None = None,
    ocr_profile: str = "ocr_default",
    image_as_pdf: bool = False,
    image_dpi: int = 300,
    fallback_tesseract: bool = False,
    force_tesseract_merge: bool = False,
    formats: list[str] | None = None,
    docx_math: str = "text",
    crop: str | None = None,
    excel_mode: str = "layout",
    excel_meta_sheet: bool = True,
) -> Path:
    formats = formats or ["md"]
    meta = inspect(path)
    output_dir = None
    needs_json = ("xlsx" in formats or "csv" in formats) or ("docx" in formats and docx_math == "image")
    if meta.is_pdf:
        _run_pdf(
            meta.path,
            mode=mode,
            device=device,
            use_math_refiner=use_math_refiner,
            output_root=output_root,
            extra_args=_append_force_flags(extra_pdf_args, fallback_tesseract, force_tesseract_merge),
            force_tesseract_merge=force_tesseract_merge,
            emit_csv=False,  # CSV is no longer needed for Excel, assuming user didn't ask explicitly for CSV
            emit_json=needs_json,
            crop=crop,
        )
        output_dir = _infer_pdf_output_dir(meta.path, output_root=output_root, extra_args=extra_pdf_args)
    elif meta.is_image:
        output_dir = _run_image(
            meta.path,
            mode=mode,
            device=device,
            output_root=output_root,
            svg_dpi=svg_dpi,
            enable_figure=enable_figure,
            ocr_profile=ocr_profile,
            image_as_pdf=image_as_pdf,
            image_dpi=image_dpi,
            extra_pdf_args=extra_pdf_args,
            fallback_tesseract=fallback_tesseract,
            force_tesseract_merge=force_tesseract_merge,
            emit_csv=False,
            emit_json=needs_json,
            crop=crop,
        )
    else:
        raise IngestError(f"未対応の入力種別です: {path}")

    # Post-processing for formats
    if "docx" in formats and output_dir:
        # Try to find the markdown file to convert
        # 1. Merged markdown (PDF or image-as-pdf)
        merged_md = output_dir / f"{output_dir.name}_merged.md"
        if merged_md.exists():
            print(f"[dispatcher] Converting to docx: {merged_md}")
            convert_file(merged_md, math_mode=docx_math)
        else:
            # 2. Single page markdown (Image)
            # For single image, it might be page_001.md. 
            # We should rename it to match the stem so collect_output_files finds it,
            # or just convert what we find.
            # But collect_output_files looks for {stem}.docx or {stem}_merged.docx
            
            # If we have page_001.md, let's rename it to {stem}.md if it doesn't exist
            page_md = output_dir / "page_001.md"
            target_md = output_dir / f"{path.stem}.md"
            
            if page_md.exists():
                # Rename for consistency if it's the only file
                # But be careful not to overwrite if we processed multiple images?
                # _run_image processes one image.
                if not target_md.exists():
                    import shutil

                    shutil.copy(page_md, target_md)
                    print(f"[dispatcher] Copied {page_md} to {target_md}")

                md_to_convert = target_md if target_md.exists() else page_md
                convert_file(md_to_convert, math_mode=docx_math)
                print(f"[dispatcher] Converting to docx: {md_to_convert}")

    if "xlsx" in formats and output_dir:
        # json -> xlsx
        print("[dispatcher] processing excel_via=json")
        _convert_to_excel(output_dir, output_root, excel_mode=excel_mode, excel_meta_sheet=excel_meta_sheet)

    if "csv" in formats and output_dir:
        print("[dispatcher] processing csv_via=json")
        _convert_to_csv(output_dir, excel_mode=excel_mode)

    return output_dir


def _run_pdf(
    pdf_path: Path,
    *,
    mode: str,
    device: str,
    use_math_refiner: bool,
    output_root: Path,
    extra_args: list[str] | None,
    force_tesseract_merge: bool,
    emit_csv: bool = False,
    emit_json: bool = False,
    crop: str | None = None,
) -> None:
    script = Path(__file__).resolve().parent / "ocr_chanked.py"
    cmd = [
        sys.executable,
        "-u",
        str(script),
        str(pdf_path),
        "--mode",
        mode,
        "--device",
        device,
        "--output-root",
        str(output_root),
    ]
    # ocr_chanked.py 側は --math-refiner オプションのみ持つため、有効時だけ付与する
    if use_math_refiner:
        cmd.append("--math-refiner")
    if emit_csv:
        cmd.append("--emit-csv")
    if emit_json:
        cmd.extend(["--emit-json", "on"])
    if crop:
        cmd.extend(["--crop", crop])
    if force_tesseract_merge and "--force-tesseract-merge" not in (extra_args or []):
        extra_args = (extra_args or []) + ["--force-tesseract-merge"]
    if extra_args:
        cmd.extend(extra_args)
    print(f"[dispatcher] PDF を OCR ルートへ委譲: {' '.join(cmd)}")
    subprocess.run(cmd, check=True)


def _run_image(
    image_path: Path,
    *,
    mode: str,
    device: str,
    output_root: Path,
    svg_dpi: int,
    enable_figure: bool,
    ocr_profile: str,
    image_as_pdf: bool,
    image_dpi: int,
    extra_pdf_args: list[str] | None,
    fallback_tesseract: bool,
    force_tesseract_merge: bool,
    emit_csv: bool = False,
    emit_json: bool = False,
    crop: str | None = None,
) -> Path:
    # 画像処理に必要なモジュールはここで遅延インポートして、PDF 経路では Pillow 未インストールでも動くようにする
    from image_preprocessor import (
        PROFILE_REGISTRY,
        get_profile,
        preprocess_image_variants,
    )

    output_dir = _ensure_output_dir(image_path, output_root)
    convert_dir = output_dir / CONVERTED_DIR_NAME
    try:
        conversion = ensure_png_image(image_path, convert_dir=convert_dir, svg_dpi=svg_dpi)
    except ImageConversionError as exc:
        raise IngestError(str(exc)) from exc

    if crop:
        try:
            from PIL import Image, ImageOps
        except ImportError as exc:
            raise IngestError("Pillow がインストールされていません（トリミングに必要）") from exc
        from image_normalizer import ImageConversionResult

        def _parse_crop(value: str) -> tuple[float, float, float, float]:
            parts = [p.strip() for p in value.split(",")]
            if len(parts) != 4:
                raise ValueError("crop は left,top,width,height の4要素が必要です")
            left, top, width, height = (float(p) for p in parts)
            return left, top, width, height

        try:
            left, top, width, height = _parse_crop(crop)
        except ValueError as exc:
            raise IngestError(f"--crop の指定が不正です: {exc}") from exc
        left = max(0.0, min(1.0, left))
        top = max(0.0, min(1.0, top))
        width = max(0.0, min(1.0 - left, width))
        height = max(0.0, min(1.0 - top, height))
        if width > 0 and height > 0:
            cropped_path = convert_dir / f"{image_path.stem}_cropped.png"
            with Image.open(conversion.converted) as img:
                img = ImageOps.exif_transpose(img)
                w, h = img.size
                lpx = int(round(left * w))
                tpx = int(round(top * h))
                rpx = int(round((left + width) * w))
                bpx = int(round((top + height) * h))
                if rpx > lpx and bpx > tpx:
                    img.crop((lpx, tpx, rpx, bpx)).save(cropped_path, format="PNG", optimize=True)
                    conversion = ImageConversionResult(
                        source=conversion.source,
                        converted=cropped_path,
                        performed=True,
                    )

    if image_as_pdf:
        pdf_path = convert_dir / f"{image_path.stem}.pdf"
        _convert_image_to_pdf(conversion.converted, pdf_path, dpi=image_dpi)
        print(f"[dispatcher] 画像を PDF 化してから OCR: {pdf_path} (dpi={image_dpi})")
        print(f"[dispatcher] PDF exists: {pdf_path.exists()}")
        _run_pdf(
            pdf_path,
            mode=mode,
            device=device,
            use_math_refiner=False,
            output_root=output_root,
            extra_args=_append_force_flags(extra_pdf_args, fallback_tesseract, force_tesseract_merge),
            force_tesseract_merge=force_tesseract_merge,
            emit_json=emit_json,  # PDF経由もJSONを出す
            crop=crop,
        )
        return output_dir

    ocr_profile_obj = get_profile(ocr_profile)
    variants = preprocess_image_variants(
        conversion.converted,
        output_dir / PREPROCESSED_DIR_NAME,
        profiles=[ocr_profile_obj],
        page_number=1,
    )

    ocr_source = variants[ocr_profile_obj.key]
    options = OcrOptions(
        mode=mode,
        device=device,
        enable_figure=enable_figure,
        fallback_tesseract=fallback_tesseract,
        force_tesseract_merge=force_tesseract_merge,
    )
    print(f"[dispatcher] 画像を OCR ルートへ委譲: {ocr_source}")
    run_ocr(ocr_source, output_dir, page_number=1, options=options)
    if emit_csv:
        export_csv(ocr_source, output_dir, options)
    if emit_json:
        # ocr.py の export_json は便利関数として使える
        from ocr import export_json
        export_json(ocr_source, output_dir, options)
    return output_dir


def _convert_to_excel(
    output_dir: Path,
    output_root: Path,
    *,
    excel_mode: str,
    excel_meta_sheet: bool,
) -> None:
    """yomi_formats/json 内の JSON を集めて Excel に変換する。"""

    import re

    json_dir = output_dir / "yomi_formats" / "json"
    json_files: list[Path] = []
    if json_dir.exists():
        json_files = sorted(list(json_dir.glob("*.json")))
    else:
        print(f"[dispatcher] JSON dir not found: {json_dir}")

    from export_excel_poc import (
        add_meta_sheet,
        load_tables_from_json,
        split_text_to_paragraphs,
        write_tables_to_workbook,
        write_text_to_workbook,
    )
    from plain_text import to_plain_text

    all_tables = []

    # ページ順に読み込む
    for json_path in json_files:
        try:
            page_image_path = None
            match = re.search(r"page_(\d{3})", json_path.name)
            if match:
                page_no = int(match.group(1))
                candidate = output_dir / "page_images" / f"page_{page_no:03}.png"
                if candidate.exists():
                    page_image_path = candidate

            tables = load_tables_from_json(
                json_path,
                page_image_path=page_image_path,
                enable_symbol_fallback=True,
            )
            all_tables.extend(tables)
        except ValueError:
            # tables が無い（または空）ページは通常あり得るのでスキップ
            continue
        except Exception as exc:
            print(f"[dispatcher] Failed to parse JSON tables: {json_path.name}: {exc}")

    # 保存先
    xlsx_path = output_dir / f"{output_dir.name}.xlsx"

    if all_tables:
        wb = write_tables_to_workbook(
            all_tables,
            sheet_prefix="Page",
            review_columns=False,
            auto_format=True,
            excel_mode=excel_mode,
        )
    else:
        merged_md = output_dir / f"{output_dir.name}_merged.md"
        md_candidates = [merged_md] if merged_md.exists() else sorted(output_dir.glob("page_*.md"))
        if not md_candidates:
            print("[dispatcher] No tables extracted, and no markdown found to export as text")
            return

        text_parts: list[str] = []
        for md_path in md_candidates:
            try:
                text_parts.append(to_plain_text(md_path.read_text(encoding="utf-8")))
            except OSError as exc:
                print(f"[dispatcher] Failed to read markdown: {md_path.name}: {exc}")
        paragraphs = split_text_to_paragraphs("\n\n".join(text_parts))
        wb = write_text_to_workbook(paragraphs, sheet_name="本文")
        print("[dispatcher] No tables extracted from JSONs; exported markdown as text sheet")

    # meta シート追加
    sheet_names = wb.sheetnames.copy()
    # Mock args for add_meta_sheet
    class MockArgs:
        input = "merged_json_pipeline"
        format = "json"
        output = xlsx_path
    
    if excel_meta_sheet:
        add_meta_sheet(wb, args=MockArgs(), sheet_names=sheet_names)
    
    wb.save(xlsx_path)
    print(f"[dispatcher] Saved Excel: {xlsx_path}")


def _convert_to_csv(output_dir: Path, *, excel_mode: str) -> None:
    """yomi_formats/json 内の JSON を集めて CSV（結合解除＋分割）に変換する。"""

    import csv
    import re

    json_dir = output_dir / "yomi_formats" / "json"
    json_files: list[Path] = []
    if json_dir.exists():
        json_files = sorted(list(json_dir.glob("*.json")))
    else:
        print(f"[dispatcher] JSON dir not found: {json_dir}")

    from export_excel_poc import (
        load_tables_from_json,
        split_text_to_paragraphs,
        write_tables_to_csv_files,
    )
    from plain_text import to_plain_text

    all_tables = []

    # ページ順に読み込む
    for json_path in json_files:
        try:
            page_image_path = None
            match = re.search(r"page_(\d{3})", json_path.name)
            if match:
                page_no = int(match.group(1))
                candidate = output_dir / "page_images" / f"page_{page_no:03}.png"
                if candidate.exists():
                    page_image_path = candidate

            tables = load_tables_from_json(
                json_path,
                page_image_path=page_image_path,
                enable_symbol_fallback=True,
            )
            all_tables.extend(tables)
        except ValueError:
            continue
        except Exception as exc:
            print(f"[dispatcher] Failed to parse JSON tables: {json_path.name}: {exc}")

    if all_tables:
        outputs = write_tables_to_csv_files(
            all_tables,
            output_dir=output_dir,
            base_name=output_dir.name,
            excel_mode=excel_mode,
        )
        if outputs:
            print(f"[dispatcher] Saved CSVs: {len(outputs)}")
        else:
            print("[dispatcher] CSV export requested, but no table segments produced")
        return

    # tables が無い場合は Markdown を本文 CSV として出す
    merged_md = output_dir / f"{output_dir.name}_merged.md"
    md_candidates = [merged_md] if merged_md.exists() else sorted(output_dir.glob("page_*.md"))
    if not md_candidates:
        print("[dispatcher] No tables extracted, and no markdown found to export as CSV")
        return

    text_parts: list[str] = []
    for md_path in md_candidates:
        try:
            text_parts.append(to_plain_text(md_path.read_text(encoding="utf-8")))
        except OSError as exc:
            print(f"[dispatcher] Failed to read markdown: {md_path.name}: {exc}")
    paragraphs = split_text_to_paragraphs("\n\n".join(text_parts))
    csv_path = output_dir / f"{output_dir.name}.csv"
    with csv_path.open("w", encoding="utf-8", newline="") as fp:
        writer = csv.writer(fp)
        writer.writerow(["本文"])
        for para in paragraphs:
            writer.writerow([para])
    print(f"[dispatcher] Saved CSV: {csv_path}")


def _ensure_output_dir(source: Path, output_root: Path) -> Path:
    output_root = Path(output_root)
    output_root.mkdir(parents=True, exist_ok=True)
    target = output_root / source.stem
    target.mkdir(parents=True, exist_ok=True)
    return target


def _convert_image_to_pdf(image_path: Path, pdf_path: Path, *, dpi: int) -> None:
    try:
        from PIL import Image
    except ImportError as exc:
        raise IngestError("Pillow がインストールされていません（画像→PDF 変換に必要）") from exc

    pdf_path.parent.mkdir(parents=True, exist_ok=True)
    with Image.open(image_path) as img:
        rgb = img.convert("RGB")
        rgb.save(pdf_path, format="PDF", resolution=dpi)


def main() -> None:
    args = parse_args()
    print(
        "[dispatcher] parsed args:",
        {
            "input_path": args.input_path,
            "mode": args.mode,
            "image_as_pdf": args.image_as_pdf,
            "image_dpi": args.image_dpi,
            "ocr_profile": args.ocr_profile,
            "enable_figure": args.enable_figure,
            "math_refiner": args.math_refiner,
            "fallback_tesseract": args.fallback_tesseract,
            "force_tesseract_merge": args.force_tesseract_merge,
            "formats": args.formats,
            "excel_mode": args.excel_mode,
            "excel_meta_sheet": args.excel_meta_sheet,
            "docx_math": args.docx_math,
            "crop": args.crop,
            "extra": args.extra,
        },
    )
    try:
        run(
            Path(args.input_path),
            mode=args.mode,
            device=args.device,
            output_root=args.output_root,
            svg_dpi=args.svg_dpi,
            enable_figure=args.enable_figure,
            use_math_refiner=args.math_refiner,
            extra_pdf_args=args.extra or None,
            ocr_profile=args.ocr_profile,
            image_as_pdf=args.image_as_pdf,
            image_dpi=args.image_dpi,
            fallback_tesseract=args.fallback_tesseract,
            force_tesseract_merge=args.force_tesseract_merge,
            formats=args.formats,
            docx_math=args.docx_math,
            crop=args.crop,
            excel_mode=args.excel_mode,
            excel_meta_sheet=args.excel_meta_sheet,
        )
    except (IngestError, ImageConversionError, subprocess.CalledProcessError) as exc:
        print(f"[dispatcher] エラー: {exc}")
        sys.exit(1)


if __name__ == "__main__":
    main()
