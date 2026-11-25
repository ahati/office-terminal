//! ECMA-376 §19.5.31.3 preset-geometry formula VM and guide evaluation.
//!
//! Port of the reference evaluator
//! (`packages/core/src/shape/preset-geometry/evaluator.ts:1-149`) plus the
//! `effAdj` adjust-default substitution at `geometry.ts:164-168`.
//!
//! Every `<a:gd>` formula is a postfix expression. A token resolves to an
//! already-defined name first (built-in, adjust, or earlier guide) and to a
//! numeric literal second (`evaluator.ts:113-118`). Angle-bearing operators
//! take their arguments in 60 000ths of a degree (`evaluator.ts:10-16`), the
//! same unit as `cd4`, `3cd4`, and `<a:arcTo>` angles.

use std::collections::{BTreeMap, HashMap};

use super::preset_data::{preset_def, PresetDef};

/// 60 000ths of a degree per full revolution (2π radians).
/// ref `evaluator.ts:15`
const CD: f64 = 21_600_000.0;

/// Radians per 60 000th of a degree. ref `evaluator.ts:16`
const DEG60K_TO_RAD: f64 = std::f64::consts::TAU / CD;

/// Evaluated adjust values and guides of one preset instance.
///
/// `adj` holds the effective adjust values keyed by their declared names. The
/// reference mirrors the `adj`/`adj1` alias in its environment
/// (`evaluator.ts:95-96`), so both spellings are present here too.
///
/// `gd` holds every evaluated guide keyed by name. When a preset redefines a
/// guide (e.g. `circulararrow`'s `dxI`), the last value wins, exactly like the
/// reference's mutable environment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresetGuides {
    pub adj: BTreeMap<String, f64>,
    pub gd: BTreeMap<String, f64>,
}

/// Evaluate every adjust default and guide of `geometry`.
///
/// `adj` holds caller-supplied raw adjust values (1/100 000 units, never
/// normalised); `None` or a short slice falls back to the preset default.
/// Returns `None` for an unknown preset or a malformed formula.
pub fn evaluate_preset(geometry: &str, w: f64, h: f64, adj: &[Option<f64>]) -> Option<PresetGuides> {
    let def = preset_def(geometry)?;
    let env = eval_env(def, w, h, adj)?;

    let mut guides = PresetGuides::default();
    // ref `evaluator.ts:91-97`: adjusts are named in the preset's declaration
    // order, with caller values overriding the declared default.
    for (index, (name, _)) in def.adj.iter().enumerate() {
        let value = effective_adj(def, index, adj);
        guides.adj.insert(name.clone(), value);
        match name.as_str() {
            "adj" => {
                guides.adj.insert("adj1".to_string(), value);
            }
            "adj1" => {
                guides.adj.insert("adj".to_string(), value);
            }
            _ => {}
        }
    }

    // ref `evaluator.ts:100-102`: guides are evaluated in declaration order.
    for (name, _) in &def.gd {
        guides.gd.insert(name.clone(), *env.get(name.as_str())?);
    }

    Some(guides)
}

/// Full name → value environment of a preset: built-ins, effective adjusts,
/// and evaluated guides.
///
/// The path executor resolves bare `<a:path>` command tokens through this map
/// (ref `geometry.ts:288-292` calls `evaluator.resolve` per token), avoiding a
/// re-evaluation of the guide list for every command. Adjust aliases are
/// included, just like the reference environment.
pub fn preset_env(geometry: &str, w: f64, h: f64, adj: &[Option<f64>]) -> Option<HashMap<&'static str, f64>> {
    let def = preset_def(geometry)?;
    eval_env(def, w, h, adj)
}

