"""YomiToku OCR ラッパー。

- full/lite の切り替え
- --figure オプションの有効/無効
- 出力 Markdown のファイル名正規化（`page_000.md` 形式）

既存の `ocr_chanked.py` などからインポートして使えるように、
最小限の関数とデータクラスを提供する。
"""

from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass, replace, fields
from pathlib import Path
from typing import Iterable, Sequence, Dict, Any

from PIL import Image, ImageStat

from markdown_cleanup import clean_file

RAW_MD_PATTERN = re.compile(r"page_(\d+)_p(\d+)\.md")
ALT_MD_PATTERN = re.compile(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md")
TARGET_MD_PATTERN = re.compile(r"page_(\d+)(?:_p(\d+))?\.md")
RAW_FIG_PATTERN = re.compile(r"(?:.*_)?page_(\d+)(?:_p(\d+))?_figure_(\d+)(\.[A-Za-z0-9]+)$")
MATH_PATTERN = re.compile(r"(\\\(|\\\[)(.*?)(\\\)|\\\])", re.DOTALL)
IMG_TAG_PATTERN = re.compile(
    r"<img[^>]*?src=\"(?P<src>[^\"]+)\"[^>]*?(?:alt=\"(?P<alt>[^\"]*)\")?[^>]*?>",
    re.IGNORECASE,
)


@dataclass
class IconFilterConfig:
    policy: str = "auto"  # auto / review / keep
    log_candidates: bool = True
    candidate_log_name: str = "icon_candidates.json"
    log_all_figures: bool = False
    all_stats_log_name: str = "all_fig_stats.json"
    max_width: int = 1000
    max_height: int = 1000
    max_area: int = 1_000_000
    max_width_ratio: float = 0.35
    max_height_ratio: float = 0.25
    max_area_ratio: float = 0.1
    auto_drop_area: int = 2_500
    auto_drop_area_ratio: float = 0.002
    auto_drop_unique_colors: int = 20
    auto_drop_avg_std: float = 10.0
    likely_area: int = 100_000
    likely_area_ratio: float = 0.1
    likely_unique_colors: int = 80
    likely_avg_std: float = 18.0
    likely_dominant_ratio: float = 0.7
    whitespace_mean_luma: float = 245.0
    whitespace_non_white_ratio: float = 0.05
    max_color_samples: int = 4_096


_ICON_FILTER_CONFIG = IconFilterConfig()


def update_icon_filter_config(**overrides: Any) -> IconFilterConfig:
    """Update global icon filter config with validated overrides."""

    global _ICON_FILTER_CONFIG
    if not overrides:
        return _ICON_FILTER_CONFIG

    valid_keys = {f.name for f in fields(IconFilterConfig)}
    filtered: dict[str, Any] = {}
    for key, value in overrides.items():
        if key not in valid_keys:
            raise ValueError(f"未知の icon filter 設定キーです: {key}")
        filtered[key] = value
    _ICON_FILTER_CONFIG = replace(_ICON_FILTER_CONFIG, **filtered)
    return _ICON_FILTER_CONFIG


def get_icon_filter_config() -> IconFilterConfig:
    return _ICON_FILTER_CONFIG


@dataclass
class OcrOptions:
    mode: str = "lite"  # "lite" or "full"
    device: str = "cpu"
    enable_figure: bool = True
    extra_args: Sequence[str] | None = None
    fallback_tesseract: bool = False
    force_tesseract_merge: bool = False

    def to_cli_args(self) -> list[str]:
        args: list[str] = []
        if self.mode == "lite":
            args.extend(["--lite", "-d", self.device])
        else:
            # full の場合でも device を明示しておく
            args.extend(["-d", self.device])
        if self.enable_figure:
            args.append("--figure")
        if self.extra_args:
            args.extend(self.extra_args)
        return args


def build_command(image_path: Path, output_dir: Path, options: OcrOptions) -> list[str]:
    cmd = [
        "yomitoku",
        str(image_path),
        "-f",
        "md",
        "-o",
        str(output_dir),
    ]
    cmd.extend(options.to_cli_args())
    return cmd


def build_json_command(image_path: Path, output_dir: Path, options: OcrOptions) -> list[str]:
    """YomiToku で JSON を出力するコマンドを構築する。

    - 出力先は output_dir / "yomi_formats" / "json"
    - 画像入力（page_images/*.png）に対して 1 ページずつ実行する想定。
    """

    json_dir = Path(output_dir) / "yomi_formats" / "json"
    json_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        "yomitoku",
        str(image_path),
        "-f",
        "json",
        "-o",
        str(json_dir),
        "-d",
        options.device,
    ]
    # json 出力では図版抽出は不要なので --figure は付けない
    if options.mode == "lite":
        cmd.insert(4, "--lite")
    if options.extra_args:
        cmd.extend(options.extra_args)
    return cmd


