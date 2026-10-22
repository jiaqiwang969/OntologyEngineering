"""Generate tests/fixtures/pdfmedia/oracle.json from the original (images and media).

The fixtures come from generate_fixtures.py. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python \
        <crate>/tests/fixtures/pdfmedia/generate_oracle.py <crate>/tests/fixtures/pdfmedia

(PATH without ffprobe/tesseract, as in the reference environment.) Absolute fixture paths
are written as "$FILES". Bytes values are written as Python's str(bytes), tuples as lists
(what json.dumps(..., default=str) gives). No network.
"""

import email.message  # noqa: F401
import json
import os
import shutil
import sys
import tempfile

sys.path.insert(0, os.getcwd())

OUT = os.path.abspath(sys.argv[1])

from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

get_progress_tracker().displays = []

from PIL import Image  # noqa: E402
from semantica.parse import ImageParser, MediaParser  # noqa: E402
from semantica.parse import image_parser as IP  # noqa: E402


def conv(o):
    if isinstance(o, bytes):
        return str(o)
    if isinstance(o, tuple):
        return [conv(x) for x in o]
    if isinstance(o, list):
        return [conv(x) for x in o]
    if isinstance(o, dict):
        return {str(k): conv(v) for k, v in o.items()}
    if isinstance(o, float) or isinstance(o, int) or isinstance(o, str) or o is None or isinstance(o, bool):
        return o
    if hasattr(o, "__dict__") and not callable(o):
        return conv(o.__dict__)
    try:
        return float(o)
    except Exception:  # noqa: BLE001
        return str(o)


def norm(v):
    return json.loads(json.dumps(conv(v)).replace(OUT, "$FILES"))


def run(fn, *a, **k):
    try:
        return {"ok": norm(fn(*a, **k))}
    except Exception as e:  # noqa: BLE001
        return {"error": type(e).__name__, "message": str(e).replace(OUT, "$FILES")}


IMAGES = ["rgb.png", "dpi.png", "text.png", "photo.jpg", "progressive.jpg", "exif.jpg", "exif.png",
          "rgba.png", "gray.png", "palette.png", "bilevel.png", "la.png", "i16.png", "image.gif",
          "image.bmp", "image.webp", "image.tiff", "cmyk.jpg", "gray.jpg", "gradient.png",
          "broken.png", "truncated.png", "notes.txt", "missing.png"]
ip = ImageParser()
out = {"tesseract_available": IP.TESSERACT_AVAILABLE, "images": {}, "media": {}, "preprocess": {}}


def verify(path):
    with Image.open(path) as img:
        img.verify()
    return True


for name in IMAGES:
    p = os.path.join(OUT, name)
    out["images"][name] = {
        "extract_metadata": run(ip.extract_metadata, p),
        "parse": run(ip.parse, p),
        "parse_no_metadata": run(ip.parse, p, extract_metadata=False),
        "parse_text_no_metadata": run(ip.parse, p, extract_metadata=False, extract_text=True),
        "extract_text": run(ip.extract_text, p),
        "verify": run(verify, p),
    }

out["constructor_tesseract_path"] = run(ImageParser, tesseract_path="/usr/bin/tesseract")

# preprocess_image: work on copies in a scratch directory under OUT (removed afterwards); the
# results are compared pixel by pixel.
scratch = tempfile.mkdtemp(dir=OUT)
try:
    for name, kwargs in [("gradient.png", {}), ("gradient.png", {"grayscale": False}),
                         ("gradient.png", {"enhance_contrast": False}), ("gradient.png", {"resize": 0.5}),
                         ("gradient.png", {"resize": 2, "grayscale": False, "enhance_contrast": False}),
                         ("rgba.png", {}), ("palette.png", {"grayscale": False}), ("gray.png", {}),
                         ("photo.jpg", {"enhance_contrast": False})]:
        src = os.path.join(scratch, name)
        shutil.copyfile(os.path.join(OUT, name), src)
        key = name + ":" + json.dumps(kwargs, sort_keys=True)
        try:
            res = ip.preprocess_image(src, **kwargs)
            with Image.open(res) as im:
                out["preprocess"][key] = {"name": os.path.basename(str(res)), "mode": im.mode, "size": list(im.size),
                                          "pixels": [list(px) if isinstance(px, tuple) else px for px in im.getdata()]
                                          if not name.endswith(".jpg") else None}
        except Exception as e:  # noqa: BLE001
            out["preprocess"][key] = {"error": type(e).__name__, "message": str(e).replace(OUT, "$FILES")}
        os.remove(src)
        for f in os.listdir(scratch):
            os.remove(os.path.join(scratch, f))
finally:
    shutil.rmtree(scratch)

mp = MediaParser()
for name, media_type in [("rgb.png", None), ("song.mp3", None), ("clip.mp4", None), ("clip.mkv", None),
                         ("data.xyz", None), ("rgb.png", "audio"), ("song.mp3", "video"), ("missing.mp3", None),
                         ("photo.jpg", "image"), ("broken.png", None)]:
    p = os.path.join(OUT, name)
    key = name + ":" + str(media_type)
    out["media"][key] = {
        "parse_media": run(mp.parse_media, p, media_type=media_type),
        "extract_metadata": run(mp.extract_metadata, p, media_type=media_type),
    }
out["media_supported_formats"] = mp.supported_formats
out["safe_parse_fps"] = {s: run(__import__("semantica.parse.media_parser", fromlist=["x"])._safe_parse_fps, s)
                         for s in ["30000/1001", "25/1", "24", "0/0", "x", "1/0", "2.5", " 30/1 "]}

with open(os.path.join(OUT, "oracle.json"), "w", encoding="utf-8") as fh:
    json.dump(out, fh, ensure_ascii=False, indent=0)
print("wrote oracle")
