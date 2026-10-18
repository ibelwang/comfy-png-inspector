"""Build the application icon (.ico + PNG) from dev/design/icon-1024.png.

Run: python dev/make_icon.py
"""

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "dev" / "design" / "icon-1024.png"
ASSETS = ROOT / "assets"

SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def main() -> None:
    image = Image.open(SOURCE).convert("RGBA")
    ASSETS.mkdir(exist_ok=True)

    image.resize((256, 256), Image.LANCZOS).save(ASSETS / "icon-256.png")
    image.resize((512, 512), Image.LANCZOS).save(ASSETS / "icon-512.png")
    image.save(ASSETS / "icon.ico", format="ICO", sizes=SIZES)
    print("wrote", ASSETS / "icon.ico", "and icon-256.png / icon-512.png")


if __name__ == "__main__":
    main()
