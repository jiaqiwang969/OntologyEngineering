// QueryService.java
// SPARQL查询服务 - 使用 Apache Jena 执行本体查询
// SPARQL Query Service using Apache Jena
//
// 实现核心能力问题（Competency Questions）的查询接口

import org.apache.jena.ontology.OntModel;
import org.apache.jena.query.Query;
import org.apache.jena.query.QueryExecution;
import org.apache.jena.query.QueryFactory;
import org.apache.jena.query.QuerySolution;
import org.apache.jena.query.ResultSet;
import java.util.ArrayList;
import java.util.List;

/**
 * SPARQL查询服务：提供面向业务的本体查询接口
 * Query Service: provides business-oriented ontology query interface
 */
public class QueryService {

    private static final String NS = "http://example.org/manufacturing#";
    private OntModel model;

    public QueryService(OntModel model) {
        this.model = model;
    }

    /**
     * CQ1: 哪些设备可以加工指定材料？
     * Which equipment can process the specified material?
     *
     * @param material 材料名称（如 "Aluminum"）
     * @return 可加工该材料的设备列表
     */
    public List<String> getEquipmentForMaterial(String material) {
        // SPARQL查询：查找能加工指定材料的设备
        String sparql =
            "PREFIX : <http://example.org/manufacturing#>\n" +
            "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n" +
            "SELECT ?equipment ?name\n" +
            "WHERE {\n" +
            "    ?equipment a :Equipment ;\n" +
            "               :canProcess :" + material + " ;\n" +
            "               rdfs:label ?name .\n" +
            "    FILTER(lang(?name) = \"zh\")\n" +
            "}\n" +
            "ORDER BY ?name";

        List<String> results = new ArrayList<>();

        // 执行查询（使用Jena 4.x推荐的构建器API）
        Query query = QueryFactory.create(sparql);
        try (QueryExecution qexec = QueryExecution.create()
                .query(query)
                .model(model)
                .build()) {
            ResultSet rs = qexec.execSelect();
            while (rs.hasNext()) {
                QuerySolution soln = rs.nextSolution();
                String uri = soln.getResource("equipment").getURI();
                String name = soln.getLiteral("name").getString();
                results.add(name + " (" + uri + ")");
            }
        }

        return results;
    }

    /**
     * CQ2: 查询所有空闲设备
     * Find all idle equipment
     */
    public List<String> getIdleEquipment() {
        String sparql =
            "PREFIX : <http://example.org/manufacturing#>\n" +
            "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n" +
            "SELECT ?equipment ?name ?location\n" +
            "WHERE {\n" +
            "    ?equipment a :Equipment ;\n" +
            "               :hasStatus :Idle ;\n" +
            "               rdfs:label ?name .\n" +
            "    OPTIONAL {\n" +
            "        ?equipment :locatedIn ?workCell .\n" +
            "        ?workCell rdfs:label ?location .\n" +
            "    }\n" +
            "    FILTER(lang(?name) = \"zh\")\n" +
            "}";

        List<String> results = new ArrayList<>();
        Query query = QueryFactory.create(sparql);
        try (QueryExecution qexec = QueryExecution.create()
                .query(query)
                .model(model)
                .build()) {
            ResultSet rs = qexec.execSelect();
            while (rs.hasNext()) {
                QuerySolution soln = rs.nextSolution();
                String name = soln.getLiteral("name").getString();
                String location = soln.contains("location")
                    ? soln.getLiteral("location").getString()
                    : "未知位置";
                results.add(name + " @ " + location);
            }
        }
        return results;
    }

    /**
     * CQ3: 检测任务冲突（需要SWRL推理结果）
     * Detect task conflicts (requires SWRL inference results)
     */
    public List<String> getConflicts() {
        // SWRL规则推理后，冲突实例会被推导到对应的Conflict类
        String sparql =
            "PREFIX : <http://example.org/manufacturing#>\n" +
            "SELECT ?task ?conflictType\n" +
            "WHERE {\n" +
            "    {\n" +
            "        ?task a :CapabilityConflict .\n" +
            "        BIND(\"能力冲突：设备无法加工所需材料\" AS ?conflictType)\n" +
            "    }\n" +
            "    UNION\n" +
            "    {\n" +
            "        ?task a :TimeConflict .\n" +
            "        BIND(\"时间冲突：设备时间段重叠\" AS ?conflictType)\n" +
            "    }\n" +
            "    UNION\n" +
            "    {\n" +
            "        ?task a :StatusConflict .\n" +
            "        BIND(\"状态冲突：设备状态不可用\" AS ?conflictType)\n" +
            "    }\n" +
            "}";

        List<String> results = new ArrayList<>();
        Query query = QueryFactory.create(sparql);
        try (QueryExecution qexec = QueryExecution.create()
                .query(query)
                .model(model)
                .build()) {
            ResultSet rs = qexec.execSelect();
            while (rs.hasNext()) {
                QuerySolution soln = rs.nextSolution();
                String type = soln.getLiteral("conflictType").getString();
                String task = soln.getResource("task").getLocalName();
                results.add("[" + type + "] 任务: " + task);
            }
        }
        return results;
    }

    public static void main(String[] args) {
        // 演示用法
        OntologyManager manager = new OntologyManager();
        manager.loadOntology("src/manufacturing.owl");

        QueryService service = new QueryService(manager.getModel());

        // CQ1: 查询能加工铝合金的设备
        System.out.println("=== 能加工铝合金的设备 ===");
        service.getEquipmentForMaterial("Aluminum").forEach(System.out::println);

        // CQ2: 查询空闲设备
        System.out.println("\n=== 当前空闲设备 ===");
        service.getIdleEquipment().forEach(System.out::println);

        // CQ3: 查询冲突
        System.out.println("\n=== 任务冲突 ===");
        List<String> conflicts = service.getConflicts();
        if (conflicts.isEmpty()) {
            System.out.println("无冲突");
        } else {
            conflicts.forEach(System.out::println);
        }
    }
}
