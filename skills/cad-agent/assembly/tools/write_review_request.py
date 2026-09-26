#!/usr/bin/env python3
"""生成 <run>/REVIEW-REQUEST.md（人工复看请求；无回执时验收包 A10.5/6 如实 FAIL，成片只是评审候选）。
所有数字从 pipeline-state / state-chain / media-qc 现读，不手填。
用法：python write_review_request.py --case-dir . --run FILM_v001"""
import argparse, json, hashlib
from pathlib import Path
from datetime import datetime, timezone
ap = argparse.ArgumentParser(); ap.add_argument("--case-dir", required=True, type=Path); ap.add_argument("--run", required=True)
a = ap.parse_args(); D = a.case_dir / "装配动画" / a.run
st = json.loads((D / "pipeline-state.v1.json").read_text(encoding="utf-8")); asm = st["assembly"]
chain = json.loads((D / "state-chain.json").read_text(encoding="utf-8")) if (D / "state-chain.json").exists() else {}
qc = json.loads((D / "media-qc.v1.json").read_text(encoding="utf-8")) if (D / "media-qc.v1.json").exists() else {}
mp4 = qc.get("mp4", {})
lines = [f"# 人工复看请求 · {a.run}", "", f"生成时间：{datetime.now(timezone.utc).isoformat()}", "",
 "## 成片", f"- 文件：`{mp4.get('path','（未生成）')}`", f"- sha256：`{mp4.get('sha256','')}`", f"- 帧数/时长：{mp4.get('frames','?')} 帧 / {mp4.get('duration_s','?')} s", "",
 "## 冻结状态闭合", f"- 目标动件 {asm['closure']['targets']} = 已动画 {asm['closure']['animated']} + 未动画 {asm['closure']['not_animated']}（{asm['closure']['status']}）",
 f"- 运动章 {asm['summary']['operations']}，presence-only 结合章 {asm['summary']['presence_chapters']}", "",
 "## 复看要求（X1+ A8/A10）", "- 全片逐章观看，不抽样；每章接近位帧核对方向（K-DEL-02）。", "- 任何肉眼可见穿模、方向反、件提前出场、相机丢件，记录时间码与工序号。",
 "- 未动画件在片尾标注卡如实列出；不得要求表达层补造运动。", "",
 "## 回执格式（另存为 review-receipt.json）", "```json", json.dumps({"full_film_review_complete": False, "reviewer": "<姓名>", "reviewed_at": "<ISO 时间>", "video_sha256": mp4.get("sha256", ""), "findings": [{"time_s": 0, "op": "", "issue": ""}], "decision": "ACCEPT|REJECT"}, ensure_ascii=False, indent=1), "```", "",
 "## 主张边界", "- 本片只表达已认证的单轴平移安装运动；不主张扭矩、螺纹、压装、接线与物理放行。"]
(D / "REVIEW-REQUEST.md").write_text("\n".join(lines), encoding="utf-8"); print("写出", D / "REVIEW-REQUEST.md")
