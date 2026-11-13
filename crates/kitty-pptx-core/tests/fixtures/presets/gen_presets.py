#!/usr/bin/env python3
"""Independent Python oracle for the ECMA-376 DrawingML preset formula language.

This generator is a from-scratch port of the *reference* implementation (it does
not use, and must not be derived from, the Rust port it is meant to check):

  * ``/tmp/oooxml/packages/core/src/shape/preset-geometry/evaluator.ts``
      - ``compileFormula``            (evaluator.ts:55)  -- whitespace tokenisation
      - ``createEvaluator``           (evaluator.ts:60)  -- builtins, adj resolution,
                                                            gd loop, resolve/evaluate
      - ``applyOp``                   (evaluator.ts:125) -- the operator table
      - ``CD`` / ``DEG60K_TO_RAD``    (evaluator.ts:15)  -- 60 000-ths of a degree
  * ``/tmp/oooxml/packages/core/src/shape/preset-geometry/geometry.ts``
      - ``effAdj``                    (geometry.ts:164)  -- literal ``val`` default
                                                            substitution used by
                                                            ``effectivePreset``

Preset data: ``crates/kitty-pptx-core/src/compiler/shapes/presets.json`` (verbatim
copy of the reference's ``presets.json``, 186 definitions).

Output: ``expected.json`` next to this script::

    {"version":1,
     "cases":[{"geometry":str, "w":num, "h":num, "adj":[num|null]*8,
               "effAdj":{name:num}, "guides":{name:num}}, ...]}

Case matrix (per preset, deterministic order: sorted preset name, then box, then
adjust variant)::

    boxes    = [(100,100), (300,150), (150,300)]
    variants = [all-null]            # preset defaults
             + [all 25000]
             + [all 75000]
             + [index i = 50000, rest null for i in 0..7]

    186 presets * 3 boxes * 11 variants = 6138 cases

``adj`` is the raw 8-slot handle vector handed to the evaluator (index i is the
i-th entry of the definition's ``adj``/``avLst`` list, exactly like the
reference's ``adj[i]``; trailing slots beyond the definition are null).

``effAdj`` holds the adjust values *in effect* after the reference's default
substitution ``createEvaluator`` performs (a non-null supplied value wins,
otherwise the preset's declared default formula is evaluated in order).  The
reference also aliases the first handle both ways (``adj`` <-> ``adj1``,
evaluator.ts:91-97), so that alias pair is emitted too: those are literally the
``env`` entries the guide formulas read.

``guides`` holds the final value of every ``gdLst`` entry, in the reference's
declaration order semantics (later entries may read earlier ones; a duplicated
name overwrites, as ``env[name] = ...`` does -- e.g. ``circulararrow`` defines
``dxI``/``dyI`` twice, ``funnel`` shadows the builtin ``cd``).

Arithmetic is IEEE-754 ``f64`` with *JavaScript* semantics, because the oracle
must reproduce the reference bit-for-bit rather than raise where the reference
propagates ``NaN``/``Infinity`` (division by zero, ``Math.min(NaN, x)``, ...).
Values are reported rounded to 6 decimals (half away from zero) so the JSON is
stable; the rounder also folds integral values to JSON integers.

Non-finite results are real reference behaviour -- a degenerate handle makes a
preset divide 0 by 0 (``nosmoking`` with ``adj = 50000`` on a square has an inner
radius of exactly 0; ``circulararrow`` with every handle at 75000 has a ring
width of 0) -- and *all* guides downstream of that division inherit the NaN. JSON
cannot spell ``NaN``, so those guide values are emitted as ``null``; the generator
prints how many and which. ``None`` therefore means "the reference yields a
non-finite float here", not "not evaluated".

Out of scope on purpose: the oracle evaluates the preset definition *as named*.
The reference's ``effectivePreset`` (geometry.ts:172-200) additionally swaps the
three wedge callouts for a base geometry when the tail tip lands inside the body
(``wedgerectcallout`` -> inline rect, ``wedgeellipsecallout`` -> ``ellipse``,
``wedgeroundrectcallout`` -> ``roundrect`` with ``adj3``). That is a rendering
policy applied by ``resolvePresetGeometryPaths``/``getPresetGeometryBounds``, not
by ``buildPresetGeometryPath``/``getConnectorAnchors``, so the raw definition is
what a preset evaluator must agree on; the 18 affected cases are the uniform
``25000`` and the ``adj2 = 50000`` variants of those three presets.

Usage::

    python3 tests/fixtures/presets/gen_presets.py            # regenerate
    python3 tests/fixtures/presets/gen_presets.py --check    # verify up to date

The script has no third-party dependencies (stdlib only).
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from decimal import ROUND_HALF_UP, Decimal
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

# ── Domain constants (evaluator.ts:15-16) ────────────────────────────────────
#
# Angles in DrawingML are 60 000-ths of a degree; a full turn is 21 600 000
# units, i.e. 2*pi in radians (ECMA-376 §20.1.9.2, shared by cd4/3cd4/... and by
# <arcTo>'s stAng/swAng).
CD = 21_600_000
DEG60K_TO_RAD = (math.pi * 2) / CD

# ── Reference data / output locations ───────────────────────────────────────
_SCRIPT = Path(__file__).resolve()
_REPO_ROOT = _SCRIPT.parents[3]
DEFAULT_PRESETS = (
    _REPO_ROOT / "crates/kitty-pptx-core/src/compiler/shapes/presets.json"
)
DEFAULT_OUT = _SCRIPT.parent / "expected.json"

BOXES: Tuple[Tuple[float, float], ...] = ((100, 100), (300, 150), (150, 300))
HANDLE_SLOTS = 8
BASE_VARIANTS: Tuple[Tuple[Optional[float], ...], ...] = (
    (None,) * HANDLE_SLOTS,
    (25000,) * HANDLE_SLOTS,
    (75000,) * HANDLE_SLOTS,
)


# ── JavaScript numeric semantics ────────────────────────────────────────────
_JS_NUMERIC = re.compile(r"[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?\Z")


def js_number(text: str) -> Optional[float]:
    """Port of ``Number(text)`` for the token shapes that occur in the data.

    Returns ``None`` where ``Number()`` would yield ``NaN`` (or where the token
    is not a plain decimal literal, which the data never contains).
    """
    if not _JS_NUMERIC.match(text.strip()):
        return None
    try:
        return float(text)
    except ValueError:  # pragma: no cover - regex already guarantees a decimal
        return None


def js_div(a: float, b: float) -> float:
    """``a / b`` with JS semantics: x/0 -> +-Infinity, 0/0 -> NaN, never raises."""
    if b == 0.0:
        if a != a or a == 0.0:  # NaN / 0 -> NaN, 0 / 0 -> NaN
            return math.nan
        return math.copysign(math.inf, a) * math.copysign(1.0, b)
    return a / b


def js_min(a: float, b: float) -> float:
    """``Math.min``: any NaN operand makes the result NaN (Python's min does not)."""
    if a != a or b != b:
        return math.nan
    return a if a < b else b


def js_max(a: float, b: float) -> float:
    """``Math.max``: any NaN operand makes the result NaN."""
    if a != a or b != b:
        return math.nan
    return a if a > b else b


# ── Formula compilation / evaluation (evaluator.ts) ─────────────────────────
#
# A formula is *not* a stack program: presets.json stores each gdLst/avLst entry
# as "<op> <operand> <operand> ...", and the reference applies `op` to the
# resolved operand list (`applyOp(op, args)`). Operands are guide names,
# built-ins, adjust names or numeric literals; arity is fixed per operator.
CompiledFormula = Tuple[str, Tuple[str, ...]]

OPERATOR_ARITY: Dict[str, int] = {
    "val": 1,
    "*/": 3,
    "+-": 3,
    "+/": 3,
    "?:": 3,
    "abs": 1,
    "min": 2,
    "max": 2,
    "pin": 3,
    "sqrt": 1,
    "mod": 3,
    "sin": 2,
    "cos": 2,
    "tan": 2,
    "at2": 2,
    "cat2": 3,
    "sat2": 3,
}


class PresetEvalError(Exception):
    """Mirrors the reference's thrown ``preset-shape: ...`` errors."""


def compile_formula(expr: str) -> CompiledFormula:
    """``compileFormula`` (evaluator.ts:55): ``expr.trim().split(/\\s+/)``."""
    parts = expr.strip().split()
    if not parts:
        raise PresetEvalError(f"empty formula {expr!r}")
    return parts[0], tuple(parts[1:])


def builtin_env(w: float, h: float) -> Dict[str, float]:
    """Built-ins of ECMA-376 §20.1.9.2 as initialised by evaluator.ts:72-89."""
    ss = js_min(w, h)
    ls = js_max(w, h)
    env: Dict[str, float] = {
        "w": w,
        "h": h,
        "l": 0.0,
        "t": 0.0,
        "r": w,
        "b": h,
        "hc": w / 2,
        "vc": h / 2,
    }
    for n in (2, 3, 4, 5, 6, 8, 10, 12, 16, 32):
        env[f"wd{n}"] = w / n
        env[f"hd{n}"] = h / n
    for n in (2, 4, 6, 8, 16, 32):
        env[f"ssd{n}"] = ss / n
        env[f"lsd{n}"] = ls / n
    env["ss"] = ss
    env["ls"] = ls
    env.update(
        {
            "cd": float(CD),
            "cd2": CD / 2,
            "cd4": CD / 4,
            "cd8": CD / 8,
            "3cd4": (3 * CD) / 4,
            "3cd8": (3 * CD) / 8,
            "5cd8": (5 * CD) / 8,
            "7cd8": (7 * CD) / 8,
        }
    )
    return env


def apply_op(op: str, args: Sequence[float]) -> float:
    """``applyOp`` (evaluator.ts:125-146), bit-faithful to the JS arithmetic.

    Operands past the operator's arity are resolved but ignored (``applyOp`` only
    reads ``a[0..2]``; eight shipped formulas carry one such trailing operand).
    Missing operands become ``NaN`` exactly as reading past the end of a JS array
    does (``undefined * 1 -> NaN``); ``validate_operators`` verifies no shipped
    formula is short an operand.
    """

    def arg(i: int) -> float:
        return args[i] if i < len(args) else math.nan

    if op == "val":
        return arg(0)
    if op == "*/":
        return js_div(arg(0) * arg(1), arg(2))
    if op == "+-":
        return arg(0) + arg(1) - arg(2)
    if op == "+/":
        return js_div(arg(0) + arg(1), arg(2))
    if op == "?:":
        return arg(1) if arg(0) > 0 else arg(2)
    if op == "abs":
        return abs(arg(0))
    if op == "min":
        return js_min(arg(0), arg(1))
    if op == "max":
        return js_max(arg(0), arg(1))
    if op == "pin":
        v = arg(1)
        return arg(0) if v < arg(0) else (arg(2) if v > arg(2) else v)
    if op == "sqrt":
        return math.sqrt(js_max(0.0, arg(0)))
    if op == "mod":
        # NB: the reference evaluates `mod` as the 3-D hypotenuse, not as the
        # ECMA-376 remainder operator (evaluator.ts:137).
        return math.sqrt(arg(0) * arg(0) + arg(1) * arg(1) + arg(2) * arg(2))
    if op == "sin":
        return arg(0) * math.sin(arg(1) * DEG60K_TO_RAD)
    if op == "cos":
        return arg(0) * math.cos(arg(1) * DEG60K_TO_RAD)
    if op == "tan":
        return arg(0) * math.tan(arg(1) * DEG60K_TO_RAD)
    if op == "at2":
        # Returns 60 000-ths of a degree.
        return math.atan2(arg(1), arg(0)) / DEG60K_TO_RAD
    if op == "cat2":
        # a0 * cos(atan2(a2, a1)) -- point on an ellipse (evaluator.ts:144).
        return arg(0) * math.cos(math.atan2(arg(2), arg(1)))
    if op == "sat2":
        return arg(0) * math.sin(math.atan2(arg(2), arg(1)))
    raise PresetEvalError(f'unknown operator "{op}"')


class Evaluator:
    """``createEvaluator`` (evaluator.ts:60-150).

    ``env`` is grown exactly like the reference's ``Object.create(null)`` map:
    built-ins, then adjust handles (caller-supplied value, else the declared
    default formula, with adj/adj1 aliasing), then the guide list in declaration
    order.
    """

    def __init__(
        self,
        w: float,
        h: float,
        adj: Sequence[Optional[float]],
        adj_defaults: Sequence[Tuple[str, str]],
        gd_list: Sequence[Tuple[str, str]],
    ) -> None:
        self.env: Dict[str, float] = builtin_env(w, h)
        self.adj_names: List[str] = []

        # Adjust resolution (evaluator.ts:91-98): caller value wins, otherwise
        # the preset's declared default formula is evaluated in place.
        self.adj_defaults = [compile_formula(f) for _, f in adj_defaults]
        for i, (name, _f) in enumerate(adj_defaults):
            supplied = adj[i] if i < len(adj) else None
            if isinstance(supplied, (int, float)) and not isinstance(supplied, bool):
                self.env[name] = float(supplied)
            else:
                self.env[name] = self.formula(self.adj_defaults[i])
            self.adj_names.append(name)
            # Many shapes name the first adjust "adj" but reference it as "adj1".
            if name == "adj":
                self.env["adj1"] = self.env["adj"]
            if name == "adj1":
                self.env["adj"] = self.env["adj1"]

        # Guide list, evaluated in declaration order (evaluator.ts:100-102).
        self.gd = [compile_formula(f) for _, f in gd_list]
        self.gd_names = [n for n, _ in gd_list]
        for i, (name, _f) in enumerate(gd_list):
            self.env[name] = self.formula(self.gd[i])

    def resolve(self, token: str) -> float:
        """``resolve`` (evaluator.ts:113-118): name first, then numeric literal."""
        if token in self.env:
            return self.env[token]
        n = js_number(token)
        if n is not None and math.isfinite(n):
            return n
        raise PresetEvalError(f'preset-shape: cannot resolve "{token}"')

    def formula(self, f: CompiledFormula) -> float:
        """``evaluateFormula`` (evaluator.ts:120-123)."""
        op, arg_tokens = f
        arity = OPERATOR_ARITY.get(op)
        if arity is None:
            spelled = " ".join((op,) + arg_tokens)
            raise PresetEvalError(f'preset-shape: unknown operator "{op}" in "{spelled}"')
        return apply_op(op, [self.resolve(t) for t in arg_tokens])


def validate_operators(presets: Dict[str, dict], notes: List[str]) -> List[str]:
    """Static checks the reference would only fail on at runtime.

    An *extra* operand is legal-but-ignored (``applyOp`` only reads ``a[0..2]``:
    eight shipped formulas such as ``circulararrow.xB = "+- xH 0 dxB 0"`` carry a
    trailing operand), so it is reported as a note. A *missing* operand silently
    degrades to ``NaN`` in JS (``undefined * 1``), which no shipped formula does.
    """
    problems: List[str] = []
    for name in sorted(presets):
        definition = presets[name]
        for section in ("adj", "gd"):
            for entry_name, expr in definition.get(section, []):
                op, args = compile_formula(expr)
                arity = OPERATOR_ARITY.get(op)
                where = f"{name}.{section}.{entry_name}"
                if arity is None:
                    problems.append(f"{where}: unknown operator {op!r}")
                elif len(args) < arity:
                    problems.append(f"{where}: {op!r} needs {arity}, got {len(args)}")
                elif len(args) > arity:
                    notes.append(
                        f"{where}: {op!r} takes {arity}, {len(args) - arity} extra "
                        f"ignored ({expr!r})"
                    )
    return problems


# ── geometry.ts:164 -- literal `val` default substitution ──────────────────
_VAL_PREFIX = re.compile(r"^val\s+")


def eff_adj(adj: Sequence[Optional[float]], definition: dict, i: int) -> float:
    """``effAdj`` (geometry.ts:164-169).

    Separate from the evaluator's substitution: this variant parses the declared
    default as a *literal* ``val N`` string instead of evaluating the formula,
    and falls back to 0 for anything else. Both mechanisms agree on the shipped
    data (every adjust default is a plain ``val N``), which ``build_cases``
    asserts; the Rust port must keep that equivalence in mind when it desugars
    handle defaults.
    """
    supplied = adj[i] if i < len(adj) else None
    if isinstance(supplied, (int, float)) and not isinstance(supplied, bool):
        return float(supplied)
    defaults = definition.get("adj", [])
    if i < len(defaults):
        n = js_number(_VAL_PREFIX.sub("", defaults[i][1].strip()))
        return n if n else 0.0
    return 0.0


# ── Case matrix ─────────────────────────────────────────────────────────────
def adj_variants() -> List[Tuple[Optional[float], ...]]:
    """Default, two uniform handles, then one 50000 handle at every slot."""
    variants: List[Tuple[Optional[float], ...]] = list(BASE_VARIANTS)
    for i in range(HANDLE_SLOTS):
        slot: List[Optional[float]] = [None] * HANDLE_SLOTS
        slot[i] = 50000
        variants.append(tuple(slot))
    return variants


def q6(value: float) -> Optional[float]:
    """Round to 6 decimals, half away from zero, folding integral values to int.

    JSON has no ``NaN``/``Infinity`` literals and ``serde_json`` cannot parse
    them, so non-finite reference results are emitted as ``null``. They are
    real reference behaviour (a degenerate handle makes the preset divide by
    zero: e.g. ``nosmoking`` with ``adj = 50000`` on a square has an inner
    radius of 0), so they are reported separately from hard failures.
    """
    if not math.isfinite(value):
        return None
    quantised = Decimal(repr(value)).quantize(Decimal("0.000001"), rounding=ROUND_HALF_UP)
    if quantised == 0:
        return 0
    if quantised == quantised.to_integral_value():
        return int(quantised)
    return float(quantised)


def _adj_json(value: Optional[float]) -> Optional[float]:
    return None if value is None else q6(float(value))


def build_cases(
    presets: Dict[str, dict],
    failures: List[str],
    nonfinite: List[str],
) -> List[dict]:
    """Evaluate every preset x box x adjust variant; collect errors and NaNs."""
    cases: List[dict] = []
    for geometry in sorted(presets):
        definition = presets[geometry]
        adj_defaults = [(n, f) for n, f in definition.get("adj", [])]
        gd_list = [(n, f) for n, f in definition.get("gd", [])]
        for (w, h) in BOXES:
            for variant in adj_variants():
                case = {
                    "geometry": geometry,
                    "w": w,
                    "h": h,
                    "adj": [_adj_json(v) for v in variant],
                    "effAdj": {},
                    "guides": {},
                }
                try:
                    ev = Evaluator(float(w), float(h), variant, adj_defaults, gd_list)
                except PresetEvalError as exc:
                    failures.append(f"{geometry} w={w} h={h} adj={list(variant)}: {exc}")
                    continue

                # effAdj: the env entries the guide formulas actually read,
                # including the adj<->adj1 alias pair (evaluator.ts:91-97).
                for name in ev.adj_names:
                    case["effAdj"][name] = q6(ev.env[name])
                for alias in ("adj", "adj1"):
                    if alias in ev.env and alias not in case["effAdj"]:
                        case["effAdj"][alias] = q6(ev.env[alias])

                # Cross-check the evaluator's default substitution against
                # geometry.ts's literal `val` substitution (effAdj).
                for i, name in enumerate(ev.adj_names):
                    literal = eff_adj(variant, definition, i)
                    if not _close(literal, ev.env[name]):
                        failures.append(
                            f"{geometry}: effAdj mismatch for {name}: evaluator="
                            f"{ev.env[name]!r} literal={literal!r}"
                        )

                for name in ev.gd_names:
                    value = ev.env[name]
                    if not math.isfinite(value):
                        nonfinite.append(
                            f"{geometry} w={w} h={h} adj={list(variant)}: "
                            f"non-finite guide {name}={value!r} -> null"
                        )
                    case["guides"][name] = q6(value)
                cases.append(case)
    return cases


def _close(a: float, b: float) -> bool:
    if a != a or b != b:
        return a != a and b != b
    return abs(a - b) <= 1e-6 * max(1.0, abs(a), abs(b))


# ── Serialisation ───────────────────────────────────────────────────────────
def serialise(doc: dict) -> str:
    """One case per line (stable, diffable), with sorted object keys."""
    lines = [
        json.dumps(case, sort_keys=True, separators=(",", ":"), allow_nan=False)
        for case in doc["cases"]
    ]
    body = ",\n".join(lines)
    version = json.dumps(doc["version"])
    return '{"cases":[\n' + body + '\n],"version":' + version + "}\n"


def load_presets(path: Path) -> Dict[str, dict]:
    with path.open("r", encoding="utf-8") as fh:
        data = json.load(fh)
    if not isinstance(data, dict):
        raise SystemExit(f"{path}: expected a JSON object of preset definitions")
    return data


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--presets", type=Path, default=DEFAULT_PRESETS)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write; fail if the checked-in expected.json is stale",
    )
    args = parser.parse_args(argv)

    presets = load_presets(args.presets)
    notes: List[str] = []
    problems = validate_operators(presets, notes)
    failures: List[str] = list(problems)
    nonfinite: List[str] = []
    cases = build_cases(presets, failures, nonfinite)
    text = serialise({"version": 1, "cases": cases})

    variants = len(adj_variants())
    print(f"presets: {len(presets)}  boxes: {len(BOXES)}  variants: {variants}")
    print(f"cases: {len(cases)}")
    print(f"guides per case: total={sum(len(c['guides']) for c in cases)}")
    for title, messages, limit, stream in (
        ("FAILURES", failures, 40, sys.stderr),
        ("data notes", notes, 5, sys.stdout),  # legal-but-ignored data quirks
        ("non-finite guides (emitted as null)", nonfinite, 5, sys.stdout),
    ):
        if not messages:
            continue
        print(f"{title}: {len(messages)}", file=stream)
        for message in messages[:limit]:
            print(f"  {message}", file=stream)
        if len(messages) > limit:
            print(f"  ... {len(messages) - limit} more", file=stream)

    if args.check:
        current = args.out.read_text(encoding="utf-8") if args.out.exists() else ""
        if current != text:
            print(f"stale: {args.out}", file=sys.stderr)
            return 1
        print(f"up to date: {args.out}")
        return 1 if failures else 0

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(text, encoding="utf-8")
    print(f"wrote {args.out} ({args.out.stat().st_size} bytes)")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
