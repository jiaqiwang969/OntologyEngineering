"""Navigation coverage and service failures; fixtures do not assert model accuracy."""
import json
import threading
from unittest.mock import patch

from ontology_engineering.misumi import catalog, cli
from ontology_engineering.misumi.judger import Judge, JevFailure
from ontology_engineering.misumi.knowledge import (
    CHAPTER_BATCH_LIMIT, CHAPTER_WORKERS, Knowledge, METHOD_BOUNDARY, PATTERN_ROUTE_STAGE, PAYLOAD_BYTES,
)
from test_misumi_provider import fixture_answers


CONTEXT = {"schema": "ontology-engineering.supplier-knowledge-context/v1",
           "task_id": "navigation-fixture", "goal": "Inspect evidence for the current design question."}


class NavigationJudge:
    model = "jev-1.13.0"

    def __init__(self, target="tail", fail_chapter_call=None, fail_chapter_id=None):
        self.identity, self.payloads = {}, []
        self.target, self.fail_chapter_call, self.chapter_calls = target, fail_chapter_call, 0
        self.fail_chapter_id, self.lock = fail_chapter_id, threading.Lock()

    def ask(self, payload):
        observed = payload["state"].get("observed_catalog_chapters")
        with self.lock:
            self.payloads.append(payload)
            if observed is not None:
                self.chapter_calls += 1
            chapter_call = self.chapter_calls
        if observed is not None:
            if (chapter_call == self.fail_chapter_call or
                    self.fail_chapter_id in {value["id"] for value in observed.values()}):
                raise JevFailure("fixture_service_failure")
            choice = next((key for key, value in observed.items() if value["id"] == self.target),
                          next(iter(observed), "none"))
            return fixture_answers(payload, {"primary": choice, "companion": "none"})
        return fixture_answers(payload)


def chapters(count=186):
    # Tail has no query-token overlap. The fixture tests its availability, not
    # whether a real model will deem it relevant to this or any other query.
    return [{"id": "tail" if i == count - 1 else "chapter-" + str(i),
             "catalog": "book-" + str(i % 6), "title": "章节标题与适用条件 " * 9 + str(i),
             "aliases": ["Observed navigation synonym " + str(i)]} for i in range(count)]


def knowledge(judge):
    return Knowledge({"model": judge.model}, CONTEXT, judge)


def test_all_186_headings_reach_bounded_calls_and_tail_survives_merge():
    judge = NavigationJudge()
    engine = knowledge(judge)
    source = chapters()
    route = engine.route("A conceptual query without a literal heading match", source)
    nav = route["chapter_navigation"]
    assert nav["available"] == nav["considered"] == 186
    assert set(nav["submitted_ids"]) == {chapter["id"] for chapter in source}
    assert nav["omitted_ids"] == nav["failed_ids"] == []
    assert nav["status"] == "completed"
    assert nav["merge"]["status"] == "completed"
    assert [chapter["id"] for chapter in route["chapters"]] == ["tail"]
    assert 3 <= len(judge.payloads) <= CHAPTER_BATCH_LIMIT + 2
    for request in judge.payloads:
        assert len(catalog.dumps(request).encode()) <= PAYLOAD_BYTES
        assert request["state"]["engineering_method"] == engine.method
        for question in request["questions"].values():
            assert question["type"] == "choice"
            assert len(question["criteria"]) <= 255
            assert "Apply engineering_method" in question["instructions"]
    assert len(judge.payloads[-1]["state"]["observed_catalog_chapters"]) <= 2 * len(nav["batches"])


def test_one_batch_retains_original_answer_and_call_contract():
    judge = NavigationJudge()
    route = knowledge(judge).route("Inspect", chapters(2))
    nav = route["chapter_navigation"]
    assert len(judge.payloads) == 2
    assert route["chapter_answers"] == nav["batches"][0]["answers"]
    assert route["chapter_call"] == nav["batches"][0]["call"]
    assert "merge" not in nav


def test_local_mode_never_calls_model_or_claims_navigation_review():
    with patch.object(NavigationJudge, "ask", side_effect=AssertionError("network forbidden")):
        route = Knowledge({}, CONTEXT).route("Inspect", chapters())
    assert route["mode"] == "local"
    assert route["topic_judgment"] == "not_run"
    assert route["chapters"] == []


