"""佐证 demo · 第二卷 ch12 —— 身份本体：族检查接住"把型号当单件"，桥接查询兑现记忆。

书中论断（references/product-trustworthiness-book/ch12-identity-ontology/
chapter.md §12.4）：

  1) 族检查：「单件别名不能挂接到型号上——这正是'把型号当单件'的座位错误
     被机器接住的样子」（PLM 记录最初被标成装配物料号，族检查当场亮灯）；
  2) 桥接查询（书中原样）：DUT-P07 与 SN-EPS-000417 是否同一台、凭什么——
     答案每一格指向可核对的受控记录；
  3)「机器判'同一'的资格，来自可核对的证据链，不来自名字的长相，
     也不来自任何人的记忆」。

数据：fixtures/ch12_bridge_bad.ttl（误挂型号）、ch12_bridge_good.ttl（改正后）。
执行：A. 误挂型号 → 族检查门禁拒绝；改挂单件条目 → 通过；
     B. 书中 SPARQL 原样执行，两条别名汇于同一单件、桥接证据在案。
"""

import sys
from _common import load_fixture

import pyshacl
from rdflib import Graph, Namespace

ID = Namespace("https://product-trustworthiness.local/identity#")

GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix id: <https://product-trustworthiness.local/identity#> .
id:AliasFamilyGate a sh:NodeShape ;
    sh:targetSubjectsOf id:aliasOf ;
    sh:sparql [
        sh:message "族检查：单件别名不能挂接到型号（设计定义）上——把型号当单件" ;
        sh:select \"\"\"
            PREFIX id: <https://product-trustworthiness.local/identity#>
            SELECT $this WHERE {
                $this id:aliasOf ?t .
                FILTER NOT EXISTS { ?t a id:PhysicalUnit }
            }
        \"\"\" ;
    ] .
"""
shapes = Graph().parse(data=GATE, format="turtle")

print("【书中论断】单件别名挂到型号被族检查接住；同一性判定凭证据链而非记忆")
print("【锚点】ch12 chapter.md §12.4 挂接四拍与桥接查询\n")

# A. 族检查
bad = load_fixture("ch12_bridge_bad")
conforms_bad, _, text = pyshacl.validate(data_graph=bad, shacl_graph=shapes, advanced=True)
msg = next((l.strip() for l in text.splitlines() if l.strip().startswith("Message")), "")
print(f"A. 别名挂到型号：conforms={conforms_bad}（False=族检查亮灯）")
if msg:
    print(f"   {msg[:60]}")
good = load_fixture("ch12_bridge_good")
conforms_ok, _, _ = pyshacl.validate(data_graph=good, shacl_graph=shapes, advanced=True)
print(f"   改挂单件条目后：conforms={conforms_ok}（True=链闭合，挂接写入）")

# B. 书中桥接查询原样执行
rows = list(good.query("""
    PREFIX id: <https://product-trustworthiness.local/identity#>
    SELECT ?unit ?bridge WHERE {
      id:Alias_DUT_P07    id:aliasOf  ?unit .
      id:Alias_SN_000417  id:aliasOf  ?unit .
      id:Alias_DUT_P07    id:evidence ?bridge .
    }"""))
same = len(rows) == 1 and rows[0][0] == ID.Unit_SN_000417
print(f"\nB. 桥接查询：{len(rows)} 行，判定同一 -> {str(rows[0][0]).split('#')[-1] if rows else '-'}，"
      f"桥接证据 {str(rows[0][1]).split('#')[-1] if rows else '-'}")

ok = (not conforms_bad) and conforms_ok and same
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：族检查拒绝型号挂接、证据链闭合后"
      f"「这就是同一台」成为可核对的查询结果，不再依赖记忆。")
sys.exit(0 if ok else 1)