def _img_tag_to_markdown(text: str) -> str:
    """Convert HTML <img> tags to markdown image syntax ![alt](src)."""

    def repl(match: re.Match) -> str:
        src = match.group("src")
        alt = match.group("alt") or ""
        return f"![{alt}]({src})"

    return IMG_TAG_PATTERN.sub(repl, text)


def normalize_markdown_files(output_dir: Path, target_page: int | None = None) -> None:
    for md_path in output_dir.glob("*.md"):
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
        new_path = output_dir / f"page_{page_num:03}{suffix}.md"
        if new_path.exists():
            new_path.unlink()
        md_path.rename(new_path)


def rename_figure_assets(
    output_dir: Path,
    page_number: int,
    icon_config: IconFilterConfig | None = None,
    page_metrics: dict[str, Any] | None = None,
) -> None:
    figure_dir = output_dir / "figures"
    if not figure_dir.exists():
        return

    entries = []
    for fig_path in figure_dir.glob("*"):
        match = RAW_FIG_PATTERN.match(fig_path.name)
        if not match:
            continue
        page = int(match.group(1))
        if page != page_number:
            continue
        part = int(match.group(2) or 0)
        idx = int(match.group(3))
        entries.append((part, idx, fig_path))

    if not entries:
        return

    entries.sort()
    mapping: Dict[str, str] = {}
    for new_idx, (_, _, fig_path) in enumerate(entries, start=1):
        new_name = f"fig_page{page_number:03d}_{new_idx:02d}{fig_path.suffix.lower()}"
        new_path = figure_dir / new_name
        fig_path.rename(new_path)
        mapping[fig_path.name] = new_name

    _update_markdown_figure_links(output_dir, page_number, mapping)
    remove_icon_figures(output_dir, page_number, icon_config, page_metrics)


def _update_markdown_figure_links(output_dir: Path, page_number: int, mapping: Dict[str, str]) -> None:
    if not mapping:
        return

    candidates = list(output_dir.glob(f"page_{page_number:03}*.md"))
    for md_path in candidates:
        text = md_path.read_text(encoding="utf-8")
        replaced = False
        for old, new in mapping.items():
            new_path = f"./figures/{new}"
            replacements = [
                (f'src="figures/{old}"', f'src="{new_path}"'),
                (f'src="./figures/{old}"', f'src="{new_path}"'),
                (f'src="{old}"', f'src="{new_path}"'),
                (f'](figures/{old})', f']({new_path})'),
                (f'](./figures/{old})', f']({new_path})'),
                (f']({old})', f']({new_path})'),
            ]
            for old_token, new_token in replacements:
                if old_token in text:
                    text = text.replace(old_token, new_token)
                    replaced = True
            # フォールバック（素のパスだけが残っているケース）
            raw_variants = [f"figures/{old}", f"./figures/{old}", old]
            for variant in raw_variants:
                if variant in text:
                    text = text.replace(variant, new_path)
                    replaced = True
        text = _img_tag_to_markdown(text)
        sanitized = _sanitize_math(text)
        if sanitized != text:
            text = sanitized
            replaced = True
        if replaced:
            md_path.write_text(text, encoding="utf-8")

    cleanup_markdown_files(output_dir, page_number)


def cleanup_markdown_files(output_dir: Path, page_number: int) -> None:
    for md_path in output_dir.glob(f"page_{page_number:03}*.md"):
        clean_file(md_path, inplace=True)


def remove_icon_figures(
    output_dir: Path,
    page_number: int,
    config: IconFilterConfig | None = None,
    page_metrics: dict[str, Any] | None = None,
) -> None:
    figure_dir = output_dir / "figures"
    if not figure_dir.exists():
        return

    config = config or get_icon_filter_config()
    removable: list[tuple[Path, dict[str, Any], str]] = []
    log_records: list[dict[str, Any]] = []
    all_records: list[dict[str, Any]] = []
    for fig_path in figure_dir.glob(f"fig_page{page_number:03d}_*"):
        if fig_path.suffix.lower() not in {".png", ".jpg", ".jpeg"}:
            continue
        try:
            with Image.open(fig_path) as img:
                stats = collect_figure_stats(img, config, page_metrics)
        except Exception:
            continue
        decision = decide_icon_action(stats, config)
        record = {
            "page": page_number,
            "figure": fig_path.name,
            "decision": decision,
            "removed": should_remove_icon(decision, config),
            "metrics": stats,
        }
        all_records.append(record)
        if config.log_candidates and decision != "keep":
            log_records.append(record)
        if should_remove_icon(decision, config):
            removable.append((fig_path, stats, decision))

    if config.log_all_figures and all_records:
        _append_all_stats_log(figure_dir, all_records, config)
    if log_records:
        _append_icon_log(figure_dir, log_records, config)

    if not removable:
        return

    for icon, _, _ in removable:
        remove_figure_references(output_dir, page_number, icon.name)
        try:
            icon.unlink()
        except FileNotFoundError:
            pass


