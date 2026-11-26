//! JSON Pointers (RFC 6901) and JSON Patches (RFC 6902) over the project's
//! model as JSON: what an agent reads and changes of it when no other tool
//! does (`project_json`, `patch_project`).

use serde_json::Value;

/// A pointer's steps, unescaped (`~1` is `/`, `~0` is `~`).
fn steps(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = pointer.strip_prefix('/') else {
        return Err(format!("a pointer starts with \"/\": {pointer}"));
    };
    Ok(rest
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// An array's index from a step (`len` allowed when `end`: past the last).
fn index(step: &str, len: usize, end: bool) -> Result<usize, String> {
    if end && step == "-" {
        return Ok(len);
    }
    let ok = !step.is_empty()
        && step.bytes().all(|b| b.is_ascii_digit())
        && (step == "0" || !step.starts_with('0'));
    let i: usize = if ok {
        step.parse().map_err(|_| format!("not an index: {step}"))?
    } else {
        return Err(format!("not an index: {step}"));
    };
    if i < len || (end && i == len) {
        Ok(i)
    } else {
        Err(format!("index {i} past the end ({len})"))
    }
}

/// What `pointer` points at in `doc`.
pub(crate) fn get<'a>(doc: &'a Value, pointer: &str) -> Result<&'a Value, String> {
    let mut at = doc;
    for step in steps(pointer)? {
        at = match at {
            Value::Object(m) => m
                .get(&step)
                .ok_or_else(|| format!("nothing at {pointer} (no \"{step}\")"))?,
            Value::Array(a) => &a[index(&step, a.len(), false)?],
            _ => return Err(format!("nothing at {pointer} (\"{step}\" in a value)")),
        };
    }
    Ok(at)
}

fn get_mut<'a>(
    doc: &'a mut Value,
    steps: &[String],
    pointer: &str,
) -> Result<&'a mut Value, String> {
    let mut at = doc;
    for step in steps {
        at = match at {
            Value::Object(m) => m
                .get_mut(step)
                .ok_or_else(|| format!("nothing at {pointer} (no \"{step}\")"))?,
            Value::Array(a) => {
                let i = index(step, a.len(), false)?;
                &mut a[i]
            }
            _ => return Err(format!("nothing at {pointer} (\"{step}\" in a value)")),
        };
    }
    Ok(at)
}

/// `value` put at `pointer`: a member set, an array's element inserted
/// (`-` after the last), or the whole document.
fn add(doc: &mut Value, pointer: &str, value: Value) -> Result<(), String> {
    let mut path = steps(pointer)?;
    let Some(last) = path.pop() else {
        *doc = value;
        return Ok(());
    };
    match get_mut(doc, &path, pointer)? {
        Value::Object(m) => {
            m.insert(last, value);
        }
        Value::Array(a) => {
            let i = index(&last, a.len(), true)?;
            a.insert(i, value);
        }
        _ => return Err(format!("cannot add at {pointer}")),
    }
    Ok(())
}

/// What is at `pointer`, taken out.
fn remove(doc: &mut Value, pointer: &str) -> Result<Value, String> {
    let mut path = steps(pointer)?;
    let Some(last) = path.pop() else {
        return Err("the whole document cannot be removed".into());
    };
    match get_mut(doc, &path, pointer)? {
        Value::Object(m) => m
            .remove(&last)
            .ok_or_else(|| format!("nothing at {pointer}")),
        Value::Array(a) => {
            let i = index(&last, a.len(), false)?;
            Ok(a.remove(i))
        }
        _ => Err(format!("nothing at {pointer}")),
    }
}