def test_route_stage_reaches_only_pattern_questions_without_fabricating_evidence():
    judge = NavigationJudge()
    engine = knowledge(judge)
    original = json.loads(json.dumps(engine.policy["pattern_questions"]))
    expected_state = engine._payload("Inspect", original)["state"]
    route = engine.route("Inspect", chapters(2))
    request = judge.payloads[0]
    assert request["state"] == expected_state
    assert request["state"]["source_text"] == ""
    assert request["state"]["claim"]["status"] == "question_context_only"
    assert request["state"]["claim"]["statement"] == "Inspect"
    for qid, question in request["questions"].items():
        if qid in original:
            assert question["instructions"] == PATTERN_ROUTE_STAGE + METHOD_BOUNDARY + original[qid]["instructions"]
            assert question["criteria"] == original[qid]["criteria"]
        else:
            assert PATTERN_ROUTE_STAGE not in question["instructions"]
    assert all(PATTERN_ROUTE_STAGE not in q["instructions"] for q in judge.payloads[1]["questions"].values())
    assert engine.policy["pattern_questions"] == original
    assert route["pattern_relevance"] == "candidate"
    assert route["semantic_review"] == route["ontology_promotion"] == "not_run"


def test_review_keeps_its_original_stage_and_model_unknown_is_not_overridden():
    class UnknownPatternJudge(NavigationJudge):
        def ask(self, payload):
            self.payloads.append(payload)
            return fixture_answers(payload, {qid: "not_established" for qid in payload["questions"]
                                             if qid.startswith("pattern_")})

    judge = UnknownPatternJudge()
    engine = knowledge(judge)
    route = engine.route("Object and goal have not been supplied", [])
    plan = engine.review("Object and goal have not been supplied", [], route)
    review_request = judge.payloads[-1]
    for qid, question in engine.policy["pattern_questions"].items():
        assert review_request["questions"][qid] == {
            **question, "instructions": METHOD_BOUNDARY + question["instructions"]}
    assert all(PATTERN_ROUTE_STAGE not in q["instructions"] for q in review_request["questions"].values())
    assert all(item["choice"] == "not_established" for item in route["pattern_refs"] + plan["pattern_refs"])
    assert plan["execution"] == "not_run"


def test_failed_batch_preserves_other_candidates_and_failed_ids():
    judge = NavigationJudge(fail_chapter_id="chapter-2")
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 2):
        route = knowledge(judge).route("Inspect", chapters(6))
    nav = route["chapter_navigation"]
    assert nav["status"] == "partial"
    assert nav["considered"] == 4
    assert nav["failed_ids"] == nav["batches"][1]["chapter_ids"]
    assert nav["batches"][1]["error"] == "fixture_service_failure"
    assert route["errors"] == [{"stage": "chapter_navigation", "batch": 1, "code": "fixture_service_failure"}]
    assert route["chapters"]


def test_failed_merge_preserves_bounded_survivors_without_final_choice():
    judge = NavigationJudge(fail_chapter_call=4)
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 2):
        # Three batch survivors exceed the patched option guard, so the merge
        # fails locally without making a fourth call.
        route = knowledge(judge).route("Inspect", chapters(6))
    nav = route["chapter_navigation"]
    assert nav["status"] == "partial"
    assert nav["merge"] == {"status": "failed", "error": "chapter_merge_exceeds_request_budget"}
    assert len(route["chapters"]) == 3
    assert "chapter_answers" not in route
    assert judge.chapter_calls == 3


def test_service_merge_failure_is_distinct_from_no_match():
    judge = NavigationJudge(fail_chapter_call=4)
    engine = knowledge(judge)
    source = chapters(9)
    # Force exactly three successful batches without changing the byte guard.
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 3):
        route = engine.route("Inspect", source)
    assert len(route["chapter_navigation"]["batches"]) == 3
    assert route["chapter_navigation"]["merge"]["error"] == "fixture_service_failure"
    assert route["chapters"]
    assert "chapter_answers" not in route


