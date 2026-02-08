use serde_json::Value;

use super::common::map_position;
use super::slots::{
    adjust_stage_slots, apply_slot_aliases, collect_push_from_children, collect_push_slots,
    has_focus, new_parent_slot, seek_parent, set_seeking_parent, slot_level, ParentReferenceState,
};
use super::super::super::error::ParserError;

pub(crate) fn annotate_parent_references(expr: Value) -> Result<Value, ParserError> {
    let mut state = ParentReferenceState::default();
    let mut annotated = annotate_parent_expr(expr, &mut state)?;
    apply_slot_aliases(&mut annotated, &state);
    Ok(annotated)
}

fn annotate_parent_expr(
    expr: Value,
    state: &mut ParentReferenceState,
) -> Result<Value, ParserError> {
    let mut map = match expr {
        Value::Object(map) => map,
        other => return Ok(other),
    };

    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if expr_type == "parent" {
        map.insert("slot".to_string(), new_parent_slot(state));
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "path" {
        return annotate_parent_path(map, state);
    }

    if expr_type == "function" || expr_type == "partial" {
        annotate_field_array(&mut map, "arguments", state)?;
        annotate_field_value(&mut map, "procedure", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "procedure",
                "type",
                "value",
                "position",
                "name",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "lambda" {
        annotate_field_value(&mut map, "body", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "transform" {
        annotate_field_value(&mut map, "pattern", state)?;
        annotate_field_value(&mut map, "update", state)?;
        annotate_field_value(&mut map, "delete", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "apply" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "bind" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "lhs",
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "condition" {
        annotate_field_value(&mut map, "condition", state)?;
        annotate_field_value(&mut map, "then", state)?;
        annotate_field_value(&mut map, "else", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "block" {
        annotate_field_array(&mut map, "expressions", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "unary" {
        let unary_value = map
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if unary_value == "{" {
            if let Some(lhs) = map.remove("lhs") {
                let mut pairs = Vec::new();
                for pair in lhs.as_array().cloned().unwrap_or_default() {
                    let pair_items = pair
                        .as_array()
                        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
                    if pair_items.len() != 2 {
                        return Err(ParserError::new("S0206", map_position(&map)));
                    }
                    let key = annotate_parent_expr(pair_items[0].clone(), state)?;
                    let value = annotate_parent_expr(pair_items[1].clone(), state)?;
                    pairs.push(Value::Array(vec![key, value]));
                }
                map.insert("lhs".to_string(), Value::Array(pairs));
            }
            annotate_field_array(&mut map, "stages", state)?;
            annotate_field_array(&mut map, "predicate", state)?;
            let slots = collect_push_from_children(
                &map,
                &[
                    "type",
                    "value",
                    "position",
                    "keepArray",
                    "keepSingletonArray",
                    "consarray",
                    "tuple",
                    "focus",
                    "index",
                    "slot",
                    "ancestor",
                    "seekingParent",
                ],
            );
            set_seeking_parent(&mut map, slots);
            return Ok(Value::Object(map));
        }
        annotate_field_value(&mut map, "expression", state)?;
        annotate_field_array(&mut map, "expressions", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "binary" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "sort" {
        annotate_field_array(&mut map, "terms", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    annotate_field_value(&mut map, "expression", state)?;
    annotate_field_value(&mut map, "lhs", state)?;
    annotate_field_value(&mut map, "rhs", state)?;
    annotate_field_value(&mut map, "condition", state)?;
    annotate_field_value(&mut map, "then", state)?;
    annotate_field_value(&mut map, "else", state)?;
    annotate_field_value(&mut map, "procedure", state)?;
    annotate_field_value(&mut map, "body", state)?;
    annotate_field_value(&mut map, "pattern", state)?;
    annotate_field_value(&mut map, "update", state)?;
    annotate_field_value(&mut map, "delete", state)?;
    annotate_field_array(&mut map, "arguments", state)?;
    annotate_field_array(&mut map, "expressions", state)?;
    annotate_field_array(&mut map, "terms", state)?;
    annotate_field_array(&mut map, "steps", state)?;
    annotate_field_array(&mut map, "stages", state)?;
    annotate_field_array(&mut map, "predicate", state)?;
    annotate_field_value(&mut map, "group", state)?;
    annotate_field_value(&mut map, "expr", state)?;
    let slots = collect_push_from_children(
        &map,
        &[
            "type",
            "value",
            "position",
            "keepArray",
            "keepSingletonArray",
            "consarray",
            "tuple",
            "focus",
            "index",
            "slot",
            "ancestor",
            "seekingParent",
            "name",
            "descending",
        ],
    );
    set_seeking_parent(&mut map, slots);
    Ok(Value::Object(map))
}

fn annotate_parent_path(
    mut map: serde_json::Map<String, Value>,
    state: &mut ParentReferenceState,
) -> Result<Value, ParserError> {
    let position = map_position(&map);
    let mut steps = Vec::new();
    for step in map
        .remove("steps")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        steps.push(annotate_parent_expr(step, state)?);
    }

    let mut unresolved_slots = Vec::new();
    for step_index in 0..steps.len() {
        adjust_stage_slots(&mut steps[step_index], state)?;
        let step_slots = collect_push_slots(&steps[step_index]);
        for mut slot in step_slots {
            let mut search_index = step_index as isize - 1;
            while slot_level(&slot) > 0 {
                if search_index < 0 {
                    unresolved_slots.push(slot.clone());
                    break;
                }
                let mut candidate = search_index;
                while candidate > 0 {
                    let current_focus = has_focus(&steps[candidate as usize]);
                    let previous_focus = has_focus(&steps[(candidate - 1) as usize]);
                    if !(current_focus && previous_focus) {
                        break;
                    }
                    candidate -= 1;
                }
                let candidate_index = candidate as usize;
                let candidate_step = steps
                    .get_mut(candidate_index)
                    .ok_or_else(|| ParserError::new("S0206", position))?;
                seek_parent(candidate_step, &mut slot, state)?;
                search_index = candidate - 1;
            }
        }
    }

    map.insert("steps".to_string(), Value::Array(steps));
    set_seeking_parent(&mut map, unresolved_slots);
    Ok(Value::Object(map))
}

fn annotate_field_value(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let Some(value) = map.remove(key) else {
        return Ok(());
    };
    let analyzed = annotate_parent_expr(value, state)?;
    map.insert(key.to_string(), analyzed);
    Ok(())
}

fn annotate_field_array(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let Some(value) = map.remove(key) else {
        return Ok(());
    };
    if let Some(items) = value.as_array() {
        let mut analyzed = Vec::with_capacity(items.len());
        for item in items {
            analyzed.push(annotate_parent_expr(item.clone(), state)?);
        }
        map.insert(key.to_string(), Value::Array(analyzed));
        return Ok(());
    }
    let analyzed = annotate_parent_expr(value, state)?;
    map.insert(key.to_string(), analyzed);
    Ok(())
}