/// Evaluate a raw postfix formula against `vars`.
///
/// Tokens resolve to `vars` entries first and decimal numeric literals second
/// (`evaluator.ts:113-118`). Extra operands beyond an operator's arity are
/// evaluated and then ignored, matching the reference (`applyOp` only reads the
/// indices it needs; eight shipped `+-` guides carry a redundant trailing `0`).
pub fn eval_formula(formula: &str, vars: &HashMap<&'static str, f64>) -> Option<f64> {
    let mut tokens = formula.split_whitespace();
    let op = tokens.next()?;
    let args = tokens
        .map(|token| resolve_token(token, vars))
        .collect::<Option<Vec<f64>>>()?;
    apply_op(op, &args)
}

/// Resolve one bare token: environment name first, numeric literal second.
/// ref `evaluator.ts:113-118` (`resolve`).
pub fn resolve_token(token: &str, vars: &HashMap<&'static str, f64>) -> Option<f64> {
    vars.get(token).copied().or_else(|| token.parse::<f64>().ok())
}

/// Build the evaluator environment for one preset instance.
///
/// Takes a `'static` definition so the names inserted into the map borrow the
/// table directly instead of being cloned per shape.
fn eval_env(def: &'static PresetDef, w: f64, h: f64, adj: &[Option<f64>]) -> Option<HashMap<&'static str, f64>> {
    let mut env = builtin_vars(w, h);

    // ref `evaluator.ts:91-97`: a caller-supplied value (including 0) wins;
    // otherwise the preset's declared default is used.
    for (index, (name, _)) in def.adj.iter().enumerate() {
        let value = effective_adj(def, index, adj);
        let name: &'static str = name.as_str();
        env.insert(name, value);
        // Many shapes name the first adjust "adj" but reference it as "adj1"
        // (and vice versa); mirror both spellings.
        if name == "adj" {
            env.insert("adj1", value);
        } else if name == "adj1" {
            env.insert("adj", value);
        }
    }

    // ref `evaluator.ts:100-102`: the guide list runs in declaration order and
    // each result becomes available to the following guides.
    for (name, formula) in &def.gd {
        let value = eval_formula(formula, &env)?;
        env.insert(name.as_str(), value);
    }

    Some(env)
}

/// Effective adjust value `index`: caller override, else JSON default.
///
/// ref `geometry.ts:164-168` (`effAdj`): defaults are read straight out of the
/// definition as `Number("val N".replace(/^val\s+/, '')) || 0` and stay in raw
/// 1/100 000 units; nothing is normalised or clamped here.
fn effective_adj(def: &PresetDef, index: usize, supplied: &[Option<f64>]) -> f64 {
    if let Some(value) = supplied.get(index).copied().flatten() {
        return value;
    }
    def.adj
        .get(index)
        .map_or(0.0, |(_, formula)| adj_default_value(formula))
}

/// Numeric value of a `val <number>` adjust default, or 0 when the formula is
/// not a plain literal (ref `geometry.ts:167`).
fn adj_default_value(formula: &str) -> f64 {
    let numeric = match formula.strip_prefix("val") {
        // The reference regex requires whitespace after `val`.
        Some(rest) if rest.trim_start().len() != rest.len() => rest.trim(),
        _ => "",
    };
    numeric.parse::<f64>().unwrap_or(0.0)
}

/// ECMA-376 §20.1.9.2 built-in guide values.
/// ref `evaluator.ts:66-88`
fn builtin_vars(w: f64, h: f64) -> HashMap<&'static str, f64> {
    let ss = js_min(w, h);
    let ls = js_max(w, h);

    let mut env: HashMap<&'static str, f64> = HashMap::with_capacity(50);
    env.insert("w", w);
    env.insert("h", h);
    env.insert("l", 0.0);
    env.insert("t", 0.0);
    env.insert("r", w);
    env.insert("b", h);
    env.insert("hc", w / 2.0);
    env.insert("vc", h / 2.0);

    for (name, divisor) in [
        ("wd2", 2.0),
        ("wd3", 3.0),
        ("wd4", 4.0),
        ("wd5", 5.0),
        ("wd6", 6.0),
        ("wd8", 8.0),
        ("wd10", 10.0),
        ("wd12", 12.0),
        ("wd16", 16.0),
        ("wd32", 32.0),
    ] {
        env.insert(name, w / divisor);
    }
    for (name, divisor) in [
        ("hd2", 2.0),
        ("hd3", 3.0),
        ("hd4", 4.0),
        ("hd5", 5.0),
        ("hd6", 6.0),
        ("hd8", 8.0),
        ("hd10", 10.0),
        ("hd12", 12.0),
        ("hd16", 16.0),
        ("hd32", 32.0),
    ] {
        env.insert(name, h / divisor);
    }
    for (name, divisor) in [
        ("ssd2", 2.0),
        ("ssd4", 4.0),
        ("ssd6", 6.0),
        ("ssd8", 8.0),
        ("ssd16", 16.0),
        ("ssd32", 32.0),
    ] {
        env.insert(name, ss / divisor);
    }
    for (name, divisor) in [
        ("lsd2", 2.0),
        ("lsd4", 4.0),
        ("lsd6", 6.0),
        ("lsd8", 8.0),
        ("lsd16", 16.0),
        ("lsd32", 32.0),
    ] {
        env.insert(name, ls / divisor);
    }

    env.insert("ss", ss);
    env.insert("ls", ls);
    env.insert("cd", CD);
    env.insert("cd2", CD / 2.0);
    env.insert("cd4", CD / 4.0);
    env.insert("cd8", CD / 8.0);
    env.insert("3cd4", 3.0 * CD / 4.0);
    env.insert("3cd8", 3.0 * CD / 8.0);
    env.insert("5cd8", 5.0 * CD / 8.0);
    env.insert("7cd8", 7.0 * CD / 8.0);

    env
}

