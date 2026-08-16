#!/usr/bin/env python3
"""从 gen_figures.py 的 PROMPTS（单一来源）生成 gen_image_ppt 的 deck_plan.json。

十张书籍插图作为十个互不依赖的页面提交；页序与 FIG_ORDER 一致，
生成后 page_00N.png 按该顺序回拷为 figures/<name>.png。
"""
import json
from pathlib import Path

from gen_figures import PROMPTS

HERE = Path(__file__).resolve().parent
FIG_ORDER = ["cover", "ch01", "ch02", "ch03", "ch04",
             "ch05", "ch06", "ch07", "ch08", "ch09"]

TITLES = {
    "cover": "封面：知识图谱从书页升起",
    "ch01": "第1章 从哲学之问到工程之答",
    "ch02": "第2章 类层次与实例的两层世界",
    "ch03": "第3章 本体构建生命周期循环",
    "ch04": "第4章 语义网技术栈分层",
    "ch05": "第5章 推理机：事实到结论",
    "ch06": "第6章 四个工程应用领域",
    "ch07": "第7章 数据到知识图谱的流水线",
    "ch08": "第8章 本体作为大模型守门人",
    "ch09": "第9章 实战系统三层架构",
}

LAYOUT = ("Single full-bleed illustration, main subject centered and slightly "
          "above the optical middle, generous margins on all sides, balanced "
          "composition. Strictly no text, no labels, no watermark, no border.")

plan = {
    "title": "《工程本体论》配套手册章首插图",
    "aspect_ratio": "3:2",
    "image_size": "1536x1024",
    "language": "zh",
    "outputs": ["images"],
    "max_image_workers": 2,
    "timeout_sec": 600,
    "style": ("Consistent flat vector textbook illustration system across all "
              "pages: muted slate blue and teal palette with one warm amber "
              "accent, clean white background, thin precise line work, "
              "generous negative space, calm professional academic mood. "
              "Absolutely no text or lettering of any kind, no watermark."),
    "pages": [
        {
            "page_id": name,
            "title": TITLES[name],
            "points": [],
            "description": {
                "details": PROMPTS[name],
                "layout": LAYOUT,
            },
        }
        for name in FIG_ORDER
    ],
}

out = HERE / "deck_plan.json"
out.write_text(json.dumps(plan, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"已生成 {out}（{len(plan['pages'])} 页）")
