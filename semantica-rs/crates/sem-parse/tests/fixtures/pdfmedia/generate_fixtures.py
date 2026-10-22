"""Write the binary fixtures of the pdf/image/media tests into this directory.

PDFs are written by hand (raw PDF syntax, so their content is known by construction);
images are written with PIL in the oracle environment. Run from
var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python -I \
        <crate>/tests/fixtures/pdfmedia/generate_fixtures.py <crate>/tests/fixtures/pdfmedia
"""

import os
import sys
import zlib

OUT = sys.argv[1]


def write(name, data):
    with open(os.path.join(OUT, name), "wb") as f:
        f.write(data)


def stream(extra, data):
    return b"<< /Length %d%s >>\nstream\n" % (len(data), extra) + data + b"\nendstream"


def make_pdf(objects, info=None):
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    trailer = b"<< /Size %d /Root 1 0 R" % (len(objects) + 1)
    if info:
        trailer += b" /Info %d 0 R" % info
    out += b"trailer\n" + trailer + b" >>\nstartxref\n%d\n%%%%EOF\n" % xref
    return bytes(out)


def utf16(s):
    return b"<FEFF" + s.encode("utf-16-be").hex().upper().encode() + b">"


HELV = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"

# 1. simple.pdf: two lines of Helvetica text, a word gap made by TJ kerning, Info metadata.
content = (
    b"BT /F1 12 Tf 72 720 Td (Hello World) Tj 0 -20 Td (Second line here.) Tj ET\n"
    b"BT /F1 12 Tf 72 680 Td [(Kerned) -500 (words)] TJ ET"
)
write("simple.pdf", make_pdf([
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    stream(b"", content),
    HELV,
    b"<< /Title (Test PDF) /Author " + utf16("Ünïcode Author") + b" /Subject (Caf\\351 notes) "
    b"/Creator (fixture writer) /Producer (hand) /CreationDate (D:20240102030405Z) /ModDate (D:20240203040506+01'00') /Pages 7 >>",
], info=6))

# 2. multipage.pdf: three pages, the second with a fractional A4 MediaBox, the third rotated.
pages = []
objs = [b"<< /Type /Catalog /Pages 2 0 R >>", None]
kids = []
texts = [(b"Page one", b"[0 0 612 792]", b""), (b"Page two", b"[0 0 595.276 841.89]", b""), (b"Page three", b"[0 0 612 792]", b" /Rotate 90")]
next_id = 3
font_id = 3 + 2 * len(texts)
for text, box, extra in texts:
    page_id, content_id = next_id, next_id + 1
    kids.append(b"%d 0 R" % page_id)
    objs.append(b"<< /Type /Page /Parent 2 0 R /MediaBox " + box + extra + b" /Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>" % (font_id, content_id))
    objs.append(stream(b"", b"BT /F1 14 Tf 72 700 Td (" + text + b") Tj ET"))
    next_id += 2
objs[1] = b"<< /Type /Pages /Kids [" + b" ".join(kids) + b"] /Count %d >>" % len(texts)
objs.append(HELV)
write("multipage.pdf", make_pdf(objs))

# 3. table.pdf: a 3x2 ruled table (rectangles) with cell texts, plus a caption line.
cells = [(b"Name", b"Age"), (b"Alice", b"30"), (b"Bob", b"25")]
ops = [b"0.5 w"]
x0, x1, x2 = 100, 200, 300
top = 700
for r in range(3):
    y = top - 20 * (r + 1)
    ops.append(b"%d %d 100 20 re S" % (x0, y))
    ops.append(b"%d %d 100 20 re S" % (x1, y))
for r, (a, b) in enumerate(cells):
    y = top - 20 * (r + 1) + 6
    ops.append(b"BT /F1 10 Tf %d %d Td (%s) Tj ET" % (x0 + 5, y, a))
    ops.append(b"BT /F1 10 Tf %d %d Td (%s) Tj ET" % (x1 + 5, y, b))
ops.append(b"BT /F1 10 Tf 100 720 Td (People) Tj ET")
ops.append(b"100 600 m 300 600 l S")
write("table.pdf", make_pdf([
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    stream(b"", b"\n".join(ops)),
    HELV,
]))

# 4. image.pdf: an image XObject placed with cm, and an inline image.
pixels = bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
img = stream(b" /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", pixels)
content = (
    b"q 100 0 0 50 72 600 cm /Im1 Do Q\n"
    b"q 20 0 0 10 300 400 cm BI /W 2 /H 1 /BPC 8 /CS /G ID \x10\x20 EI Q\n"
    b"BT /F1 12 Tf 72 560 Td (Picture above) Tj ET"
)
write("image.pdf", make_pdf([
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 4 0 R >>",
    stream(b"", content),
    HELV,
    img,
]))