def test_oversized_heading_and_request_cap_are_explicit_omissions():
    source = [{"id": "oversized", "catalog": "book-0", "title": "超" * 30000}, *chapters(6)]
    judge = NavigationJudge()
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 2), \
         patch("ontology_engineering.misumi.knowledge.CHAPTER_BATCH_LIMIT", 2):
        route = knowledge(judge).route("Inspect", source)
    nav = route["chapter_navigation"]
    assert nav["status"] == "partial"
    assert nav["considered"] == 4
    assert len(nav["omitted_ids"]) == 3
    assert {item["reason"] for item in nav["omissions"]} == {"chapter_exceeds_request_budget", "chapter_batch_limit"}
    assert "oversized" not in nav["submitted_ids"]
    assert all(len(catalog.dumps(p).encode()) <= PAYLOAD_BYTES for p in judge.payloads)


def test_empty_navigation_does_not_spend_a_chapter_request():
    judge = NavigationJudge()
    route = knowledge(judge).route("Inspect", [])
    assert len(judge.payloads) == 1
    assert route["chapter_navigation"]["available"] == 0
    assert route["chapter_navigation"]["considered"] == 0
    assert route["chapters"] == []


class ScheduledNavigationJudge:
    """Events force overlap and opposite completion orders without timing sleeps."""
    model = "jev-1.13.0"

    def __init__(self, source, reverse_pairs=False, failed_batches=()):
        self.identity = {}
        self.batch_ids = [tuple(c["id"] for c in source[i:i + 4]) for i in range(0, len(source), 4)]
        self.reverse_pairs, self.failed_batches = reverse_pairs, set(failed_batches)
        self.entered = [threading.Event() for _ in self.batch_ids]
        self.finished = [threading.Event() for _ in self.batch_ids]
        self.lock = threading.Lock()
        self.active = self.maximum_active = self.topic_calls = self.merge_calls = 0
        self.finished_order = []

    def ask(self, payload):
        observed = payload["state"].get("observed_catalog_chapters")
        if observed is None:
            self.topic_calls += 1
            assert self.active == 0
            return fixture_answers(payload)
        ids = tuple(chapter["id"] for chapter in observed.values())
        if ids not in self.batch_ids:
            self.merge_calls += 1
            assert self.active == 0
            assert all(event.is_set() for event in self.finished)
            return fixture_answers(payload)
        batch = self.batch_ids.index(ids)
        with self.lock:
            assert self.topic_calls == 1
            self.active += 1
            self.maximum_active = max(self.maximum_active, self.active)
            assert self.active <= CHAPTER_WORKERS
        self.entered[batch].set()
        try:
            # A serial implementation cannot satisfy this handshake. Every pair
            # starts together, then a chosen member must finish before the other.
            peer = batch ^ 1
            assert self.entered[peer].wait(5), "independent batches did not overlap"
            waits_for_peer = batch % 2 == (0 if self.reverse_pairs else 1)
            if waits_for_peer:
                assert self.finished[peer].wait(5), "paired batch did not finish"
            if batch in self.failed_batches:
                raise JevFailure("fixture_service_failure_" + str(batch))
            return fixture_answers(payload)
        finally:
            with self.lock:
                self.active -= 1
                self.finished_order.append(batch)
            self.finished[batch].set()


def test_batches_overlap_at_small_limit_and_output_order_is_stable():
    assert CHAPTER_WORKERS == 2
    source, routes = chapters(16), []
    for reverse in (False, True):
        judge = ScheduledNavigationJudge(source, reverse_pairs=reverse)
        with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 4):
            routes.append(knowledge(judge).route("Inspect", source))
        assert judge.maximum_active == CHAPTER_WORKERS
        assert judge.topic_calls == judge.merge_calls == 1
        first, second = (1, 0) if reverse else (0, 1)
        assert judge.finished_order.index(first) < judge.finished_order.index(second)
    assert routes[0] == routes[1]
    nav = routes[0]["chapter_navigation"]
    assert nav["submitted_ids"] == nav["considered_ids"] == [c["id"] for c in source]
    assert nav["concurrency_limit"] == CHAPTER_WORKERS
    assert [b["batch"] for b in nav["batches"]] == [0, 1, 2, 3]
    assert nav["candidate_ids"] == [source[i]["id"] for i in (0, 4, 8, 12)]
    assert nav["failed_ids"] == nav["omitted_ids"] == []


