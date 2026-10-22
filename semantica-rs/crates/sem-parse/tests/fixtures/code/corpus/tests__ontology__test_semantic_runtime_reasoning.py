from __future__ import annotations

from dataclasses import FrozenInstanceError
import hashlib

import pytest

from semantica.ontology import (
    ForwardChainResultDTO,
    ForwardInferenceDTO,
    ForwardRuleDTO,
    SemanticRuntime,
)
from semantica.ontology.runtime import (
    CAP_RULE_FORWARD_CHAIN,
    CapabilityUnavailableError,
    SemanticInputError,
    SemanticPackageAssetDTO,
    SemanticRuntimeError,
)


def _asset(content: str, *, format: str = "semantica-rule") -> SemanticPackageAssetDTO:
    payload = content.encode("utf-8")
    return SemanticPackageAssetDTO(
        asset_id="forward-chain-rules",
        role="rules",
        content=payload,
        sha256=hashlib.sha256(payload).hexdigest(),
        format=format,
    )


def test_forward_reasoning_is_one_runtime_capability_and_returns_frozen_dtos():
    runtime = SemanticRuntime()
    assert runtime.supports(CAP_RULE_FORWARD_CHAIN)
    assert CAP_RULE_FORWARD_CHAIN in runtime.profile.capabilities

    rules = _asset(
        """
        # chapter-package executable adaptation
        IF Equipment(?x) AND PowerAbove10(?x) THEN HighPowerEquipment(?x)
        IF HighPowerEquipment(?x) THEN RequiresCooling(?x)
        """
    )
    before_revision = runtime.revision
    result = runtime.reason(
        ["PowerAbove10(Lathe_003)", "Equipment(Lathe_003)"],
        rules,
    )

    assert isinstance(result, ForwardChainResultDTO)
    assert all(isinstance(item, ForwardInferenceDTO) for item in result.inferences)
    assert result.complete is True
    assert result.semantics == "semantica.textual-forward-chain.v1"
    assert result.conclusions == (
        "HighPowerEquipment(Lathe_003)",
        "RequiresCooling(Lathe_003)",
    )
    target = result.inferences[-1]
    assert target.rule_id == "rule_2"
    assert target.rule_text == (
        "IF HighPowerEquipment(?x) THEN RequiresCooling(?x)"
    )
    assert target.premises == ("HighPowerEquipment(Lathe_003)",)
    assert target.confidence == 1.0
    assert "RequiresCooling(Lathe_003)" in target.explanation
    assert "rule_2" in target.explanation
    assert runtime.revision == before_revision
    assert runtime.quad_count == 0

    with pytest.raises(FrozenInstanceError):
        target.confidence = 0.5


def test_forward_reasoning_is_stateless_and_deterministic_with_explicit_rules():
    runtime = SemanticRuntime()
    rules = [
        ForwardRuleDTO(
            rule_id="derive-b",
            text="IF A(?x) THEN B(?x)",
            confidence=0.75,
            priority=10,
        ),
        ForwardRuleDTO(
            rule_id="derive-c",
            text="IF B(?x) THEN C(?x)",
            confidence=0.5,
        ),
    ]

    first = runtime.reason(["A(one)"], rules)
    second = runtime.reason(["A(one)"], rules)

    assert first == second
    assert first.input_facts == ("A(one)",)
    assert first.all_facts == ("A(one)", "B(one)", "C(one)")
    assert first.inferences[0].rule_id == "derive-b"
    assert first.inferences[0].confidence == 0.75
    assert first.inferences[1].rule_id == "derive-c"
    assert first.inferences[1].confidence == 0.5


def test_rdf_profile_and_unsupported_rule_inputs_fail_closed():
    rdf_runtime = SemanticRuntime(profile="rdf")
    assert rdf_runtime.supports(CAP_RULE_FORWARD_CHAIN) is False
    with pytest.raises(CapabilityUnavailableError, match="rule.forward_chain"):
        rdf_runtime.reason(["A(one)"], ["IF A(?x) THEN B(?x)"])

    runtime = SemanticRuntime()
    with pytest.raises(SemanticInputError, match="semantica-rule"):
        runtime.reason(["A(one)"], _asset("A(?x) → B(?x)", format="swrl"))
    with pytest.raises(SemanticInputError, match="must use 'IF"):
        runtime.reason(["A(one)"], ["A(?x) -> B(?x)"])
    with pytest.raises(SemanticInputError, match="unbound variables"):
        runtime.reason(["A(one)"], ["IF A(?x) THEN B(?y)"])
    with pytest.raises(SemanticInputError, match="ground atoms"):
        runtime.reason(["A(?x)"], ["IF A(?x) THEN B(?x)"])
    with pytest.raises(SemanticInputError, match="ordered iterable"):
        runtime.reason({"A(one)"}, ["IF A(?x) THEN B(?x)"])


def test_iteration_limit_never_returns_an_incomplete_closure():
    runtime = SemanticRuntime()
    with pytest.raises(SemanticRuntimeError, match="did not converge"):
        runtime.reason(
            ["Seed(a)"],
            ["IF Seed(?x) THEN Seed(next(?x))"],
            max_iterations=2,
        )

