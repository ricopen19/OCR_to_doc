from pathlib import Path
import argparse
import re

INPUT_DIR = Path("results_pages")
PATTERN = re.compile(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md")


def collect_md_files():
    files = []
    for f in INPUT_DIR.glob("*.md"):
        m = PATTERN.match(f.name)
        if not m:
            continue
        page = int(m.group(1))
        part = int(m.group(2)) if m.group(2) else 0
        files.append((page, part, f))
    return files


def write_merged_md(files, output_path: Path):
    files.sort(key=lambda x: (x[0], x[1]))

    with output_path.open("w", encoding="utf-8") as out:
        current_page = None
        first_section = True
        for page, _, fpath in files:
            if page != current_page:
                if not first_section:
                    out.write("\n")
                out.write(f"# Page {page}\n\n")
                current_page = page
                first_section = False

            text = fpath.read_text(encoding="utf-8").strip()
            if text:
                out.write(text + "\n\n")


def cleanup_pages(files):
    for _, _, fpath in files:
        try:
            fpath.unlink()
        except FileNotFoundError:
            pass


def main():
    parser = argparse.ArgumentParser(
        description="results_pages 以下の md を結合して 1 ファイルにまとめます。"
    )
    parser.add_argument(
        "--base-name",
        default="merged",
        help="出力ファイル名のベース（拡張子は自動で `_merged.md` が付与されます）",
    )
    parser.add_argument(
        "--keep-pages",
        action="store_true",
        help="ページ単位の md ファイルを削除せずに残します。",
    )
    args = parser.parse_args()

    output_file = Path(f"{args.base_name}_merged.md")

    files = collect_md_files()
    if not files:
        print(f"結合対象の md ファイルが見つかりません: {INPUT_DIR}")
        raise SystemExit(1)

    write_merged_md(files, output_file)

    if args.keep_pages:
        print("ページ単位の md ファイルは保持しました (--keep-pages)。")
    else:
        cleanup_pages(files)
        print("ページ単位の md ファイルを削除しました。")

    print(f"結合完了: {output_file}")


if __name__ == "__main__":
    main()
