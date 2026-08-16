# 本体引导的Agent参考实现：提议 -> 校验 -> 执行 -> 审计
# Ontology-Guided Agent: propose -> validate -> execute -> audit
#
# 演示用最小实现：本体用字典模拟（生产中替换为rdflib/owlready2 + SPARQL端点）
# 运行：python3 ontology-guided-agent.py

from dataclasses import dataclass, field
from datetime import datetime, timezone


# ------------------------------
# 1. 本体层（生产中=OWL本体+知识图谱，此处用字典模拟TBox/ABox）
# ------------------------------
ONTOLOGY = {
    # 设备状态机：本体中的合法状态转换公理
    # Running -> Maintenance 合法；Fault -> Running 非法（必须先维修）
    "status_transitions": {
        ("Running", "Maintenance"), ("Idle", "Maintenance"),
        ("Idle", "Running"), ("Running", "Idle"),
        ("Maintenance", "Idle"), ("Fault", "Maintenance"),
    },
    # 动作的前置约束（来自公理：维护中的设备不能派工）
    "action_preconditions": {
        "assign_task":    {"allowed_status": {"Idle"}},
        "set_maintenance": {"allowed_status": {"Running", "Idle", "Fault"}},
    },
    # ABox：当前事实
    "equipment": {
        "Lathe_003": {"type": "CNCLathe", "status": "Running", "power": 15.5},
        "Mill_007":  {"type": "CNCMillingMachine", "status": "Fault", "power": 11.0},
    },
}

VALID_ACTIONS = {"assign_task", "set_maintenance", "set_status", "query"}


@dataclass
class AuditRecord:
    """PROV风格审计记录：每个决策可溯源"""
    timestamp: str
    action: str
    target: str
    verdict: str          # APPROVED / REJECTED
    reason: str
    evidence: dict = field(default_factory=dict)


# ------------------------------
# 2. 本体护栏：对LLM提议的动作做形式校验
# ------------------------------
class OntologyGuardrail:
    def __init__(self, onto):
        self.onto = onto
        self.audit_log: list[AuditRecord] = []

    def _audit(self, action, target, verdict, reason, evidence=None):
        rec = AuditRecord(
            timestamp=datetime.now(timezone.utc).isoformat(),
            action=action, target=target,
            verdict=verdict, reason=reason, evidence=evidence or {},
        )
        self.audit_log.append(rec)
        return rec

    def validate(self, proposal: dict) -> AuditRecord:
        """三道检查：词汇 -> 实体 -> 公理（与幻觉控制三层一一对应）"""
        action = proposal.get("action", "")
        target = proposal.get("target", "")

        # 检查1 词汇约束：动作必须在本体定义的动作集中
        if action not in VALID_ACTIONS:
            return self._audit(action, target, "REJECTED",
                               f"未知动作 '{action}'（词汇幻觉）")

        # 检查2 实体存在性：目标必须是ABox中的个体
        equip = self.onto["equipment"].get(target)
        if equip is None:
            return self._audit(action, target, "REJECTED",
                               f"实体 '{target}' 不存在于知识库（实体幻觉）")

        # 检查3 公理约束：状态机与前置条件
        cur = equip["status"]
        if action == "set_status":
            new = proposal.get("value", "")
            if (cur, new) not in self.onto["status_transitions"]:
                return self._audit(action, target, "REJECTED",
                                   f"非法状态转换 {cur} -> {new}（违反状态机公理）",
                                   {"current_status": cur})
        pre = self.onto["action_preconditions"].get(action)
        if pre and cur not in pre["allowed_status"]:
            return self._audit(action, target, "REJECTED",
                               f"前置条件不满足：{action} 要求状态 "
                               f"{pre['allowed_status']}，当前为 {cur}",
                               {"current_status": cur})

        return self._audit(action, target, "APPROVED", "通过全部本体校验",
                           {"current_status": cur})


# ------------------------------
# 3. Agent主循环：LLM提议（此处用桩模拟）-> 校验 -> 执行 -> 审计
# ------------------------------
def llm_propose(user_request: str) -> dict:
    """生产中此处调用LLM把自然语言转为结构化动作提议；
    演示用关键词桩模拟（含一个会被拦截的坏提议）"""
    table = {
        "把3号车床转入维护": {"action": "set_status",
                              "target": "Lathe_003", "value": "Maintenance"},
        "让7号铣床直接开工": {"action": "set_status",
                              "target": "Mill_007", "value": "Running"},
        "给幻影设备派活":    {"action": "assign_task", "target": "Ghost_999"},
    }
    return table.get(user_request, {"action": "query", "target": "Lathe_003"})


def execute(proposal: dict, onto) -> str:
    """只有APPROVED的提议才会到达这里"""
    if proposal["action"] == "set_status":
        onto["equipment"][proposal["target"]]["status"] = proposal["value"]
        return f"{proposal['target']} 状态已更新为 {proposal['value']}"
    return "（查询/派工执行略）"


def agent_turn(user_request: str, guard: OntologyGuardrail):
    print(f"\n用户：{user_request}")
    proposal = llm_propose(user_request)
    verdict = guard.validate(proposal)
    print(f"  提议：{proposal}")
    print(f"  裁决：{verdict.verdict} —— {verdict.reason}")
    if verdict.verdict == "APPROVED":
        print(f"  执行：{execute(proposal, guard.onto)}")
    else:
        print("  执行：已阻断（向用户解释原因并请求人工确认）")


if __name__ == "__main__":
    guard = OntologyGuardrail(ONTOLOGY)
    agent_turn("把3号车床转入维护", guard)   # 合法：Running->Maintenance
    agent_turn("让7号铣床直接开工", guard)   # 拦截：Fault->Running 违反状态机
    agent_turn("给幻影设备派活", guard)      # 拦截：实体不存在

    print("\n=== 审计日志（PROV风格，可序列化为RDF归档）===")
    for rec in guard.audit_log:
        print(f"  [{rec.timestamp}] {rec.action}({rec.target}) "
              f"-> {rec.verdict}: {rec.reason}")
