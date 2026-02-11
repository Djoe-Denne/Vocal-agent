"""
Shared application: pipeline factory and stage registry.

Provides the global registry of stage builders and functions to
construct Pipeline instances from TOML configuration dicts.
"""

from __future__ import annotations

from copy import deepcopy
from typing import Callable

from shared.domain.pipeline import Stage
from shared.domain.pipeline_runner import Pipeline


# ---------------------------------------------------------------------------
# Global registry: stage_type_name -> builder function
# ---------------------------------------------------------------------------

_STAGE_BUILDERS: dict[str, Callable[[dict], Stage]] = {}


def register_stage(type_name: str):
    """
    Decorator for registering stage builder functions.

    Usage::

        @register_stage("huggingface_asr")
        def _build_hf_asr(cfg: dict) -> Stage:
            from ptt.infra_huggingface.stage import HuggingFaceASRStage
            return HuggingFaceASRStage(**cfg)
    """

    def wrapper(fn: Callable[[dict], Stage]):
        _STAGE_BUILDERS[type_name] = fn
        return fn

    return wrapper


# ---------------------------------------------------------------------------
# Pipeline construction
# ---------------------------------------------------------------------------


def build_pipeline(
    name: str,
    pipeline_def: dict,
    stages_config: dict,
) -> Pipeline:
    """
    Construct a :class:`Pipeline` from parsed TOML config dicts.

    Parameters
    ----------
    name:
        Human-readable pipeline name (used for lookups).
    pipeline_def:
        Dict with at least a ``"stages"`` key listing stage names.
    stages_config:
        The full ``[stages]`` section from config, keyed by stage name.
    """
    stages: list[Stage] = []
    for stage_name in pipeline_def["stages"]:
        stage_cfg = deepcopy(stages_config.get(stage_name, {}))
        type_name = stage_cfg.pop("type", stage_name)
        builder = _STAGE_BUILDERS.get(type_name)
        if not builder:
            raise ValueError(
                f"Unknown stage type: {type_name!r}. "
                f"Registered types: {sorted(_STAGE_BUILDERS)}"
            )
        stages.append(builder(stage_cfg))
    return Pipeline(name=name, stages=stages)


def load_pipelines(config_raw: dict) -> dict[str, Pipeline]:
    """
    Load all named pipelines from a raw TOML config dict.

    Expects optional top-level keys ``"pipelines"`` and ``"stages"``.
    Returns an empty dict if no ``[pipelines]`` section is present.
    """
    pipelines_section = config_raw.get("pipelines", {})
    stages_section = config_raw.get("stages", {})

    if not pipelines_section:
        return {}

    pipelines: dict[str, Pipeline] = {}
    for name, pdef in pipelines_section.items():
        pipelines[name] = build_pipeline(name, pdef, stages_section)
    return pipelines


__all__ = [
    "register_stage",
    "build_pipeline",
    "load_pipelines",
]
