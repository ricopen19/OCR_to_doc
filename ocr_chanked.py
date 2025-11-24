import argparse
import os
import sys
import time
import platform
import subprocess
from pathlib import Path

from pdf2image import convert_from_path, pdfinfo_from_path

from math_refiner import MathRefiner
from ocr import OcrOptions, run_ocr, build_command

"""PDF をチャンク処理しながら OCR するユーティリティ。

例:
    poetry run python ocr_chanked.py input.pdf --start 11 --end 20
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="PDF をチャンク処理で OCR")
    parser.add_argument("pdf_path", help="入力 PDF ファイル")
    parser.add_argument("--start", type=int, default=1, help="開始ページ (1 起点)")
    parser.add_argument("--end", type=int, default=None, help="終了ページ (指定なしは最終ページ)")
    parser.add_argument(
        "--chunk-size",
        type=int,
        default=10,
        help="チャンク単位のページ数 (既定: 10)",
    )
    parser.add_argument(
        "--rest-seconds",
        type=int,
        default=10,
        help="チャンク完了後の休憩秒数 (既定: 10)",
    )
    parser.add_argument(
        "--enable-rest",
        action="store_true",
        help="低スペック対策の休憩を有効化 (既定: 無効)",
    )
    parser.add_argument(
        "--mode",
        choices=["lite", "full"],
        default="lite",
        help="YomiToku のモード (lite or full)",
    )
    parser.add_argument(
        "--label",
        help="追加ラベル。出力ディレクトリ名 (result/<PDF名>_<label>) に付与されます",
    )
    parser.add_argument(
        "--math-refiner",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Pix2Text を用いた数式置換を有効/無効にします (default: 有効)",
    )
    parser.add_argument(
        "--math-score",
        type=float,
        default=0.7,
        help="MathRefiner が採用する最小信頼度 (default: 0.7)",
    )
    parser.add_argument(
        "--math-cache",
        type=Path,
        help="Pix2Text のキャッシュルート (default: ./\.pix2text_cache)",
    )
    parser.add_argument(
        "--math-resized-shape",
        type=int,
        default=960,
        help="Pix2Text 推論時の resized_shape (default: 960)",
    )
    parser.add_argument(
        "--keep-page-images",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="ページ画像 (page_images/*.png) を保存します。--no-keep-page-images で削除",
    )
    return parser.parse_args()


args = parse_args()

PDF_PATH = Path(args.pdf_path)

if not PDF_PATH.exists():
    print(f"エラー: {PDF_PATH} が見つかりません: {PDF_PATH}")
    sys.exit(1)

CHUNK_SIZE = max(1, args.chunk_size)
REST_SECONDS = max(0, args.rest_seconds) if args.enable_rest else 0

# プロジェクト内 poppler
BASE_DIR = Path(__file__).resolve().parent


def resolve_poppler_path(base_dir: Path) -> Path:
    system = sys.platform
    candidates: list[Path] = []

    if system.startswith("win"):
        candidates.append(base_dir / "poppler" / "win" / "bin")
        candidates.append(base_dir / "poppler" / "Library" / "bin")  # legacy 互換
    elif system == "darwin":
        candidates.append(base_dir / "poppler" / "macos" / "bin")
        candidates.append(Path("/opt/homebrew/opt/poppler/bin"))
        candidates.append(Path("/usr/local/opt/poppler/bin"))
    else:
        candidates.append(base_dir / "poppler" / system / "bin")

    for path in candidates:
        if path.exists():
            return path

    raise FileNotFoundError(
        "Poppler バイナリが見つかりません。OS ごとの bin ディレクトリを用意するか、"
        "Homebrew / Choco などでインストールして PATH を設定してください。"
    )


POPPLER_PATH = resolve_poppler_path(BASE_DIR)
os.environ["PATH"] = str(POPPLER_PATH) + os.pathsep + os.environ.get("PATH", "")

OPTIONS = OcrOptions(mode=args.mode, device="cpu", enable_figure=True)

MATH_REFINER: MathRefiner | None = None
if args.math_refiner:
    try:
        MATH_REFINER = MathRefiner(
            cache_root=args.math_cache,
            min_score=args.math_score,
            resized_shape=args.math_resized_shape,
        )
    except RuntimeError as exc:
        print(f"MathRefiner の初期化に失敗したため無効化します: {exc}")
        MATH_REFINER = None


def run_merger(base_name: str):
    merger = BASE_DIR / "poppler" / "merged_md.py"
    if not merger.exists():
        print("merged_md.py が見つからないため、自動マージはスキップします。")
        return

    cmd = [
        sys.executable,
        str(merger),
        "--input",
        str(OUT_DIR),
        "--base-name",
        base_name,
    ]
    print("\n--- merged_md.py を実行 ---")
    subprocess.run(cmd, check=True)

DPI = 150

info = pdfinfo_from_path(str(PDF_PATH), poppler_path=str(POPPLER_PATH))
num_pages = int(info["Pages"])

start_page_limit = max(1, args.start)
end_page_limit = args.end if args.end is not None else num_pages
end_page_limit = min(end_page_limit, num_pages)

if start_page_limit > end_page_limit:
    raise SystemExit(
        f"開始ページ ({start_page_limit}) が終了ページ ({end_page_limit}) より後です。"
    )

label_suffix = args.label
if not label_suffix and (start_page_limit != 1 or end_page_limit != num_pages):
    label_suffix = f"p{start_page_limit}-{end_page_limit}"

RESULT_ROOT = Path("result")
output_dir_name = PDF_PATH.stem if not label_suffix else f"{PDF_PATH.stem}_{label_suffix}"
OUT_DIR = RESULT_ROOT / output_dir_name
OUT_DIR.mkdir(parents=True, exist_ok=True)
(OUT_DIR / "figures").mkdir(exist_ok=True)
PAGE_IMAGE_DIR = OUT_DIR / "page_images"
PAGE_IMAGE_DIR.mkdir(exist_ok=True)

print(f"PDF: {PDF_PATH}")
print(f"出力ディレクトリ: {OUT_DIR}")
print(f"総ページ数: {num_pages}")
print(f"処理範囲: {start_page_limit}〜{end_page_limit}")
print(f"チャンクサイズ: {CHUNK_SIZE}")
if REST_SECONDS > 0:
    print(f"チャンク休憩: {REST_SECONDS} 秒 (有効)")
else:
    print("チャンク休憩: 無効 ( --enable-rest を指定で有効化 )")
print(f"poppler path: {POPPLER_PATH}")

current = start_page_limit
chunk_index = 1

while current <= end_page_limit:
    chunk_start = current
    chunk_end = min(current + CHUNK_SIZE - 1, end_page_limit)

    print(f"\n=== Chunk {chunk_index}: {chunk_start}〜{chunk_end} ===")

    for page in range(chunk_start, chunk_end + 1):
        print(f"\n--- Page {page}/{num_pages} ---")

        images = convert_from_path(
            str(PDF_PATH),
            dpi=DPI,
            first_page=page,
            last_page=page,
            fmt="png",
            poppler_path=str(POPPLER_PATH),
        )
        img = images[0]

        img_path = PAGE_IMAGE_DIR / f"page_{page:03}.png"
        img.save(img_path)
        del img

        preview_cmd = build_command(img_path, OUT_DIR, OPTIONS)
        print(" ".join(preview_cmd))
        run_ocr(img_path, OUT_DIR, page_number=page, options=OPTIONS)

        if MATH_REFINER:
            md_paths = sorted(OUT_DIR.glob(f"page_{page:03}*.md"))
            if md_paths:
                result = MATH_REFINER.refine_page(
                    page_md_paths=md_paths,
                    image_path=img_path,
                    page_number=page,
                )
                if result.replaced:
                    print(
                        f"MathRefiner: {result.replaced} 件の数式を置換 (未使用 {result.unused})"
                    )
                elif result.unused:
                    print(
                        f"MathRefiner: 数式を {result.unused} 件検出しましたが置換対象がありませんでした"
                    )

        if not args.keep_page_images:
            try:
                img_path.unlink()
            except FileNotFoundError:
                pass

        time.sleep(1.0)  # ページごとの軽い休憩

    if REST_SECONDS > 0:
        print(f"\n=== Chunk {chunk_index} 完了 → {REST_SECONDS} 秒休憩 ===")
        time.sleep(REST_SECONDS)
    else:
        print(f"\n=== Chunk {chunk_index} 完了 → 休憩なし ===")

    current += CHUNK_SIZE
    chunk_index += 1

run_merger(output_dir_name)

print("\nすべてのチャンク処理が完了しました。")
