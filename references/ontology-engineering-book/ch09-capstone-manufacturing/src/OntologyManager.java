// OntologyManager.java
// 本体加载与管理 - 使用 Apache Jena 框架
// Ontology Loading and Management using Apache Jena
//
// Maven 依赖:
// <dependency>
//   <groupId>org.apache.jena</groupId>
//   <artifactId>apache-jena-libs</artifactId>
//   <version>4.10.0</version>
// </dependency>

import org.apache.jena.ontology.*;
import org.apache.jena.rdf.model.ModelFactory;
import java.util.List;

/**
 * 本体管理器：负责加载、查询和管理OWL本体
 * Ontology Manager: responsible for loading, querying and managing OWL ontologies
 */
public class OntologyManager {

    // 本体命名空间
    private static final String NS = "http://example.org/manufacturing#";

    // Jena本体模型（带推理器）
    private OntModel model;

    /**
     * 加载本体文件
     * Load ontology from file path
     * @param filePath 本体文件路径（支持OWL/TTL/RDF格式）
     */
    public void loadOntology(String filePath) {
        // OWL_DL_MEM_RULE_INF: OWL DL语义 + 内置规则推理器
        model = ModelFactory.createOntologyModel(
            OntModelSpec.OWL_DL_MEM_RULE_INF);

        // 读取文件（TTL格式，可改为"RDF/XML"或"N3"）
        model.read(filePath, "TTL");

        System.out.println("已加载本体类数量: " +
            model.listClasses().toList().size());
        System.out.println("已加载实例数量: " +
            model.listIndividuals().toList().size());
    }

    /**
     * 获取某个类的所有直接子类
     * Get all direct subclasses of a given class
     */
    public List<OntClass> getSubClasses(String className) {
        OntClass cls = model.getOntClass(NS + className);
        if (cls == null) {
            System.err.println("类未找到: " + className);
            return List.of();
        }
        // true = 只返回直接子类，false = 包含所有子孙类
        return cls.listSubClasses(true).toList();
    }

    /**
     * 获取Jena本体模型（供QueryService使用）
     */
    public OntModel getModel() {
        return model;
    }

    /**
     * 主函数演示
     */
    public static void main(String[] args) {
        OntologyManager manager = new OntologyManager();
        manager.loadOntology("src/manufacturing.owl");

        // 打印设备类的所有子类
        System.out.println("\n设备子类：");
        manager.getSubClasses("Equipment").forEach(cls ->
            System.out.println("  - " + cls.getLocalName()));
    }
}