def collect_figure_stats(
    image: Image.Image,
    config: IconFilterConfig,
    page_metrics: dict[str, Any] | None = None,
) -> dict[str, Any]:
    width, height = image.size
    area = width * height
    aspect_ratio = (width / height) if height else 0
    sample = image.convert("RGB")
    colors = sample.getcolors(maxcolors=config.max_color_samples) or []
    unique = len(colors)

    stat_rgb = ImageStat.Stat(sample)
    avg_std = sum(stat_rgb.stddev) / max(1, len(stat_rgb.stddev))

    gray = image.convert("L")
    stat_gray = ImageStat.Stat(gray)
    mean_luma = stat_gray.mean[0]
    histogram = gray.histogram()
    white_pixels = sum(histogram[250:])
    non_white_ratio = 0.0
    if area:
        non_white_ratio = max(0.0, min(1.0, (area - white_pixels) / area))

    dominant_ratio = 0.0
    if colors and area:
        dominant = max(count for count, _ in colors)
        dominant_ratio = dominant / area

    page_width = page_metrics.get("width") if page_metrics else None
    page_height = page_metrics.get("height") if page_metrics else None
    page_area = page_metrics.get("area") if page_metrics else None

    width_ratio = (width / page_width) if page_width else 0.0
    height_ratio = (height / page_height) if page_height else 0.0
    area_ratio = (area / page_area) if page_area else 0.0

    return {
        "width": width,
        "height": height,
        "area": area,
        "aspect_ratio": aspect_ratio,
        "unique_colors": unique,
        "avg_std": round(avg_std, 4),
        "mean_luma": round(mean_luma, 4),
        "non_white_ratio": round(non_white_ratio, 4),
        "dominant_ratio": round(dominant_ratio, 4),
        "width_ratio": round(width_ratio, 6),
        "height_ratio": round(height_ratio, 6),
        "area_ratio": round(area_ratio, 6),
    }


def decide_icon_action(stats: dict[str, Any], config: IconFilterConfig) -> str:
    area = stats["area"]
    width = stats["width"]
    height = stats["height"]
    width_ratio = stats["width_ratio"]
    height_ratio = stats["height_ratio"]
    area_ratio = stats["area_ratio"]

    if area == 0:
        return "keep"
    if (
        width > config.max_width
        or height > config.max_height
        or area > config.max_area
        or width_ratio > config.max_width_ratio
        or height_ratio > config.max_height_ratio
        or area_ratio > config.max_area_ratio
    ):
        return "keep"

    mean_luma = stats["mean_luma"]
    non_white_ratio = stats["non_white_ratio"]
    if mean_luma >= config.whitespace_mean_luma and non_white_ratio <= config.whitespace_non_white_ratio:
        return "too_whitespace"

    unique = stats["unique_colors"]
    avg_std = stats["avg_std"]
    dominant_ratio = stats["dominant_ratio"]

    if (
        area <= config.auto_drop_area
        and area_ratio <= config.auto_drop_area_ratio
        and unique <= config.auto_drop_unique_colors
        and avg_std <= config.auto_drop_avg_std
    ):
        return "auto_drop"

    if (
        area <= config.likely_area
        and area_ratio <= config.likely_area_ratio
        and (unique <= config.likely_unique_colors or dominant_ratio >= config.likely_dominant_ratio)
        and avg_std <= config.likely_avg_std
    ):
        return "likely_icon"

    return "keep"


def should_remove_icon(decision: str, config: IconFilterConfig) -> bool:
    if config.policy == "keep":
        return False
    if decision in {"auto_drop", "too_whitespace"}:
        return True
    if decision == "likely_icon":
        return config.policy == "auto"
    return False


def _append_icon_log(figure_dir: Path, records: list[dict[str, Any]], config: IconFilterConfig) -> None:
    log_path = figure_dir / config.candidate_log_name
    existing: list[dict[str, Any]] = []
    if log_path.exists():
        try:
            existing = json.loads(log_path.read_text(encoding="utf-8"))
            if not isinstance(existing, list):
                existing = []
        except json.JSONDecodeError:
            existing = []
    existing.extend(records)
    log_path.write_text(json.dumps(existing, ensure_ascii=False, indent=2), encoding="utf-8")


