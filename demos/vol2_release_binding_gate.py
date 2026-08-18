"""佐证 demo · 第二卷 ch20 —— 发布保证本体：按绑定取件，装包门禁拒收不合群成员。

书中论断（references/product-trustworthiness-book/ch20-assurance-ontology/
chapter.md §20.2–§20.3）：

  1)「同一基线不再是一条值得表扬的纪律，而是每份证据自带、门禁可核对、
     装包那一刻就能拒绝的属性」；
  2) 装包门禁：证据包成员的 boundTo 必须等于包声明的快照——书中场景：
     SWRegReport_r12 绑定的是下一版软件预览快照而非 RC18，SPARQL 查询
     「答案表只有一行」，门禁拒收并开出冲突处置单；
  3)「门禁的拒绝不是一条报错，而是一份留痕的处置开单」。

执行：按书中示意记法原样建图（RC18 快照、PKG 草案、r11 合规成员、r12 违例
成员），先跑书中给出的那条 SPARQL（应恰好返回一行 r12），再把门禁写成 SHACL
拒收装包草案；合规包（r12 换回 r11 修订）应放行。
"""

import sys
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal

REL = Namespace("https://product-trustworthiness.local/release#")

print("【书中论断】成员绑定必须等于包声明快照；违例查询答案表只有一行；拒绝是留痕的处置开单")
print("【锚点】ch20 chapter.md §20.2 示意记法 · §20.3 装包门禁\n")

def package_graph(include_stray: bool) -> Graph:
    g = Graph()
    g.add((REL.RC18, RDF.type, REL.ReleaseSnapshot))
    for item in ["HW_H32", "SW_184", "CAL_C41", "DRV_D7", "BASE_V12"]:
        g.add((REL.RC18, REL.configItem, REL[item]))
    pkg = REL.PKG_RC18_draft
    g.add((pkg, RDF.type, REL.EvidencePackage))
    g.add((pkg, REL.declaredSnapshot, REL.RC18))
    g.add((pkg, REL.memberListClosed, Literal(True)))
    g.add((pkg, REL.hasMember, REL.SWRegReport_r11))
    g.add((REL.SWRegReport_r11, REL.boundTo, REL.RC18))
    g.add((REL.SWRegReport_r11, REL.producedBy, REL.SWRegRun_r11))
    if include_stray:   # 小林组的预演修订：绑定下一版预览快照
        g.add((pkg, REL.hasMember, REL.SWRegReport_r12))
        g.add((REL.SWRegReport_r12, REL.boundTo, REL.Snapshot_NextSW_preview))
    return g

# A. 书中原样的 SPARQL：找出不合群的那一份（应恰好一行）
draft = package_graph(include_stray=True)
rows = list(draft.query("""
    PREFIX rel: <https://product-trustworthiness.local/release#>
    SELECT ?member ?binding WHERE {
        rel:PKG_RC18_draft rel:hasMember ?member .
        ?member rel:boundTo ?binding .
        FILTER ( ?binding != rel:RC18 )
    }"""))
one_row = len(rows) == 1 and rows[0][0] == REL.SWRegReport_r12
print(f"A. 书中 SPARQL 违例查询：{len(rows)} 行"
      f"{'，' + str(rows[0][0]).split('#')[-1] + ' → ' + str(rows[0][1]).split('#')[-1] if rows else ''}"
      f"（书：答案表只有一行）")

# B. 装包门禁 SHACL 化：拒收草案，消息即处置开单
GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rel: <https://product-trustworthiness.local/release#> .
rel:PackageBindingGate a sh:NodeShape ;
    sh:targetClass rel:EvidencePackage ;
    sh:sparql [
        sh:message "装包门禁不通过：成员绑定 != 包声明快照。处置：拒收该成员；生成冲突对象；通知看守角色，处置留痕" ;
        sh:select \"\"\"
            PREFIX rel: <https://product-trustworthiness.local/release#>
            SELECT $this ?value WHERE {
                $this rel:hasMember ?value ; rel:declaredSnapshot ?snap .
                ?value rel:boundTo ?b . FILTER(?b != ?snap)
            }
        \"\"\" ;
    ] .
"""
shapes = Graph().parse(data=GATE, format="turtle")
conforms_bad, _, text = pyshacl.validate(data_graph=draft, shacl_graph=shapes, advanced=True)
msg = next((l.strip() for l in text.splitlines() if l.strip().startswith("Message")), "")
print(f"B. 装包门禁（含违例成员 r12）：conforms={conforms_bad}（False=拒收成功）")
if msg:
    print(f"   {msg[:76]}")

# C. 修订确认回包后（书中 4:20 场景）：合规包放行
fixed = package_graph(include_stray=False)
conforms_ok, _, _ = pyshacl.validate(data_graph=fixed, shacl_graph=shapes, advanced=True)
print(f"C. 正确修订回包后：conforms={conforms_ok}（True=放行）")

ok = one_row and not conforms_bad and conforms_ok
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：违例查询恰好一行、门禁拒收草案并"
      f"给出处置开单、修正后放行——「同一基线成为门禁可核对的属性」在真实校验器上成立。")
sys.exit(0 if ok else 1)
