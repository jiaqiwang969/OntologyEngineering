# Engineering Ontology Source Map

Use this map to decide where to search before answering ontology questions.

## Primary Book

Bundled root: `references/ontology-engineering-book`

| Area | Local source | Use for |
|---|---|---|
| Overview | `README.md` | Whole-book scope, chapter list, repository layout |
| Ch01 introduction | `ch01-introduction/README.md`, `examples/*.txt`, `handbook/chapters/ch01.tex` | From philosophical ontology to engineering ontology, AI-era positioning, roadmap |
| Ch02 foundations | `ch02-ontology-foundations/README.md`, `examples/*.txt`, `handbook/chapters/ch02.tex` | Core concepts, classes/relations/instances, first-order logic, description logic, reasoning basics |
| Ch03 methodology | `ch03-ontology-methodology/README.md`, `examples/*.txt`, `handbook/chapters/ch03.tex` | Ontology 101, METHONTOLOGY, competency questions, OntoClean evaluation |
| Ch04 languages | `ch04-ontology-languages/README.md`, `examples/*`, `handbook/chapters/ch04.tex` | RDF, RDFS, OWL, Manchester-style restrictions, SPARQL, Protege-style modeling · runnable: `demos/ch04_shacl_open_vs_closed.py` |
| Ch05 reasoning | `ch05-reasoning/README.md`, `examples/*`, `handbook/chapters/ch05.tex` | Description-logic reasoning, SWRL, temporal/probabilistic reasoning · runnable: `demos/ch05_forward_chaining.py` |
| Ch06 applications | `ch06-applications/README.md`, `examples/*.txt`, `handbook/chapters/ch06.tex` | Autonomous driving, BIM, aerospace FMEA, manufacturing scheduling |
| Ch07 knowledge graph | `ch07-knowledge-graph/README.md`, `examples/*`, `handbook/chapters/ch07.tex` | KG construction, entity resolution, storage/query choices, SHACL quality validation · runnable: `demos/ch04_shacl_open_vs_closed.py`（书中 kg-quality-shacl.ttl 实际执行） |
| Ch08 ontology + LLM | `ch08-ontology-llm/README.md`, `examples/*`, `handbook/chapters/ch08.tex` | GraphRAG, Text2SPARQL, hallucination control, ontology-guided agents |
| Ch09 capstone | `ch09-capstone-manufacturing/README.md`, `src/*`, `handbook/chapters/ch09.tex` | Manufacturing ontology, Java/Jena, Python/owlready2, SPARQL query service, reasoner example |
| Glossary | `handbook/chapters/appB-glossary.tex` | Quick term alignment |

Prefer `README.md`, chapter examples, and `handbook/chapters/*.tex`. Avoid
generated fragments, logs, PDFs, images, `.venv`, and cache files unless the user
explicitly asks about build artifacts.

## Optional CauchyX PDE Agent Example

This distributable skill does not bundle `cauchyx-ai`. If the user has a local
CauchyX repository, set `ONTOLOGY_ENGINEERING_ROOT` to the parent directory that
contains both `ontology-engineering-book` and `cauchyx-ai`, or pass `--root`
when running the search script.

Use CauchyX as an applied example of ontology-controlled engineering AI,
especially when discussing tool routing, hallucination control, unit checking,
formal constraints, material-property lookup, solver compatibility, and
provenance.

| Area | Local source | Use for |
|---|---|---|
| PDE Agent overview | `pde-agent/README.md` | Architecture, ontology-controlled solver routing, test claims |
| Core ontology | `pde-agent/ontology/pde_core.ttl` | TBox/ABox structure, equation types, boundary conditions, solver compatibility |
| Materials | `pde-agent/ontology/materials.ttl`, `src/material_library.py` | Material properties and diffusivity derivation |
| SHACL | `pde-agent/shapes/pde_constraints.shacl` | Parameter range and data validation constraints |
| SPARQL checks | `pde-agent/sparql/*.rq` | Hallucination checks, solver routing, unit checks, provenance queries |
| Router | `pde-agent/src/ontology_router.py` | Natural-language/spec to ontology validation and routing integration |
| Unit normalization | `pde-agent/src/unit_normalizer.py` | QUDT-style unit conversion and bounds checking |
| Provenance | `pde-agent/src/prov_generator.py`, `sparql/prov_chain.rq` | PROV-O audit trail for regulated engineering workflows |
| Tests | `pde-agent/test_ontology.py` | Concrete validation scenarios and expected behavior |
| Existing skill | `$cauchyx-pde` when installed | Run actual PDE solves or ontology routing tests |

## Common Query Pairs

Use bilingual searches when possible:

- `能力问题 competency question`
- `本体构建 方法论 Ontology 101 METHONTOLOGY`
- `OntoClean 刚性 统一性 依赖性 身份`
- `描述逻辑 description logic DL 推理`
- `RDF RDFS OWL SPARQL Turtle`
- `SHACL 质量 校验 constraint validation`
- `幻觉控制 hallucination control ontology guided agent`
- `GraphRAG Text2SPARQL`
- `solver routing unit validation provenance PDE`
- `CAD Agent Fusion evolution SHACL regression authorization`

## Optional CAD Agent Example

When the local checkout exists, set `CAD_AGENT_ROOT` and use `--scope cad`.
`cad-agent` is the concrete CAD case of the book's methodology: competency
questions delimit scope, RDF(S)/OWL describe stable meaning, SHACL enforces
closed-world delivery and safety contracts, named SPARQL queries support routing
and audit, and PROV records the control loop. Fusion remains a separate
privileged executor.

| Area | Local source | Use for |
|---|---|---|
| CAD Agent overview | `README.md`, `docs/architecture.md` | PDE-pattern mapping, three control surfaces, authority boundaries |
| Agent task ontology | `ontology/agent.ttl` | modes, adapters, execution states, evolution candidates |
| SHACL gates | `shapes/cad-agent.shacl.ttl` | task readiness, unsafe execution, reproduced/accepted evolution contracts |
| Named CQs | `sparql/cq11-agent-routing.rq` through `cq14-unsafe-evolution-acceptance.rq` | routing, unsafe execution, evolution readiness, unsafe acceptance |
| Router and audit | `src/cad_agent/ontology_router.py`, `src/cad_agent/provenance.py` | propose -> validate -> route -> audit implementation |
| Controlled evolution | `src/cad_agent/evolution.py`, `docs/video-driven-self-evolution.md` | hash-bound lesson deltas, Fusion proof, four regressions, review and authorization |
| Repository skill | `.agents/skills/cad-agent/SKILL.md` | Codex workflow for planning, Fusion handoff, and bounded evolution application |

Do not treat the CAD case as book evidence. Use it as a clearly identified
applied example grounded by Ch03 methodology, Ch07 validation, and Ch08
ontology-guided agents.

## Boundary

This skill grounds ontology answers. It does not replace the PDE solver skill or
the repository-local CAD Agent workflow.
When the user asks to actually solve, simulate, validate, or run a PDE, switch
to `$cauchyx-pde` after identifying the ontology pattern.