def _append_all_stats_log(figure_dir: Path, records: list[dict[str, Any]], config: IconFilterConfig) -> None:
    log_path = figure_dir / config.all_stats_log_name
    existing: list[dict[str, Any]] = []
    if log_path.exists():
        try:
            existing = json.loads(log_path.read_text(encoding="utf-8"))
            if not isinstance(existing, list):
                existing = []
        except json.JSONDecodeError:
            existing = []
    existing.extend(records)
    log_path.write_text(json.dumps(existing, ensure_ascii=False, indent=2), encoding="utf-8")


def _load_page_metrics(image_path: Path) -> dict[str, Any] | None:
    try:
        with Image.open(image_path) as img:
            width, height = img.size
    except Exception:
        return None
    return {"width": width, "height": height, "area": width * height}


def _maybe_fallback_tesseract(image_path: Path, output_dir: Path, page_number: int) -> None:
    """If OCR output is too sparse, try pytesseract as a fallback."""

    md_path = output_dir / f"page_{page_number:03d}.md"
    if not md_path.exists():
        return

    text = md_path.read_text(encoding="utf-8")
    stripped = re.sub(r"<img[^>]+>", "", text)
    stripped = stripped.replace("\\g<1>", "").strip()
    meaningful = re.findall(r"[A-Za-z0-9\u3040-\u30ff\u4e00-\u9fff]", stripped)
    if len(meaningful) >= 30:
        return  # それなりに文字があると判断

    log_lines: list[str] = []
    try:
        import pytesseract  # type: ignore
        from PIL import Image, ImageEnhance
    except Exception as exc:  # pragma: no cover
        (output_dir / "fallback.log").write_text(
            f"pytesseract unavailable: {exc}\n", encoding="utf-8"
        )
        return

    try:
        with Image.open(image_path) as im:
            gray = im.convert("L")
            # コントラストを少し上げてから認識
            gray = ImageEnhance.Contrast(gray).enhance(1.6)
            text = pytesseract.image_to_string(
                gray,
                lang="jpn+eng",
                config="--psm 6",
            )
            log_lines.append("fallback=tesseract applied (contrast x1.6)")
    except Exception as exc:  # pragma: no cover
        (output_dir / "fallback.log").write_text(
            f"pytesseract failed: {exc}\n", encoding="utf-8"
        )
        return

    if text.strip():
        md_path.write_text(text.strip(), encoding="utf-8")
        log_lines.append(f"chars: {len(text.strip())}")
        (output_dir / "fallback.log").write_text("\n".join(log_lines), encoding="utf-8")


def _force_tesseract_merge(image_path: Path, output_dir: Path, page_number: int) -> None:
    """Always run pytesseract and append its text to the page markdown."""

    md_path = output_dir / f"page_{page_number:03d}.md"
    if not md_path.exists():
        return

    try:
        import pytesseract  # type: ignore
        from PIL import Image, ImageEnhance
    except Exception as exc:  # pragma: no cover
        (output_dir / "fallback.log").write_text(
            f"force_tesseract_merge unavailable: {exc}\n", encoding="utf-8"
        )
        return

    try:
        with Image.open(image_path) as im:
            gray = im.convert("L")
            gray = ImageEnhance.Contrast(gray).enhance(1.6)
            text = pytesseract.image_to_string(
                gray,
                lang="jpn+eng",
                config="--psm 6",
            )
    except Exception as exc:  # pragma: no cover
        (output_dir / "fallback.log").write_text(
            f"force_tesseract_merge failed: {exc}\n", encoding="utf-8"
        )
        return

    if not text.strip():
        return

    existing = md_path.read_text(encoding="utf-8")
    merged = existing.rstrip() + "\n\n<!-- tesseract -->\n" + text.strip()
    md_path.write_text(merged, encoding="utf-8")
    (output_dir / "fallback.log").write_text(
        "force_tesseract_merge appended\n", encoding="utf-8"
    )


def remove_figure_references(output_dir: Path, page_number: int, figure_name: str) -> None:
    candidates = list(output_dir.glob(f"page_{page_number:03}*.md"))
    patterns = [
        rf"!\[[^\]]*\]\((?:\./)?figures/{re.escape(figure_name)}\)",
        rf"<img[^>]+src=\"(?:\./)?figures/{re.escape(figure_name)}\"[^>]*>"
    ]
    combined = re.compile("|".join(patterns))
    for md_path in candidates:
        text = md_path.read_text(encoding="utf-8")
        new_text = combined.sub("", text)
        if new_text != text:
            md_path.write_text(new_text, encoding="utf-8")