def test_parallel_mixed_failures_preserve_successes_and_all_coverage():
    source = chapters(16)
    judge = ScheduledNavigationJudge(source, reverse_pairs=True, failed_batches=(0, 2))
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 4):
        route = knowledge(judge).route("Inspect", source)
    nav = route["chapter_navigation"]
    assert judge.maximum_active == CHAPTER_WORKERS
    assert judge.merge_calls == 1
    assert nav["status"] == "partial"
    assert nav["submitted_ids"] == [c["id"] for c in source]
    assert nav["considered_ids"] == [c["id"] for c in source[4:8] + source[12:16]]
    assert nav["failed_ids"] == [c["id"] for c in source[:4] + source[8:12]]
    assert nav["candidate_ids"] == [source[4]["id"], source[12]["id"]]
    assert nav["merge"]["chapter_ids"] == nav["candidate_ids"]
    assert [c["id"] for c in route["chapters"]] == [source[4]["id"]]
    assert route["errors"] == [{"stage": "chapter_navigation", "batch": i,
        "code": "fixture_service_failure_" + str(i)} for i in (0, 2)]


def test_real_judge_preserves_parallel_accounting_and_atomic_cache(tmp_path):
    source = chapters(16)
    scheduled = ScheduledNavigationJudge(source, reverse_pairs=True)
    judge = Judge(tmp_path, {"model": scheduled.model}, "navigation-fixture")

    def transport(payload):
        answers, _ = scheduled.ask(payload)
        return {"model": scheduled.model, "answers": answers,
                "usage": {"input_tokens": 7, "output_tokens": 3}}

    # Inject a local transport before lazy credential resolution: no key access
    # or HTTP is possible, but real Judge locks, validation and writes are used.
    judge.transport = transport
    engine = knowledge(judge)
    with patch("ontology_engineering.misumi.knowledge.CHAPTER_OPTION_LIMIT", 4):
        first = engine.route("Inspect", source)
        replay = engine.route("Inspect", source)
    assert scheduled.maximum_active == CHAPTER_WORKERS
    assert scheduled.topic_calls == scheduled.merge_calls == 1
    assert first["chapters"] == replay["chapters"]
    assert judge.stats == {"network_requests": 6, "cache_hits": 6, "input_tokens": 42,
                          "output_tokens": 18, "usage_unreported_attempts": 0}
    assert len(judge.calls) == 12
    cache = list((tmp_path / "cache").glob("*.json"))
    assert len(cache) == 6
    for path in cache:
        record = json.loads(path.read_text())
        assert record["key"] == path.stem
        assert record["identity"] == judge.identity
        assert record["validation_errors"] == {}
    assert not list((tmp_path / "cache").glob(".write-*"))


def test_cli_preserves_navigation_error_as_partial(tmp_path, monkeypatch, capsys):
    # Exercise the real report path without original private sources or network.
    provider = {"id": "fixture", "chapters": [], "retrieve": lambda *args: [],
                "source_result": lambda page: page,
                "meta": {"fingerprint": "fixture", "built_at": "2026-01-01", "counts": {"pages": 0}}}
    monkeypatch.setattr(cli, "_providers", lambda *args: ([provider], []))
    error = {"stage": "chapter_navigation", "code": "fixture_service_failure", "batch": 1}
    monkeypatch.setattr(Knowledge, "route", lambda *args: {"scope": "catalog", "chapters": [],
        "needed_topics": [], "uncertain_topics": [], "errors": [error]})
    monkeypatch.setattr(cli.paths, "SKILL_ROOT", tmp_path)
    exit_code = cli.main(["Inspect", "--knowledge", "--local", "--json", "--state-root", str(tmp_path / "var/state/misumi")])
    report = json.loads(capsys.readouterr().out)
    assert exit_code == 2
    assert report["status"] == "partial"
    assert error in report["errors"]
