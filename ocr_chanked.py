import os
import re
import sys
import time
import subprocess
from pathlib import Path

from pdf2image import convert_from_path, pdfinfo_from_path

"""
使い方:
    poetry run python ocr_chunked.py input.pdf
"""

CHUNK_SIZE = 10          # 10ページ単位
REST_SECONDS = 10        # チャンクごとの休憩秒数

if len(sys.argv) < 2:
    print("使い方: poetry run python ocr_chunked.py <PDFファイル>")
    sys.exit(1)

PDF_PATH = Path(sys.argv[1])

if not PDF_PATH.exists():
    print(f"エラー: {PDF_PATH} が見つかりません: {PDF_PATH}")
    sys.exit(1)

# プロジェクト内 poppler
BASE_DIR = Path(__file__).resolve().parent
POPPLER_PATH = BASE_DIR / "poppler" / "Library" / "bin"

if not POPPLER_PATH.exists():
    print(f"エラー: poppler が見つかりません: {POPPLER_PATH}")
    sys.exit(1)

os.environ["PATH"] = str(POPPLER_PATH) + os.pathsep + os.environ.get("PATH", "")

OUT_DIR = Path("results_pages")
OUT_DIR.mkdir(exist_ok=True)

RAW_MD_PATTERN = re.compile(rf"{re.escape(OUT_DIR.name)}_page_(\d+)_p(\d+)\.md")
ALT_MD_PATTERN = re.compile(r"page_?(\d+)(?:_p(\d+))?\.md")
TARGET_MD_PATTERN = re.compile(r"page_(\d+)(?:_p(\d+))?\.md")


def normalize_md_files(target_page=None):
    for md_path in OUT_DIR.glob("*.md"):
        name = md_path.name
        if TARGET_MD_PATTERN.fullmatch(name):
            continue
        match = RAW_MD_PATTERN.fullmatch(name)
        if not match:
            match = ALT_MD_PATTERN.fullmatch(name)
        if not match:
            continue

        page_num = int(match.group(1))
        part = int(match.group(2) or "1")

        if target_page is not None and page_num != target_page:
            continue

        suffix = "" if part <= 1 else f"_p{part:02}"
        new_path = OUT_DIR / f"page_{page_num:03}{suffix}.md"
        if new_path.exists():
            new_path.unlink()
        md_path.rename(new_path)


normalize_md_files()


def run_merger(base_name: str):
    merger = BASE_DIR / "poppler" / "merged_md.py"
    if not merger.exists():
        print("merged_md.py が見つからないため、自動マージはスキップします。")
        return

    cmd = [sys.executable, str(merger), "--base-name", base_name]
    print("\n--- merged_md.py を実行 ---")
    subprocess.run(cmd, check=True)

DPI = 150

info = pdfinfo_from_path(str(PDF_PATH), poppler_path=str(POPPLER_PATH))
num_pages = int(info["Pages"])

print(f"PDF: {PDF_PATH}")
print(f"総ページ数: {num_pages}")
print(f"チャンクサイズ: {CHUNK_SIZE}")
print(f"チャンク休憩: {REST_SECONDS} 秒")
print(f"poppler path: {POPPLER_PATH}")

current = 1
chunk_index = 1

while current <= num_pages:
    start_page = current
    end_page = min(current + CHUNK_SIZE - 1, num_pages)

    print(f"\n=== Chunk {chunk_index}: {start_page}〜{end_page} ===")

    for page in range(start_page, end_page + 1):
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

        cmd = [
            "yomitoku",
            str(img_path),
            "-f",
            "md",
            "--lite",
            "-d",
            "cpu",
            "-o",
            str(OUT_DIR),
        ]
        print(" ".join(cmd))
        subprocess.run(cmd, check=True)

        normalize_md_files(target_page=page)

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
