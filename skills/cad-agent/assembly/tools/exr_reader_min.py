#!/usr/bin/env python3
"""最小 OpenEXR 单部件扫描线读取器（NONE/ZIP/ZIPS 压缩，half/float/uint 通道），供表达层门禁读合成器输出。
只依赖 numpy + zlib。用法：read_channels(path) -> (dict name->2D ndarray, header dict)。"""
import struct, zlib
import numpy as np

def _read_attr_string(buf, i):
    j = buf.index(b"\x00", i); return buf[i:j].decode("latin-1"), j + 1

def read_channels(path):
    buf = open(path, "rb").read()
    magic, version = struct.unpack_from("<ii", buf, 0)
    assert magic == 20000630, "not an EXR"
    if version & 0x1000: raise ValueError("multi-part EXR not supported")
    i = 8; header = {}
    while True:
        if buf[i] == 0: i += 1; break
        name, i = _read_attr_string(buf, i); typ, i = _read_attr_string(buf, i)
        size = struct.unpack_from("<i", buf, i)[0]; i += 4
        data = buf[i:i + size]; i += size
        header[name] = (typ, data)
    ch = []; d = header["channels"][1]; k = 0
    while d[k] != 0:
        j = d.index(b"\x00", k); nm = d[k:j].decode("latin-1"); k = j + 1
        ptype = struct.unpack_from("<i", d, k)[0]; k += 4; k += 4; k += 8   # pLinear+reserved, xSampling, ySampling
        ch.append((nm, ptype))
    ch.sort(key=lambda c: c[0])
    xmin, ymin, xmax, ymax = struct.unpack_from("<iiii", header["dataWindow"][1], 0)
    W, H = xmax - xmin + 1, ymax - ymin + 1
    comp = header["compression"][1][0]
    lines_per_chunk = {0: 1, 1: 1, 2: 1, 3: 16}.get(comp)
    if lines_per_chunk is None: raise ValueError(f"unsupported compression {comp}")
    nchunks = (H + lines_per_chunk - 1) // lines_per_chunk
    offsets = struct.unpack_from(f"<{nchunks}Q", buf, i)
    dt = {0: (np.uint32, 4), 1: (np.float16, 2), 2: (np.float32, 4)}
    out = {nm: np.zeros((H, W), dtype=np.float32) for nm, _ in ch}
    for off in offsets:
        y, size = struct.unpack_from("<ii", buf, off); raw = buf[off + 8: off + 8 + size]
        nl = min(lines_per_chunk, ymax - y + 1)
        if comp in (2, 3):
            raw = zlib.decompress(raw)
            a = np.frombuffer(raw, dtype=np.uint8).copy()
            # EXR zip predictor + interleave undo
            a = a.astype(np.int32); a[1:] = a[1:] ; b = np.cumsum(a) & 0xFF  # delta decode
            b = b.astype(np.uint8); n = len(b); half = (n + 1) // 2
            de = np.empty(n, dtype=np.uint8); de[0::2] = b[:half]; de[1::2] = b[half:half + n // 2]
            raw = de.tobytes()
        p = 0
        for line in range(nl):
            for nm, ptype in ch:
                npt, bpp = dt[ptype]; cnt = W * bpp
                out[nm][y - ymin + line, :] = np.frombuffer(raw[p:p + cnt], dtype=npt).astype(np.float32); p += cnt
    return out, {"W": W, "H": H, "channels": [c[0] for c in ch], "compression": comp}

if __name__ == "__main__":
    import sys
    chans, hdr = read_channels(sys.argv[1]); print(hdr)
    for k, v in chans.items(): print(k, v.shape, float(v.min()), float(v.max()), int((v > 0.5).sum()))