/// Apply a postfix operator to its operands.
/// ref `evaluator.ts:126-146` (`applyOp`).
fn apply_op(op: &str, args: &[f64]) -> Option<f64> {
    let a = |index: usize| args.get(index).copied();
    Some(match op {
        "val" => a(0)?,
        "*/" => (a(0)? * a(1)?) / a(2)?,
        "+-" => a(0)? + a(1)? - a(2)?,
        "+/" => (a(0)? + a(1)?) / a(2)?,
        "?:" => {
            if a(0)? > 0.0 {
                a(1)?
            } else {
                a(2)?
            }
        }
        "abs" => a(0)?.abs(),
        "min" => js_min(a(0)?, a(1)?),
        "max" => js_max(a(0)?, a(1)?),
        "pin" => {
            let lo = a(0)?;
            let value = a(1)?;
            let hi = a(2)?;
            // Kept as the reference's ternary chain rather than f64::clamp:
            // NaN falls through to `value` and inverted bounds yield `lo`.
            #[allow(clippy::manual_clamp)]
            let pinned = if value < lo {
                lo
            } else if value > hi {
                hi
            } else {
                value
            };
            pinned
        }
        "sqrt" => js_max(0.0, a(0)?).sqrt(),
        "mod" => {
            let (x, y, z) = (a(0)?, a(1)?, a(2)?);
            (x * x + y * y + z * z).sqrt()
        }
        "sin" => a(0)? * (a(1)? * DEG60K_TO_RAD).sin(),
        "cos" => a(0)? * (a(1)? * DEG60K_TO_RAD).cos(),
        "tan" => a(0)? * (a(1)? * DEG60K_TO_RAD).tan(),
        // at2 returns 60 000ths of a degree.
        "at2" => a(1)?.atan2(a(0)?) / DEG60K_TO_RAD,
        "cat2" => a(0)? * a(2)?.atan2(a(1)?).cos(),
        "sat2" => a(0)? * a(2)?.atan2(a(1)?).sin(),
        _ => return None,
    })
}

