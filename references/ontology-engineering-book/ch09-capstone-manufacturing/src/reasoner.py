# reasoner.py
# 基于 owlready2 的本体推理引擎
# Ontology Reasoning Engine using owlready2
#
# 功能：
#   1. 加载OWL本体
#   2. 使用Pellet推理器执行规则推理
#   3. 冲突检测（能力冲突、时间冲突、状态冲突）
#   4. 工艺推荐（基于产品特征相似性）
#
# 安装依赖：pip install owlready2
# 注意：Pellet推理需要Java 8+环境

from owlready2 import *
from typing import List, Dict


class ManufacturingReasoner:
    """智能制造知识管理系统 - 本体推理引擎"""

    def __init__(self, ontology_path: str):
        """
        初始化推理引擎，加载本体文件

        Args:
            ontology_path: OWL本体文件路径（支持.owl/.ttl格式）
        """
        self.onto = get_ontology(ontology_path).load()
        print(f"已加载本体: {ontology_path}")
        print(f"  类数量: {len(list(self.onto.classes()))}")
        print(f"  属性数量: {len(list(self.onto.properties()))}")
        print(f"  实例数量: {len(list(self.onto.individuals()))}")

    def run_reasoner(self):
        """
        执行Pellet推理器
        推理后，本体中会新增推导出的知识（隐含三元组）
        """
        print("\n正在运行 Pellet 推理器...")
        with self.onto:
            sync_reasoner_pellet(
                infer_property_values=True,       # 推导属性值
                infer_data_property_values=True   # 推导数据属性值
            )
        print("推理完成")

    def check_conflicts(self) -> List[Dict]:
        """
        检测所有任务冲突（运行SWRL规则推理后查询冲突实例）

        Returns:
            冲突列表，每个元素包含 type, task, description
        """
        # 运行推理以激活SWRL规则
        self.run_reasoner()

        conflicts = []

        # 查询能力冲突（设备无法加工所需材料）
        if hasattr(self.onto, 'CapabilityConflict'):
            for c in list(self.onto.search(type=self.onto.CapabilityConflict)):
                conflicts.append({
                    "type": "capability",
                    "type_zh": "能力冲突",
                    "task": c.name,
                    "description": f"任务 {c.name}: 分配的设备无法加工所需材料"
                })

        # 查询时间冲突（设备时间段重叠）
        if hasattr(self.onto, 'TimeConflict'):
            for c in list(self.onto.search(type=self.onto.TimeConflict)):
                conflicts.append({
                    "type": "time",
                    "type_zh": "时间冲突",
                    "task": c.name,
                    "description": f"任务 {c.name}: 设备时间段存在重叠"
                })

        # 查询状态冲突（设备处于Running状态）
        if hasattr(self.onto, 'StatusConflict'):
            for c in list(self.onto.search(type=self.onto.StatusConflict)):
                conflicts.append({
                    "type": "status",
                    "type_zh": "状态冲突",
                    "task": c.name,
                    "description": f"任务 {c.name}: 设备当前状态不可用"
                })

        return conflicts

    def recommend_process(self, product_name: str) -> List[Dict]:
        """
        基于产品特征相似性推荐工艺

        Args:
            product_name: 产品实例名称

        Returns:
            推荐工艺列表，按置信度排序
        """
        product = self.onto.search_one(iri=f"*#{product_name}")
        if not product:
            print(f"产品未找到: {product_name}")
            return []

        self.run_reasoner()

        recommendations = []
        if hasattr(product, 'recommendedProcess'):
            for proc in product.recommendedProcess:
                confidence_vals = getattr(proc, 'confidence', [])
                confidence = confidence_vals[0] if confidence_vals else 0.8
                comment_vals = getattr(proc, 'comment', [])
                description = str(comment_vals[0]) if comment_vals else ''
                recommendations.append({
                    "name": proc.name,
                    "confidence": confidence,
                    "description": description
                })

        # 按置信度降序排序
        recommendations.sort(key=lambda x: x['confidence'], reverse=True)
        return recommendations

    def get_equipment_for_material(self, material_name: str) -> List[str]:
        """
        查询能加工指定材料的设备列表

        Args:
            material_name: 材料类名称（如 'Aluminum'）
        """
        material_cls = getattr(self.onto, material_name, None)
        if not material_cls:
            print(f"材料类未找到: {material_name}")
            return []

        results = []
        if hasattr(self.onto, 'Equipment'):
            for equipment in self.onto.Equipment.instances():
                for material in getattr(equipment, 'canProcess', []):
                    if isinstance(material, material_cls):
                        # equipment.label is a LocalizedStringList; use index access
                        label_list = equipment.label
                        label = label_list[0] if label_list else equipment.name
                        results.append(str(label))
                        break
        return results


def main():
    """演示推理引擎的基本用法"""

    # 初始化推理引擎
    reasoner = ManufacturingReasoner("src/manufacturing.owl")

    # 1. 检测冲突
    print("\n=== 冲突检测 ===")
    conflicts = reasoner.check_conflicts()
    if not conflicts:
        print("无冲突（需要ABox实例数据才能触发SWRL规则）")
    else:
        for c in conflicts:
            print(f"[{c['type_zh']}] {c['description']}")

    # 2. 工艺推荐
    print("\n=== 工艺推荐（产品 Product_001） ===")
    recommendations = reasoner.recommend_process("Product_001")
    if not recommendations:
        print("暂无推荐（需要更多产品实例数据）")
    else:
        for r in recommendations:
            print(f"  工艺: {r['name']}  置信度: {r['confidence']:.1%}")


if __name__ == "__main__":
    main()
