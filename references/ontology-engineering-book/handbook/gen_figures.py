#!/usr/bin/env python3
"""生成手册章首插图（gpt-image-2 via OpenAI兼容代理）。

用法：WELLAU_API_KEY=sk-xxx python3 gen_figures.py [name ...]
输出：figures/<name>.png；已存在的文件自动跳过。
"""
import base64
import json
import os
import sys
import time
import urllib.request

API = "https://api.wellau.com/v1/images/generations"
KEY = os.environ.get("WELLAU_API_KEY", "")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "figures")
os.makedirs(OUT, exist_ok=True)

# 提示词（单一来源）：书中各章展示的提示词与此处完全一致。
# 生成主链路：gen_image_ppt 后端（Responses API image_generation 工具，
# 与 Codex 共享 ~/.codex 配置）；构建 deck_plan 用 make_deck_plan.py。
STYLE = ("Flat vector textbook illustration, muted slate blue and teal palette "
         "with one warm amber accent, clean white background, thin precise "
         "line work, generous negative space, calm and professional. "
         "No text, no watermark, no signature.")

PROMPTS = {
    "cover": "An open book lying flat with a luminous three-dimensional "
             "knowledge graph rising from its pages: dozens of circular nodes "
             "joined by thin glowing lines forming an organized constellation, "
             "a few nodes highlighted in amber. " + STYLE,
    "ch01": "A two-part scene: left, a classical Greek column and a thinking "
            "silhouette surrounded by floating question marks, dreamy; right, "
            "a precise blueprint grid where a drafting compass draws a tidy "
            "hierarchical node diagram; one smooth arrow flows left to right. " + STYLE,
    "ch02": "A two-layer diagram metaphor: upper layer a tidy tree of hollow "
            "circles (class hierarchy) with the root on top; lower layer rows "
            "of small solid dots (instances) on a subtle grid, dashed lines "
            "linking dots up to their classes. " + STYLE,
    "ch03": "A circular engineering lifecycle loop of five rounded segments "
            "connected by smooth arrows; beside it a clipboard with a "
            "checklist and a pencil; in the loop center a small node diagram "
            "being assembled piece by piece. " + STYLE,
    "ch04": "An isometric stack of four translucent slabs like a technology "
            "layer cake, each layer slightly wider than the one above, thin "
            "connector lines running vertically through all layers, tiny "
            "triple-dot motifs on the bottom slab. " + STYLE,
    "ch05": "An inference engine metaphor: a transparent glass box of "
            "interlocking precise gears; small dots (facts) flow in on a thin "
            "conveyor from the left, glowing light bulbs (conclusions) emerge "
            "to the right; a faint branching proof tree in the background. " + STYLE,
    "ch06": "A clean 2x2 quadrant composition of four engineering domains: a "
            "factory robotic arm over a conveyor; an autonomous car with "
            "sensor waves; a building as a wireframe model with a small "
            "crane; an aircraft jet turbine in cutaway view. " + STYLE,
    "ch07": "A left-to-right data pipeline: stacked documents and two "
            "database cylinders on the left; a funnel and two filter gates on "
            "a conveyor in the middle; a large tidy glowing knowledge graph "
            "with a checkmark shield on the right. " + STYLE,
    "ch08": "A friendly rounded robot head emitting three speech bubbles that "
            "travel toward a tall translucent gate built from a lattice of "
            "connected nodes; two bubbles with check marks pass through, one "
            "with a cross mark bounces off; a clean document with a checkmark "
            "waits on the far right. " + STYLE,
    "ch09": "An isometric three-tier architecture: bottom tier a wide platform "
            "holding a knowledge graph of connected nodes; middle tier a "
            "server rack with query arrows flowing both ways; top tier a "
            "dashboard screen with simple charts beside a small factory icon "
            "with a robotic arm. " + STYLE,
}


def gen_once(name: str) -> str:
    path = os.path.join(OUT, f"{name}.png")
    if os.path.exists(path) and os.path.getsize(path) > 10000:
        return f"skip {name} (已存在)"
    # 注意：当前代理上游仅接受 size=1024x1024 + quality=low，其他组合返回500
    body = json.dumps({
        "model": "gpt-image-2",
        "prompt": PROMPTS[name],
        "size": "1024x1024",
        "quality": "low",
        "n": 1,
    }).encode()
    req = urllib.request.Request(API, data=body, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {KEY}",
    })
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            d = json.loads(r.read())
        item = d["data"][0]
        if item.get("b64_json"):
            raw = base64.b64decode(item["b64_json"])
        else:
            with urllib.request.urlopen(item["url"], timeout=120) as ir:
                raw = ir.read()
        with open(path, "wb") as f:
            f.write(raw)
        return f"ok   {name} ({len(raw)//1024} KB)"
    except Exception as e:
        return f"miss {name}: {str(e)[:60]}"


def missing(names):
    return [n for n in names
            if not (os.path.exists(os.path.join(OUT, f"{n}.png"))
                    and os.path.getsize(os.path.join(OUT, f"{n}.png")) > 10000)]


def main():
    names = sys.argv[1:] or list(PROMPTS)
    if not KEY:
        sys.exit("缺少 WELLAU_API_KEY 环境变量")
    # 收割模式：循环轮询缺失图片；上游处于"好窗口"时一轮可收多张
    for rnd in range(1, 41):
        todo = missing(names)
        if not todo:
            print("全部完成", flush=True)
            return
        print(f"--- 第{rnd}轮，缺 {len(todo)} 张: {' '.join(todo)}", flush=True)
        for n in todo:
            print(" ", gen_once(n), flush=True)
            time.sleep(5)
        if missing(names):
            time.sleep(60)
    print(f"达到轮数上限，仍缺: {' '.join(missing(names))}", flush=True)
    sys.exit(1)


if __name__ == "__main__":
    main()
