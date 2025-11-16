import os
import re
import sys
import time
import subprocess
from pathlib import Path

from pdf2image import convert_from_path, pdfinfo_from_path


"""
使い方:
    poetry run python ocr_per_page.py input.pdf          # 全ページ
    poetry run python ocr_per_page.py input.pdf 1 10    # 1〜10ページだけ
"""

if len(sys.argv) < 2:
    print("使い方: poetry run python ocr_per_page.py <PDFファイル> [start_page] [end_page]")
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

# このプロセス内だけ PATH を拡張
os.environ["PATH"] = str(POPPLER_PATH) + os.pathsep + os.environ.get("PATH", "")

# 出力ディレクトリ
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


# 既存の出力があれば先にリネームしておく
normalize_md_files()


def run_merger(base_name: str):
    merger = BASE_DIR / "poppler" / "merged_md.py"
    if not merger.exists():
        print("merged_md.py が見つからないため、自動マージはスキップします。")
        return

    cmd = [sys.executable, str(merger), "--base-name", base_name]
    print("\n--- merged_md.py を実行 ---")
    subprocess.run(cmd, check=True)

DPI = 150  # 安全寄り。必要なら 200 などに調整

# ページ数取得
info = pdfinfo_from_path(str(PDF_PATH), poppler_path=str(POPPLER_PATH))
num_pages = int(info["Pages"])

# ページ範囲
if len(sys.argv) >= 3:
    start_page = max(1, int(sys.argv[2]))
else:
    start_page = 1

if len(sys.argv) >= 4:
    end_page = min(num_pages, int(sys.argv[3]))
else:
    end_page = num_pages

print(f"PDF: {PDF_PATH}")
print(f"総ページ数: {num_pages}")
print(f"処理範囲: {start_page}〜{end_page}")
print(f"poppler path: {POPPLER_PATH}")

for page in range(start_page, end_page + 1):
    print(f"\n=== Page {page}/{num_pages} ===")

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
    del img  # メモリ解放

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

    # 画像は使い終わったら削除
    try:
        img_path.unlink()
    except FileNotFoundError:
        pass

    # OSに少し休ませる
    time.sleep(1.0)

run_merger(PDF_PATH.stem)

print("\n??????????OCR????????")
