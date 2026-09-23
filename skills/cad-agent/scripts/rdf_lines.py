"""Write deterministic N-Triples evidence lines without semantic execution.

This module only serializes already validated project identifiers and strings.
It cannot parse RDF, run a query, apply a shape, infer facts, or issue a verdict.
Semantica remains the sole semantic executor of the resulting evidence.
"""

from __future__ import annotations

import json


class Iri(str):
    def __new__(cls, value: str) -> "Iri":
        if not isinstance(value, str) or ":" not in value or any(
            character.isspace() or character in '<>"{}|^`\\' for character in value
        ):
            raise ValueError(f"invalid evidence IRI: {value!r}")
        return str.__new__(cls, value)

    def n3(self) -> str:
        return f"<{self}>"


class Text(str):
    def __new__(cls, value: str) -> "Text":
        if not isinstance(value, str):
            raise TypeError("evidence literals must be strings")
        return str.__new__(cls, value)

    def n3(self) -> str:
        return json.dumps(str(self), ensure_ascii=False)


class Namespace:
    def __init__(self, base: str) -> None:
        self.base = Iri(base)

    def __str__(self) -> str:
        return str(self.base)

    def __getitem__(self, name: str) -> Iri:
        if not name or any(character.isspace() for character in name):
            raise ValueError(f"invalid evidence term: {name!r}")
        return Iri(str(self.base) + name)

    def __getattr__(self, name: str) -> Iri:
        return self[name]


class RdfTerms:
    type = Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")


class TripleLines:
    def __init__(self) -> None:
        self._rows: set[tuple[Iri, Iri, Iri | Text]] = set()

    def add(self, row: tuple[Iri, Iri, Iri | Text]) -> None:
        subject, predicate, object_ = row
        if not isinstance(subject, Iri) or not isinstance(predicate, Iri):
            raise TypeError("evidence subject and predicate must be IRIs")
        if not isinstance(object_, (Iri, Text)):
            raise TypeError("evidence object must be an IRI or text literal")
        self._rows.add(row)

    def __iter__(self):
        return iter(self._rows)
