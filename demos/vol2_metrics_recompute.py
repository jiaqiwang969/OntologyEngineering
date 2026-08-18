"""佐证 demo · 第二卷 ch06/ch16 —— SPFM/LFM 硬件度量算例全程机器复算。

书中论断（references/product-trustworthiness-book/ch06-hardware-development/
examples/metrics-walkthrough.md，§6.4–§6.6 的可重算教学数据）：

  1) 五类失效率闭合：λ = λ_SPF + λ_RF + λ_MPF,DP + λ_MPF,L + λ_S（逐行与总计）；
  2) 400 FIT 基线：SPFM_base = 95.00%，LFM_base = 94.74%（LFM 分母须先扣
     单点与残余贡献，不能直接用 Σλ）；
  3) U7 模式分布 49.5% = 55%×90%、5.5% = 55%×10%，自身 3 FIT 闭合；
  4) 加 U7 后：Σλ=403，SPFM_new = 95.48%，LFM_new = 94.76%；
  5) 变式 5.1（诊断超出 MPFDTI）：LFM = 94.37%，SPFM 不变；
  6) 对照 ASIL D 项目目标：SPFM 95.48% < 99% 未达标，LFM 94.76% ≥ 90% 达标。

执行：只从原始表格出发，用代码重走全部算式，与书中打印值逐一比对；
最后用 SHACL「got ≥ need」门禁复演目标对照（未达标必须被标出）。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import sys

# 书中 §2 基线表（原始数据，单位 FIT）
BASELINE = {
    #                Σλ    SPF  RF  MPF_DP MPF_L  S
    "传感器":       (100,   0,   5,  20,    5,   70),
    "控制器":       (200,   0,   8,  60,   12,  120),
    "驱动功率级":   (98,    0,   5,  20,    3,   70),
    "未监控供电":   (2,     2,   0,   0,    0,    0),
}

print("【书中论断】五类闭合；SPFM 95.00→95.48%；LFM 94.74→94.76%；变式 94.37%；"
      "对照 ASIL D 目标 SPFM 未达标、LFM 达标")
print("【锚点】ch06 examples/metrics-walkthrough.md §1–§6\n")

checks = []

# 1) 闭合校验
closure = all(total == spf + rf + dp + lat + s
              for total, spf, rf, dp, lat, s in BASELINE.values())
totals = [sum(col) for col in zip(*BASELINE.values())]
closure &= totals[0] == sum(totals[1:])
checks.append(("五类失效率逐行+总计闭合", closure, f"Σλ={totals[0]}"))

# 2) 基线度量
sl, spf, rf, dp, lat, s = totals
spfm_base = 1 - (spf + rf) / sl
lfm_base = 1 - lat / (sl - spf - rf)
checks.append(("SPFM_base = 95.00%", round(spfm_base * 100, 2) == 95.00, f"{spfm_base:.2%}"))
checks.append(("LFM_base = 94.74%（分母 380 非 Σλ）",
               round(lfm_base * 100, 2) == 94.74, f"{lfm_base:.2%}"))

# 3) U7 模式分布闭合（3 FIT；49.5%=55%×90%，5.5%=55%×10%）
u7 = 3.0
u7_s, u7_dp, u7_lat = 0.45 * u7, 0.55 * 0.90 * u7, 0.55 * 0.10 * u7
checks.append(("U7 分布 1.350/1.485/0.165 闭合",
               (round(u7_dp, 3), round(u7_lat, 3)) == (1.485, 0.165)
               and abs(u7_s + u7_dp + u7_lat - u7) < 1e-9,
               f"{u7_s:.3f}/{u7_dp:.3f}/{u7_lat:.3f}"))

# 4) 加 U7 后（原供电 2 FIT SPF → 0.2 RF + 1.8 MPF,DP）
sl_new = sl + u7                       # 403
spf_rf_new = rf + 0.2                  # 18 + 0.2（原 2 FIT SPF 迁出）
spfm_new = 1 - spf_rf_new / sl_new
lat_new = lat + u7_lat                 # 20 + 0.165
lfm_new = 1 - lat_new / (sl_new - spf_rf_new)
checks.append(("SPFM_new = 95.48%", round(spfm_new * 100, 2) == 95.48, f"{spfm_new:.2%}"))
checks.append(("LFM_new = 94.76%", round(lfm_new * 100, 2) == 94.76, f"{lfm_new:.2%}"))

# 5) 变式 5.1：U7 危险侧 1.65 FIT 全转潜伏
lfm_late = 1 - (lat + 1.65) / (sl_new - spf_rf_new)
checks.append(("变式 5.1 LFM = 94.37%（SPFM 不变）",
               round(lfm_late * 100, 2) == 94.37, f"{lfm_late:.2%}"))

for name, ok, val in checks:
    print(f"  [{'✓' if ok else '✗'}] {name:38s} 复算 {val}")

# 6) 目标对照：书中 §8 说模型有「got >= need 的局部 Shape」——用 SHACL 复演
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal, XSD

M = Namespace("https://product-trustworthiness.local/metric#")
gate = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix m: <https://product-trustworthiness.local/metric#> .
m:TargetGate a sh:NodeShape ;
    sh:targetClass m:MetricRecord ;
    sh:sparql [
        sh:message "got < need：度量未达到所选项目目标" ;
        sh:select \"\"\"
            PREFIX m: <https://product-trustworthiness.local/metric#>
            SELECT $this WHERE { $this m:got ?g ; m:need ?n . FILTER(?g < ?n) }
        \"\"\" ;
    ] .
"""
g = Graph()
for rid, got, need in [("SPFM_walkthrough", spfm_new * 100, 99.0),
                       ("LFM_walkthrough", lfm_new * 100, 90.0)]:
    node = M[rid]
    g.add((node, RDF.type, M.MetricRecord))
    g.add((node, M.got, Literal(round(got, 2), datatype=XSD.decimal)))
    g.add((node, M.need, Literal(need, datatype=XSD.decimal)))
conforms, _, text = pyshacl.validate(
    data_graph=g, shacl_graph=Graph().parse(data=gate, format="turtle"), advanced=True)
flagged = text.count("got < need")
print(f"\n目标门禁（ASIL D：SPFM≥99、LFM≥90）：conforms={conforms}，被标记 {max(flagged-1,1) if not conforms else 0} 项")
gate_ok = (not conforms)   # SPFM 95.48<99 必须被标出；LFM 94.76≥90 不应被标
target_verdict = not conforms and "SPFM_walkthrough" in text and "LFM_walkthrough" not in text

ok = all(c[1] for c in checks) and target_verdict
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：全部算式复算与书中打印值一致，"
      f"目标门禁只标出 SPFM 未达标（与书 §6 结论一字不差）。")
sys.exit(0 if ok else 1)
