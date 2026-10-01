#!/usr/bin/env python3
"""Builds src/assets/fonts/nebula-symbols.woff2 from the Nerd Fonts "Symbols Only" font.

Only the glyphs Nebula prints (collected from crates/nebula-sh/src) are kept.

    pip install fonttools brotli
    python scripts/subset-icons.py path/to/SymbolsNerdFontMono-Regular.ttf

Download the source font from https://github.com/ryanoasis/nerd-fonts/releases
(NerdFontsSymbolsOnly.zip, MIT license).
"""
import pathlib
import re
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = sorted((ROOT / "crates" / "nebula-sh" / "src").rglob("*.rs"))
OUTPUT = ROOT / "src" / "assets" / "fonts" / "nebula-symbols.woff2"


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    codepoints = sorted(
        {
            int(match, 16)
            for source in SOURCES
            for match in re.findall(r"\\u\{([0-9a-fA-F]+)\}", source.read_text(encoding="utf-8"))
            if int(match, 16) >= 0xE000
        }
    )
    font = TTFont(sys.argv[1])
    available = font.getBestCmap()
    missing = [f"U+{cp:04X}" for cp in codepoints if cp not in available]
    if missing:
        sys.exit(f"missing glyphs: {', '.join(missing)}")

    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = []
    options.name_IDs = ["*"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=codepoints)
    subsetter.subset(font)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    font.flavor = "woff2"
    font.save(OUTPUT)
    print(f"{len(codepoints)} glyphs -> {OUTPUT.relative_to(ROOT)} ({OUTPUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