# 5. fonts.pdf: a compressed content stream; a WinAnsi font with Differences and explicit
#    Widths; a Type0 Identity-H font with a ToUnicode CMap; a form XObject with text.
tounicode = (
    b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
    b"1 begincodespacerange <0000> <FFFF> endcodespacerange\n"
    b"3 beginbfchar <0001> <0043> <0002> <0049> <0003> <0044> endbfchar\n"
    b"endcmap CMapName currentdict /CMap defineresource pop end end"
)
content = zlib.compress(
    b"BT /F2 12 Tf 72 720 Td (caf\xe9 \x80) Tj ET\n"
    b"BT /F2 12 Tf 72 700 Td (\x01\x02) Tj ET\n"
    b"BT /F3 12 Tf 72 680 Td <000100020003> Tj ET\n"
    b"q 1 0 0 1 72 640 cm /Fm1 Do Q"
)
widths = b"[" + b" ".join(b"500" for _ in range(256)) + b"]"
form = stream(b" /Type /XObject /Subtype /Form /BBox [0 0 200 50] /Resources << /Font << /F1 9 0 R >> >>",
              b"BT /F1 12 Tf 0 10 Td (In a form) Tj ET")
write("fonts.pdf", make_pdf([
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F2 5 0 R /F3 6 0 R >> /XObject << /Fm1 10 0 R >> >> /Contents 4 0 R >>",
    b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(content) + content + b"\nendstream",
    b"<< /Type /Font /Subtype /TrueType /BaseFont /CustomSans /FirstChar 0 /LastChar 255 /Widths " + widths +
    b" /Encoding << /BaseEncoding /WinAnsiEncoding /Differences [1 /f_i /Euro] >> /FontDescriptor 11 0 R >>",
    b"<< /Type /Font /Subtype /Type0 /BaseFont /CIDSans /Encoding /Identity-H /DescendantFonts [7 0 R] /ToUnicode 8 0 R >>",
    b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDSans /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /DW 600 /W [1 [700 800] 3 3 650] >>",
    stream(b"", tounicode),
    HELV,
    form,
    b"<< /Type /FontDescriptor /FontName /CustomSans /Descent -200 /Ascent 800 /Flags 32 /FontBBox [0 -200 1000 800] >>",
]))

write("notapdf.pdf", b"dummy pdf content")
write("notes.txt", b"plain text, not a pdf")

# ---------------------------------------------------------------------------------- images
from PIL import Image, PngImagePlugin  # noqa: E402

im = Image.new("RGB", (4, 3))
im.putdata([(10 * i, 255 - 10 * i, (37 * i) % 256) for i in range(12)])
im.save(os.path.join(OUT, "rgb.png"))
im.save(os.path.join(OUT, "dpi.png"), dpi=(150, 150))
info = PngImagePlugin.PngInfo()
info.add_text("Software", "fixture writer")
info.add_text("Comment", "hello")
im.save(os.path.join(OUT, "text.png"), pnginfo=info)
im.save(os.path.join(OUT, "photo.jpg"), quality=90)
im.save(os.path.join(OUT, "progressive.jpg"), dpi=(300, 300), progressive=True)
exif = Image.Exif()
exif[271] = "Canon"
exif[272] = "EOS"
exif[282] = 72.0
exif[283] = 72.0
exif[296] = 2
exif[306] = "2024:01:02 03:04:05"
sub = exif.get_ifd(0x8769)
sub[33434] = 0.5
sub[37386] = 50.0
sub[37510] = b"ASCII\0\0\0hi"
im.save(os.path.join(OUT, "exif.jpg"), exif=exif)
im.save(os.path.join(OUT, "exif.png"), exif=exif)
Image.new("RGBA", (3, 2), (1, 2, 3, 128)).save(os.path.join(OUT, "rgba.png"))
Image.new("L", (3, 2), 77).save(os.path.join(OUT, "gray.png"))
Image.new("P", (3, 2), 1).save(os.path.join(OUT, "palette.png"))
Image.new("1", (9, 2), 1).save(os.path.join(OUT, "bilevel.png"))
Image.new("LA", (2, 2), (5, 6)).save(os.path.join(OUT, "la.png"))
Image.new("I;16", (2, 2), 300).save(os.path.join(OUT, "i16.png"))
Image.new("RGB", (5, 4), (1, 2, 3)).save(os.path.join(OUT, "image.gif"))
Image.new("RGB", (5, 4), (1, 2, 3)).save(os.path.join(OUT, "image.bmp"))
Image.new("RGB", (5, 4), (1, 2, 3)).save(os.path.join(OUT, "image.webp"))
Image.new("RGB", (5, 4), (1, 2, 3)).save(os.path.join(OUT, "image.tiff"))
Image.new("CMYK", (5, 4), (1, 2, 3, 4)).save(os.path.join(OUT, "cmyk.jpg"))
Image.new("L", (5, 4), 9).save(os.path.join(OUT, "gray.jpg"))
big = Image.new("RGB", (16, 12))
big.putdata([((x * 16) % 256, (y * 21) % 256, ((x + y) * 9) % 256) for y in range(12) for x in range(16)])
big.save(os.path.join(OUT, "gradient.png"))
write("broken.png", b"not an image")
with open(os.path.join(OUT, "rgb.png"), "rb") as f:
    write("truncated.png", f.read()[:40])

# ----------------------------------------------------------------------------------- media
write("song.mp3", b"ID3\x03\x00\x00\x00\x00\x00\x00" + b"\x00" * 32)
write("clip.mp4", b"\x00\x00\x00\x18ftypmp42" + b"\x00" * 16)
write("clip.mkv", b"\x1a\x45\xdf\xa3" + b"\x00" * 12)
write("data.xyz", b"unknown media")
print("fixtures written to", OUT)
