import argparse
import os
import sys
import time
import platform
import subprocess
from pathlib import Path

from pdf2image import convert_from_path, pdfinfo_from_path

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
        "--mode",
        choices=["lite", "full"],
        default="lite",
        help="YomiToku のモード (lite or full)",
    )
    return parser.parse_args()


args = parse_args()

PDF_PATH = Path(args.pdf_path)

if not PDF_PATH.exists():
    print(f"エラー: {PDF_PATH} が見つかりません: {PDF_PATH}")
    sys.exit(1)

CHUNK_SIZE = max(1, args.chunk_size)
REST_SECONDS = max(0, args.rest_seconds)

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

RESULT_ROOT = Path("result")
OUT_DIR = RESULT_ROOT / PDF_PATH.stem
OUT_DIR.mkdir(parents=True, exist_ok=True)
(OUT_DIR / "figures").mkdir(exist_ok=True)

OPTIONS = OcrOptions(mode=args.mode, device="cpu", enable_figure=True)


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

print(f"PDF: {PDF_PATH}")
print(f"総ページ数: {num_pages}")
print(f"処理範囲: {start_page_limit}〜{end_page_limit}")
print(f"チャンクサイズ: {CHUNK_SIZE}")
print(f"チャンク休憩: {REST_SECONDS} 秒")
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

        img_path = OUT_DIR / f"page_{page:03}.png"
        img.save(img_path)
        del img

        preview_cmd = build_command(img_path, OUT_DIR, OPTIONS)
        print(" ".join(preview_cmd))
        run_ocr(img_path, OUT_DIR, page_number=page, options=OPTIONS)

        try:
            img_path.unlink()
        except FileNotFoundError:
            pass

        time.sleep(1.0)  # ページごとの軽い休憩

    print(f"\n=== Chunk {chunk_index} 完了 → {REST_SECONDS} 秒休憩 ===")
    time.sleep(REST_SECONDS)

    current += CHUNK_SIZE
    chunk_index += 1

run_merger(PDF_PATH.stem)

print("\nすべてのチャンク処理が完了しました。")
