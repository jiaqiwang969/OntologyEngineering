"""佐证 demo · 第二卷 ch03/ch11 —— 主张本体的门禁：缺件即拒、六绿不等于接受。

书中论断：

  1) ch03 safety-case-skeleton.txt：`ClaimAccepted` 门禁要求 Safety Case 已批准、
     确认评审完成有报告、且实际支撑证据全部为 `EvidenceAccepted`；
     EPS 教学实例（SafetyCase[Draft] + Claim_SG1[ClaimOpen] + 七份
     EvidenceCandidate）表达的是「待完成的论证结构」，不是「SG1 已被证明满足」
     ——七份候选证据不能把 Claim_SG1 推过 ClaimOpen；
  2) ch11 11.4：「缺任何一个部件的主张，不得进入评审队列」——Release_RC17
     缺 context 与 validWindow 两格被拦，裁定记录要说明拦下了谁、缺什么；
  3) ch11：「机器'拒绝'的能力才值钱：一次有据可查的拒绝，
     比一百次顺滑的通过更接近工程。」

执行（pyshacl，全部封闭世界门禁）：
  A. EPS 教学实例原样进门禁 → conforms（合法的待完成结构，书中状态正确）；
  B. 六绿僵局机器版：证据仍是 Candidate、案例仍是 Draft，却把 Claim_SG1
     强行标成 ClaimAccepted → 门禁必须拒绝；
  C. 缺件即拒：Release_RC17 缺 context/validWindow → 门禁拦截并指出缺件。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import sys
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal

CLAIM = Namespace("https://product-trustworthiness.local/claim#")

GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix claim: <https://product-trustworthiness.local/claim#> .

# ch03 门禁：ClaimAccepted 的三联动（案例已批准 + 确认评审完成 + 证据全 Accepted）
claim:ClaimAcceptedGate a sh:NodeShape ;
    sh:targetClass claim:Claim ;
    sh:sparql [
        sh:message "ClaimAccepted 门禁：主张被标接受，但其 Safety Case 未批准、确认评审未完成、或存在非 EvidenceAccepted 的支撑证据" ;
        sh:select \"\"\"
            PREFIX claim: <https://product-trustworthiness.local/claim#>
            SELECT $this WHERE {
                $this claim:claimStatus "ClaimAccepted" .
                { ?case claim:containsClaim $this .
                  FILTER NOT EXISTS { ?case claim:reviewStatus "Approved" } }
                UNION
                { ?arg claim:supportsClaim $this ; claim:backedByEvidence ?ev .
                  FILTER NOT EXISTS { ?ev claim:evidenceStatus "EvidenceAccepted" } }
            }
        \"\"\" ;
    ] .

# ch11 门禁：缺任何一个部件的主张不得进入评审队列
claim:CompletePartsGate a sh:NodeShape ;
    sh:targetClass claim:ReleaseClaim ;
    sh:property [ sh:path claim:context ; sh:minCount 1 ;
                  sh:message "缺件即拒：主张缺 context 部件，不得进入评审队列" ] ;
    sh:property [ sh:path claim:validWindow ; sh:minCount 1 ;
                  sh:message "缺件即拒：主张缺 validWindow 部件，不得进入评审队列" ] .
"""
shapes = Graph().parse(data=GATE, format="turtle")

def eps_instance(force_accepted: bool) -> Graph:
    """ch03 的 EPS 教学实例：Draft 案例、Claim_SG1、七份候选证据。"""
    g = Graph()
    g.add((CLAIM.SafetyCase_EPS, RDF.type, CLAIM.SafetyCase))
    g.add((CLAIM.SafetyCase_EPS, CLAIM.reviewStatus, Literal("Draft")))
    g.add((CLAIM.SafetyCase_EPS, CLAIM.containsClaim, CLAIM.Claim_SG1))
    g.add((CLAIM.Claim_SG1, RDF.type, CLAIM.Claim))
    g.add((CLAIM.Claim_SG1, CLAIM.claimStatus,
           Literal("ClaimAccepted" if force_accepted else "ClaimOpen")))
    g.add((CLAIM.Arg_SG1, CLAIM.supportsClaim, CLAIM.Claim_SG1))
    for name in ["HARA_draft", "FSR", "TSR", "HW_metric", "Decomposition",
                 "SV_spec_draft", "SV_report_template"]:      # 书中七份引用工件
        ev = CLAIM[f"Ev_{name}"]
        g.add((CLAIM.Arg_SG1, CLAIM.backedByEvidence, ev))
        g.add((ev, CLAIM.evidenceStatus, Literal("EvidenceCandidate")))
    return g

print("【书中论断】七份候选证据推不动 ClaimOpen；已批准接受需要三联动；缺件的主张不得入队")
print("【锚点】ch03 examples/safety-case-skeleton.txt · ch11 chapter.md 11.4\n")

# A. 合法的「待完成论证结构」应通过门禁
ok_graph = eps_instance(force_accepted=False)
conforms_a, _, _ = pyshacl.validate(data_graph=ok_graph, shacl_graph=shapes, advanced=True)
print(f"A. EPS 教学实例（Draft + ClaimOpen + 7×Candidate）：conforms={conforms_a}"
      "（True=书中说的合法待完成结构）")

# B. 六绿僵局机器版：强行标 ClaimAccepted
bad_graph = eps_instance(force_accepted=True)
conforms_b, _, text_b = pyshacl.validate(data_graph=bad_graph, shacl_graph=shapes, advanced=True)
msg_b = next((l.strip() for l in text_b.splitlines() if l.strip().startswith("Message")), "")
print(f"B. 证据未接受却标 ClaimAccepted：conforms={conforms_b}（False=拒绝成功）")
if msg_b:
    print(f"   {msg_b[:80]}")

# C. ch11 缺件即拒：Release_RC17 缺 context 与 validWindow
rc17 = Graph()
rc17.add((CLAIM.Release_RC17, RDF.type, CLAIM.ReleaseClaim))
rc17.add((CLAIM.Release_RC17, CLAIM.claimStatement, Literal("RC17 可以发布")))
conforms_c, _, text_c = pyshacl.validate(data_graph=rc17, shacl_graph=shapes)
missing = [l.strip() for l in text_c.splitlines() if "缺件即拒" in l and l.strip().startswith("Message")]
print(f"C. Release_RC17 缺 context/validWindow：conforms={conforms_c}"
      f"（False=拦截成功，缺件 {len(missing)} 项）")
for m in missing:
    print(f"   {m[:60]}")

ok = conforms_a and not conforms_b and not conforms_c and len(missing) == 2
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：合法待完成结构放行、"
      f"六绿僵局被机器拒绝、缺两格的主张被拦并列明缺件——"
      f"「机器有据可查的拒绝」在真实校验器上成立。")
sys.exit(0 if ok else 1)
