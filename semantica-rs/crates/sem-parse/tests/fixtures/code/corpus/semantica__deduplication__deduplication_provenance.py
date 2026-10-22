"""
Provenance-enabled wrapper for deduplication.

Tracks: duplicates found, merge operations, deduplication strategy

Usage:
    from semantica.deduplication.deduplication_provenance import DeduplicatorWithProvenance
    
    dedup = DeduplicatorWithProvenance(provenance=True)
    unique_items = dedup.deduplicate(items)

Author: Semantica Contributors
License: MIT
"""

from typing import List, Any, Optional
from datetime import datetime
import uuid


class DeduplicatorWithProvenance:
    """Deduplicator with provenance tracking."""

    def __init__(
        self,
        provenance: bool = False,
        agent_id: Optional[str] = None,
        is_automated: bool = True,
        **config,
    ):
        from .deduplicator import Deduplicator

        self.provenance = provenance
        self._deduplicator = Deduplicator(**config)
        self._prov_manager = None
        self._agent_id = agent_id or self.__class__.__name__
        self._is_automated = is_automated

        if provenance:
            try:
                from semantica.provenance import ProvenanceManager
                self._prov_manager = ProvenanceManager()
            except ImportError:
                self.provenance = False

    def deduplicate(self, items: List[Any], source: str = None, **kwargs):
        """Deduplicate items with provenance tracking."""
        activity_started_at_time = datetime.utcnow().isoformat()
        unique_items = self._deduplicator.deduplicate(items, **kwargs)
        activity_ended_at_time = datetime.utcnow().isoformat()

        if self.provenance and self._prov_manager:
            duplicates_found = len(items) - len(unique_items)
            self._prov_manager.track_entity(
                entity_id=f"dedup_{uuid.uuid4().hex[:8]}",
                source=source or "deduplication",
                entity_type="deduplication_operation",
                agent_id=self._agent_id,
                agent_type="software_agent",
                is_automated=self._is_automated,
                activity_started_at_time=activity_started_at_time,
                activity_ended_at_time=activity_ended_at_time,
                metadata={
                    "input_count": len(items),
                    "output_count": len(unique_items),
                    "duplicates_removed": duplicates_found
                }
            )
        
        return unique_items
    
    def __getattr__(self, name):
        return getattr(self._deduplicator, name)


__all__ = ['DeduplicatorWithProvenance']
