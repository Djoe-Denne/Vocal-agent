"""
Shared domain: pipeline runner.

A Pipeline is a named, ordered sequence of Stage instances.
"""

from __future__ import annotations

import time

from shared.domain.pipeline import PipelineContext, Stage


class Pipeline:
    """Named, ordered sequence of stages."""

    def __init__(self, name: str, stages: list[Stage]) -> None:
        self.name = name
        self.stages = stages

    def run(self, ctx: PipelineContext) -> PipelineContext:
        """Execute every stage in order, recording timing in *ctx.stage_results*."""
        for stage in self.stages:
            t0 = time.time()
            ctx = stage.process(ctx)
            ctx.stage_results.append(
                {
                    "stage": stage.name,
                    "elapsed": time.time() - t0,
                }
            )
        return ctx

    def load_all(self) -> None:
        """Load heavy resources for every stage."""
        for s in self.stages:
            s.load()

    def unload_all(self) -> None:
        """Free resources for every stage."""
        for s in self.stages:
            s.unload()


__all__ = ["Pipeline"]