/// `Math.min` semantics, including NaN propagation (Rust `f64::min` ignores
/// NaN operands). ref `evaluator.ts:66`, `evaluator.ts:132-133`, `:137`.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// `Math.max` semantics; see [`js_min`].
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::shapes::preset_data::preset_names;

    /// `eval_formula` looks names up in a `&'static str` map, so the fixtures
    /// use literal keys.
    fn vars(pairs: &[(&'static str, f64)]) -> HashMap<&'static str, f64> {
        pairs.iter().copied().collect()
    }

    fn assert_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("formula should evaluate");
        assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
    }

    #[test]
    fn formula_operators() {
        let env = vars(&[("x", 6.0), ("y", 7.0), ("z", 2.0)]);

        assert_eq!(eval_formula("val x", &env), Some(6.0));
        assert_eq!(eval_formula("val 42", &env), Some(42.0));
        assert_eq!(eval_formula("*/ x y z", &env), Some(21.0));
        assert_eq!(eval_formula("+- x y z", &env), Some(11.0));
        assert_eq!(eval_formula("+/ x y z", &env), Some(6.5));
        assert_eq!(eval_formula("?: x y z", &env), Some(7.0));
        assert_eq!(eval_formula("?: 0 y z", &env), Some(2.0));
        assert_eq!(eval_formula("?: -1 y z", &env), Some(2.0));
        assert_eq!(eval_formula("pin 0 5 10", &env), Some(5.0));
        assert_eq!(eval_formula("pin 0 -5 10", &env), Some(0.0));
        assert_eq!(eval_formula("pin 0 15 10", &env), Some(10.0));
        assert_eq!(eval_formula("abs -3", &env), Some(3.0));
        assert_eq!(eval_formula("min x y", &env), Some(6.0));
        assert_eq!(eval_formula("max x y", &env), Some(7.0));
        assert_eq!(eval_formula("sqrt 9", &env), Some(3.0));
        assert_eq!(eval_formula("sqrt -9", &env), Some(0.0));
        assert_eq!(eval_formula("mod 3 4 12", &env), Some(13.0));
        assert_eq!(eval_formula("mod 0 0 0", &env), Some(0.0));
    }

    #[test]
    fn formula_angle_operators() {
        let env = vars(&[]);
        let quarter_turn = CD / 4.0; // 5 400 000

        // sin(cd/12) = sin(30°) = 1/2.
        assert_close(eval_formula("sin 2 1800000", &env), 1.0);
        assert_close(eval_formula("cos 2 0", &env), 2.0);
        assert_close(eval_formula("cos 2 10800000", &env), -2.0);
        assert_close(eval_formula("tan 1 0", &env), 0.0);
        assert_close(eval_formula("at2 1 1", &env), CD / 8.0);
        assert_close(eval_formula("at2 0 1", &env), quarter_turn);
        // cat2/sat2 project (x, y) onto an ellipse: atan2(z, y) = 0 here, so
        // cat2 = x·cos(0) and sat2 = x·sin(0).
        assert_close(eval_formula("cat2 10 5 0", &env), 10.0);
        assert_close(eval_formula("sat2 10 5 0", &env), 0.0);
    }

    #[test]
    fn formula_rejects_malformed_input() {
        let env = vars(&[("x", 1.0)]);
        assert_eq!(eval_formula("", &env), None);
        assert_eq!(eval_formula("bogus 1 2 3", &env), None);
        assert_eq!(eval_formula("val missing", &env), None);
        assert_eq!(eval_formula("*/ 1 2", &env), None); // operand missing
        assert_eq!(eval_formula("pin 0 1", &env), None);
    }

    #[test]
    fn formula_ignores_extra_operands() {
        // Eight shipped `+-` guides (circular arrows) carry a trailing `0`
        // that the reference's `applyOp` never reads.
        let env = vars(&[("a", 1.0), ("b", 2.0), ("c", 3.0)]);
        assert_eq!(eval_formula("+- a b c 99", &env), Some(0.0));
        assert_eq!(eval_formula("val a 99", &env), Some(1.0));

        let guides = evaluate_preset("circulararrow", 200.0, 200.0, &[]);
        assert!(guides.expect("circulararrow evaluates").gd.contains_key("xB"));
    }

    #[test]
    fn adj_aliasing_both_directions() {
        // `triangle` declares `adj` but guides reference `adj`; `adj1` is the
        // mirrored alias.
        let triangle = evaluate_preset("triangle", 200.0, 100.0, &[]).expect("triangle");
        assert_eq!(triangle.adj.get("adj"), Some(&50000.0));
        assert_eq!(triangle.adj.get("adj1"), Some(&50000.0));

        // `bentconnector3` declares `adj1`; `adj` is the mirrored alias.
        let bent = evaluate_preset("bentconnector3", 100.0, 50.0, &[None]).expect("bentconnector3");
        assert_eq!(bent.adj.get("adj1"), Some(&50000.0));
        assert_eq!(bent.adj.get("adj"), Some(&50000.0));
        assert_close(bent.gd.get("x1").copied(), 50.0);

        // Non-`adj` names (`hf`/`vf`) get no alias. ref evaluator.ts:95-96.
        let pentagon = evaluate_preset("pentagon", 100.0, 100.0, &[]).expect("pentagon");
        assert_eq!(pentagon.adj.get("hf"), Some(&105146.0));
        assert_eq!(pentagon.adj.get("vf"), Some(&110557.0));
        assert!(!pentagon.adj.contains_key("adj"));
        assert!(!pentagon.adj.contains_key("adj1"));
    }

    #[test]
    fn defaults_then_overrides() {
        let defaults = evaluate_preset("roundrect", 100.0, 50.0, &[]).expect("roundrect");
        assert_eq!(defaults.adj.get("adj"), Some(&16667.0));

        // An explicit `None` (or a short slice) inherits the JSON default.
        let missing = evaluate_preset("roundrect", 100.0, 50.0, &[None, None]).expect("roundrect");
        assert_eq!(missing.gd, defaults.gd);

        // A caller value replaces the default and is used raw (not /100000).
        let overridden = evaluate_preset("roundrect", 100.0, 50.0, &[Some(25000.0)]).expect("roundrect");
        assert_eq!(overridden.adj.get("adj"), Some(&25000.0));
        assert_close(overridden.gd.get("x1").copied(), 12.5);
        assert_close(overridden.gd.get("x2").copied(), 87.5);

        // 0 is a real override, not "use the default".
        let zero = evaluate_preset("roundrect", 100.0, 50.0, &[Some(0.0)]).expect("roundrect");
        assert_eq!(zero.adj.get("adj"), Some(&0.0));
        assert_eq!(zero.gd.get("x1"), Some(&0.0));
    }

    #[test]
    fn ss_uses_shorter_side() {
        // Wide box: ss = h = 100, so x1 = 100·16667/100000, not 200·….
        let env = preset_env("roundrect", 200.0, 100.0, &[]).expect("roundrect env");
        assert_eq!(env.get("ss"), Some(&100.0));
        assert_eq!(env.get("ls"), Some(&200.0));
        let guides = evaluate_preset("roundrect", 200.0, 100.0, &[]).expect("roundrect");
        assert_close(guides.gd.get("x1").copied(), 16.667);

        // Tall box: ss = w = 100 and ls = h = 300.
        let env = preset_env("octagon", 100.0, 300.0, &[]).expect("octagon env");
        assert_eq!(env.get("ss"), Some(&100.0));
        assert_eq!(env.get("ls"), Some(&300.0));
        let guides = evaluate_preset("octagon", 100.0, 300.0, &[]).expect("octagon");
        assert_close(guides.gd.get("x1").copied(), 29.289);
    }

    #[test]
    fn triangle_hand_checked() {
        // triangle: default adj = 50000, w = 200, h = 100.
        //   a  = pin 0 50000 100000 = 50000
        //   x1 = w·a/200000 = 50
        //   x2 = w·a/100000 = 100
        //   x3 = x1 + wd2 − 0 = 50 + 100 = 150
        let guides = evaluate_preset("triangle", 200.0, 100.0, &[]).expect("triangle");
        assert_close(guides.gd.get("a").copied(), 50000.0);
        assert_close(guides.gd.get("x1").copied(), 50.0);
        assert_close(guides.gd.get("x2").copied(), 100.0);
        assert_close(guides.gd.get("x3").copied(), 150.0);
    }

    #[test]
    fn roundrect_hand_checked() {
        // roundrect: default adj = 16667, w = 100, h = 50 → ss = 50.
        //   a  = pin 0 16667 50000      = 16667
        //   x1 = ss·a/100000            = 8.3335
        //   x2 = r − x1                 = 91.6665
        //   y2 = b − x1                 = 41.6665
        //   il = x1·29289/100000        = 2.440798815
        //   ir = r − il, ib = b − il
        let guides = evaluate_preset("roundrect", 100.0, 50.0, &[]).expect("roundrect");
        assert_close(guides.gd.get("a").copied(), 16667.0);
        assert_close(guides.gd.get("x1").copied(), 8.3335);
        assert_close(guides.gd.get("x2").copied(), 91.6665);
        assert_close(guides.gd.get("y2").copied(), 41.6665);
        assert_close(guides.gd.get("il").copied(), 2.440_798_815);
        assert_close(guides.gd.get("ir").copied(), 97.559_201_185);
        assert_close(guides.gd.get("ib").copied(), 47.559_201_185);
    }

    #[test]
    fn righttriangle_hand_checked() {
        // rttriangle has no adjusts: w = 120, h = 60.
        //   it = h·7/12 = 35, ir = w·7/12 = 70, ib = h·11/12 = 55
        let guides = evaluate_preset("rttriangle", 120.0, 60.0, &[]).expect("rttriangle");
        assert!(guides.adj.is_empty());
        assert_eq!(guides.gd.len(), 3);
        assert_close(guides.gd.get("it").copied(), 35.0);
        assert_close(guides.gd.get("ir").copied(), 70.0);
        assert_close(guides.gd.get("ib").copied(), 55.0);
    }

    #[test]
    fn every_adjust_default_is_a_numeric_literal() {
        // `effective_adj` uses the reference's `effAdj` numeric parse instead of
        // running the default through the VM. That is only equivalent while
        // every shipped default is `val <number>`, which this test enforces.
        for name in preset_names() {
            let def = crate::compiler::shapes::preset_data::preset_def(name).expect("definition");
            assert!(!def.paths.is_empty(), "{name} has no paths");
            if def.adj.is_empty() {
                continue;
            }
            let guides = evaluate_preset(name, 100.0, 100.0, &[]).expect("preset evaluates");
            for (declared, formula) in &def.adj {
                let tokens: Vec<&str> = formula.split_whitespace().collect();
                assert_eq!(tokens.len(), 2, "{name}: {formula}");
                assert_eq!(tokens[0], "val", "{name}: {formula}");
                assert!(tokens[1].parse::<f64>().is_ok(), "{name}: {formula}");
                assert_eq!(
                    guides.adj.get(declared),
                    Some(&adj_default_value(formula)),
                    "{name}: {declared}"
                );
            }
        }
    }

    #[test]
    fn preset_table_and_case_insensitive_lookup() {
        let names = preset_names();
        assert_eq!(names.len(), 186);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(names.contains(&"roundrect"));

        let upper = evaluate_preset("RoundRect", 100.0, 50.0, &[]).expect("case-insensitive");
        assert_eq!(upper.adj.get("adj"), Some(&16667.0));

        assert!(evaluate_preset("not-a-preset", 100.0, 50.0, &[]).is_none());
        assert!(preset_env("not-a-preset", 1.0, 1.0, &[]).is_none());
    }
}