def _sanitize_math(text: str) -> str:
    def repl(match: re.Match[str]) -> str:
        opener, body, closer = match.groups()
        body = body.replace(r"\-", "-")
        body = body.replace(r"\+", "+")
        body = body.replace(r"\×", r"\\times ")
        body = body.replace(r"\÷", r"\\div ")
        body = body.replace(r"\=", "=")
        new_opener = "$$" if opener == r"\[" else "$"
        new_closer = new_opener
        return f"{new_opener}{body}{new_closer}"

    return MATH_PATTERN.sub(repl, text)


def run_ocr(
    image_path: Path,
    output_dir: Path,
    page_number: int,
    options: OcrOptions,
    icon_config: IconFilterConfig | None = None,
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    cmd = build_command(image_path, output_dir, options)
    log_path = output_dir / "ocr.log"
    result = subprocess.run(cmd, capture_output=True, text=True)
    log_path.write_text(
        f"cmd: {' '.join(cmd)}\n\nstdout:\n{result.stdout}\n\nstderr:\n{result.stderr}",
        encoding="utf-8",
    )
    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode, cmd, output=result.stdout, stderr=result.stderr
        )
    normalize_markdown_files(output_dir, target_page=page_number)
    page_metrics = _load_page_metrics(image_path)
    rename_figure_assets(output_dir, page_number, icon_config, page_metrics)
    if options.fallback_tesseract:
        _maybe_fallback_tesseract(image_path, output_dir, page_number)
    if options.force_tesseract_merge:
        _force_tesseract_merge(image_path, output_dir, page_number)


def export_json(
    image_path: Path,
    output_dir: Path,
    options: OcrOptions,
) -> None:
    """ページ画像から JSON を追加出力する補助関数。"""

    cmd = build_json_command(image_path, output_dir, options)
    log_dir = Path(output_dir) / "yomi_formats"
    log_dir.mkdir(parents=True, exist_ok=True)
    log_path = log_dir / "json_export.log"
    result = subprocess.run(cmd, capture_output=True, text=True)
    with log_path.open("a", encoding="utf-8") as fp:
        fp.write(f"cmd: {' '.join(cmd)}\n\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}\n\n")
    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode, cmd, output=result.stdout, stderr=result.stderr
        )


def run_batch(
    image_paths: Iterable[Path],
    output_dir: Path,
    start_page: int = 1,
    options: OcrOptions | None = None,
    icon_config: IconFilterConfig | None = None,
) -> None:
    options = options or OcrOptions()
    for idx, img in enumerate(image_paths, start=start_page):
        run_ocr(img, output_dir, idx, options, icon_config)



def build_csv_command(image_path: Path, output_dir: Path, options: OcrOptions) -> list[str]:
    """YomiToku で CSV を出力するコマンドを構築する。

    - 出力先は output_dir / "yomi_formats" / "csv"
    """

    csv_dir = Path(output_dir) / "yomi_formats" / "csv"
    csv_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        "yomitoku",
        str(image_path),
        "-f",
        "csv",
        "-o",
        str(csv_dir),
        "-d",
        options.device,
    ]
    # figure は csv に関係ないが、lite モード指定は一応引き継ぐ
    if options.mode == "lite":
        cmd.insert(4, "--lite")
    if options.extra_args:
        cmd.extend(options.extra_args)
    return cmd


def export_csv(
    image_path: Path,
    output_dir: Path,
    options: OcrOptions,
) -> None:
    """ページ画像から CSV を追加出力する補助関数。"""

    cmd = build_csv_command(image_path, output_dir, options)
    log_dir = Path(output_dir) / "yomi_formats"
    log_dir.mkdir(parents=True, exist_ok=True)
    log_path = log_dir / "csv_export.log"
    result = subprocess.run(cmd, capture_output=True, text=True)
    with log_path.open("a", encoding="utf-8") as fp:
        fp.write(f"cmd: {' '.join(cmd)}\n\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}\n\n")
    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode, cmd, output=result.stdout, stderr=result.stderr
        )


__all__ = [
    "OcrOptions",
    "IconFilterConfig",
    "build_command",
    "normalize_markdown_files",
    "rename_figure_assets",
    "run_ocr",
    "run_batch",
    "update_icon_filter_config",
    "get_icon_filter_config",
    "export_json",
    "export_csv",
]
