"""YOLOv8x-DocLayNet による図表検出・切り出しスクリプト

Usage:
    python detect_figures.py <page_image> <output_dir> <page_number>
        [--conf 0.35] [--min-width 150] [--min-height 100]

output_dir/figures/ に fig_page{NNN}_{NN}.png を出力する。
"""
import argparse
import sys
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description="図表検出（YOLOv8x-DocLayNet）")
    parser.add_argument("image", help="ページ画像のパス")
    parser.add_argument("output_dir", help="出力ディレクトリ（figures/ サブディレクトリに保存）")
    parser.add_argument("page_number", type=int, help="ページ番号（1-indexed）")
    parser.add_argument("--conf", type=float, default=0.35, help="信頼度閾値 (default: 0.35)")
    parser.add_argument("--min-width", type=int, default=150, help="最小幅 (default: 150px)")
    parser.add_argument("--min-height", type=int, default=100, help="最小高さ (default: 100px)")
    args = parser.parse_args()

    image_path = Path(args.image)
    if not image_path.exists():
        print(f"[detect_figures] 画像が見つかりません: {image_path}", file=sys.stderr)
        sys.exit(1)

    figures_dir = Path(args.output_dir) / "figures"
    figures_dir.mkdir(parents=True, exist_ok=True)

    try:
        from huggingface_hub import hf_hub_download
        from ultralytics import YOLO
        from PIL import Image
    except ImportError as e:
        print(f"[detect_figures] 依存パッケージが不足: {e}", file=sys.stderr)
        print("[detect_figures] pip install ultralytics huggingface_hub Pillow", file=sys.stderr)
        sys.exit(1)

    # モデルのダウンロード（初回のみ）
    model_path = hf_hub_download(
        repo_id="DILHTWD/documentlayoutsegmentation_YOLOv8_ondoclaynet",
        filename="yolov8x-doclaynet-epoch64-imgsz640-initiallr1e-4-finallr1e-5.pt",
    )
    model = YOLO(model_path)

    # 推論
    results = model.predict(
        str(image_path),
        imgsz=1024,
        conf=args.conf,
        device="cpu",
        verbose=False,
    )

    img = Image.open(image_path)
    page_num = args.page_number
    count = 0

    for result in results:
        for box in result.boxes:
            cls_name = result.names[int(box.cls)]
            if cls_name != "Picture":
                continue

            x1, y1, x2, y2 = [int(v) for v in box.xyxy[0].tolist()]
            w, h = x2 - x1, y2 - y1

            if w < args.min_width or h < args.min_height:
                continue

            count += 1
            cropped = img.crop((x1, y1, x2, y2))
            fig_name = f"fig_page{page_num:03d}_{count:02d}.png"
            fig_path = figures_dir / fig_name
            cropped.save(fig_path)
            print(f"[detect_figures] {fig_name} ({w}x{h}, conf={float(box.conf):.2f})")

    print(f"[detect_figures] page {page_num}: {count} 件の図を検出")


if __name__ == "__main__":
    main()
