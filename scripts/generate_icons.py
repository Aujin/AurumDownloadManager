"""Generate the committed desktop icons from a simple vector-like drawing.

Requires Pillow only when regenerating icons; normal builds use the generated files.
"""

from pathlib import Path
from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

size = 512
image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((18, 18, 494, 494), radius=122, fill="#152536")
draw.polygon([(256, 86), (391, 421), (321, 421), (294, 350), (217, 350), (190, 421), (121, 421)], fill="#D8A965")
draw.polygon([(255, 194), (233, 291), (278, 291)], fill="#152536")

image.save(OUT / "icon.png")
image.resize((32, 32), Image.Resampling.LANCZOS).save(OUT / "32x32.png")
image.resize((128, 128), Image.Resampling.LANCZOS).save(OUT / "128x128.png")
image.save(OUT / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
image.save(OUT / "icon.icns")