/// `patch` (an RFC 6902 array of operations) applied to `doc` in order:
/// all of it or, at the first that fails, none (`doc` as it was).
pub(crate) fn apply(doc: &mut Value, patch: &Value) -> Result<(), String> {
    let Value::Array(ops) = patch else {
        return Err("a patch is an array of operations".into());
    };
    let mut work = doc.clone();
    for (n, op) in ops.iter().enumerate() {
        let fail = |e: String| format!("operation {n}: {e}");
        let kind = op
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("no \"op\"".into()))?;
        let path = op
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("no \"path\"".into()))?;
        let value = || {
            op.get("value")
                .cloned()
                .ok_or_else(|| fail("no \"value\"".into()))
        };
        let from = || {
            op.get("from")
                .and_then(Value::as_str)
                .ok_or_else(|| fail("no \"from\"".into()))
        };
        match kind {
            "add" => add(&mut work, path, value()?).map_err(fail)?,
            "remove" => {
                remove(&mut work, path).map_err(fail)?;
            }
            "replace" => {
                let v = value()?;
                let mut s = steps(path).map_err(fail)?;
                if s.is_empty() {
                    work = v;
                } else {
                    // It must be there already.
                    get(&work, path).map_err(fail)?;
                    let last = s.pop().unwrap_or_default();
                    match get_mut(&mut work, &s, path).map_err(fail)? {
                        Value::Object(m) => {
                            m.insert(last, v);
                        }
                        Value::Array(a) => {
                            let i = index(&last, a.len(), false).map_err(fail)?;
                            a[i] = v;
                        }
                        _ => return Err(fail(format!("cannot replace at {path}"))),
                    }
                }
            }
            "move" => {
                let from = from()?;
                if path.starts_with(&format!("{from}/")) {
                    return Err(fail(format!("cannot move {from} into itself")));
                }
                let v = remove(&mut work, from).map_err(fail)?;
                add(&mut work, path, v).map_err(fail)?;
            }
            "copy" => {
                let v = get(&work, from()?).map_err(fail)?.clone();
                add(&mut work, path, v).map_err(fail)?;
            }
            "test" => {
                let expected = value()?;
                if *get(&work, path).map_err(fail)? != expected {
                    return Err(fail(format!("test failed at {path}")));
                }
            }
            other => return Err(fail(format!("unknown op \"{other}\""))),
        }
    }
    *doc = work;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointers_find_members_and_elements() {
        let doc = json!({"a": [10, {"b/c": 3, "d~e": 4}], "": 1});
        assert_eq!(get(&doc, "").unwrap(), &doc);
        assert_eq!(get(&doc, "/a/0").unwrap(), &json!(10));
        assert_eq!(get(&doc, "/a/1/b~1c").unwrap(), &json!(3));
        assert_eq!(get(&doc, "/a/1/d~0e").unwrap(), &json!(4));
        assert_eq!(get(&doc, "/").unwrap(), &json!(1));
        assert!(get(&doc, "/a/2").is_err());
        assert!(get(&doc, "/a/01").is_err());
        assert!(get(&doc, "a").is_err());
    }

    /// RFC 6902's operations, and a patch that fails leaving the document
    /// as it was.
    #[test]
    fn patches_apply_whole_or_not_at_all() {
        let mut doc = json!({"tracks": [{"name": "A"}, {"name": "B"}], "x": 1});
        apply(
            &mut doc,
            &json!([
                {"op": "replace", "path": "/tracks/0/name", "value": "Main"},
                {"op": "add", "path": "/tracks/-", "value": {"name": "C"}},
                {"op": "add", "path": "/tracks/1/mute", "value": true},
                {"op": "copy", "from": "/x", "path": "/y"},
                {"op": "move", "from": "/tracks/2", "path": "/tracks/0"},
                {"op": "remove", "path": "/x"},
                {"op": "test", "path": "/y", "value": 1},
            ]),
        )
        .expect("applies");
        assert_eq!(
            doc,
            json!({"tracks": [{"name": "C"}, {"name": "Main"}, {"name": "B", "mute": true}], "y": 1})
        );
        let before = doc.clone();
        let failed = apply(
            &mut doc,
            &json!([
                {"op": "remove", "path": "/y"},
                {"op": "replace", "path": "/nothing", "value": 2},
            ]),
        );
        assert!(failed.unwrap_err().starts_with("operation 1"));
        assert_eq!(doc, before);
        assert!(apply(&mut doc, &json!({"op": "remove"})).is_err());
        assert!(
            apply(
                &mut doc,
                &json!([{"op": "move", "from": "/tracks", "path": "/tracks/0/x"}])
            )
            .is_err()
        );
    }
}
