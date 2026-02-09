use std::collections::HashMap;

use serde_json::{json, Value};

use super::super::super::error::ParserError;
use super::common::expr_position;

#[derive(Default)]
pub(super) struct ParentReferenceState {
    next_slot_index: usize,
    slot_label_aliases: HashMap<usize, String>,
}

pub(super) fn collect_push_slots(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => {
            let mut slots = Vec::new();
            for item in items {
                append_slots(&mut slots, collect_push_slots(item));
            }
            slots
        }
        Value::Object(map) => {
            let mut slots = Vec::new();
            if let Some(seeking_parent) = map.get("seekingParent").and_then(Value::as_array) {
                slots.extend(seeking_parent.clone());
            }
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|expr_type| expr_type == "parent")
            {
                if let Some(slot) = map.get("slot").cloned() {
                    slots.push(slot);
                }
            }
            slots
        }
        _ => Vec::new(),
    }
}

pub(super) fn collect_push_from_children(
    map: &serde_json::Map<String, Value>,
    skip_keys: &[&str],
) -> Vec<Value> {
    let mut slots = Vec::new();
    for (key, value) in map {
        if skip_keys.iter().any(|skip| skip == key) {
            continue;
        }
        append_slots(&mut slots, collect_push_slots(value));
    }
    slots
}

fn append_slots(target: &mut Vec<Value>, mut slots: Vec<Value>) {
    target.append(&mut slots);
}

pub(super) fn set_seeking_parent(map: &mut serde_json::Map<String, Value>, slots: Vec<Value>) {
    if slots.is_empty() {
        map.remove("seekingParent");
        return;
    }
    map.insert("seekingParent".to_string(), Value::Array(slots));
}

pub(super) fn new_parent_slot(state: &mut ParentReferenceState) -> Value {
    let index = state.next_slot_index;
    state.next_slot_index += 1;
    json!({
        "label": format!("!{index}"),
        "level": 1,
        "index": index as u64,
    })
}

pub(super) fn adjust_stage_slots(
    step: &mut Value,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let has_stage_fields = step
        .as_object()
        .is_some_and(|map| map.contains_key("stages") || map.contains_key("predicate"));
    if !has_stage_fields {
        return Ok(());
    }

    let mut slots = {
        let Some(step_map) = step.as_object_mut() else {
            return Ok(());
        };
        step_map
            .remove("seekingParent")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
    };
    if slots.is_empty() {
        return Ok(());
    }

    let mut transformed = Vec::with_capacity(slots.len());
    for mut slot in slots.drain(..) {
        if slot_level(&slot) == 1 {
            seek_parent(step, &mut slot, state)?;
        } else {
            let level = slot_level(&slot) - 1;
            set_slot_level(&mut slot, level);
        }
        transformed.push(slot);
    }

    let Some(step_map) = step.as_object_mut() else {
        return Ok(());
    };
    set_seeking_parent(step_map, transformed);
    Ok(())
}

pub(super) fn seek_parent(
    node: &mut Value,
    slot: &mut Value,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let node_position = expr_position(node);
    let node_type = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if node_type == "name" || node_type == "wildcard" {
        let level = slot_level(slot) - 1;
        set_slot_level(slot, level);
        if level == 0 {
            let node_map = node
                .as_object_mut()
                .ok_or_else(|| ParserError::new("S0206", node_position))?;
            if let Some(existing_label) = node_map
                .get("ancestor")
                .and_then(|ancestor| ancestor.get("label"))
                .and_then(Value::as_str)
            {
                if let Some(slot_index) = slot_index(slot) {
                    state
                        .slot_label_aliases
                        .insert(slot_index, existing_label.to_string());
                }
                set_slot_label(slot, existing_label);
            }
            node_map.insert("ancestor".to_string(), slot.clone());
            node_map.insert("tuple".to_string(), Value::Bool(true));
        }
        return Ok(());
    }

    if node_type == "parent" {
        let level = slot_level(slot) + 1;
        set_slot_level(slot, level);
        return Ok(());
    }

    if node_type == "block" {
        let node_map = node
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        node_map.insert("tuple".to_string(), Value::Bool(true));
        if let Some(expressions) = node_map
            .get_mut("expressions")
            .and_then(Value::as_array_mut)
        {
            if let Some(last_expression) = expressions.last_mut() {
                seek_parent(last_expression, slot, state)?;
            }
        }
        return Ok(());
    }

    if node_type == "path" {
        let node_map = node
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        node_map.insert("tuple".to_string(), Value::Bool(true));
        let steps = node_map
            .get_mut("steps")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        if steps.is_empty() {
            return Err(
                ParserError::new("S0217", node_position).with_token(Value::String(node_type))
            );
        }
        let mut step_index = steps.len() as isize - 1;
        if let Some(last_step) = steps.get_mut(step_index as usize) {
            seek_parent(last_step, slot, state)?;
        }
        while slot_level(slot) > 0 && step_index > 0 {
            step_index -= 1;
            if let Some(step) = steps.get_mut(step_index as usize) {
                seek_parent(step, slot, state)?;
            }
        }
        return Ok(());
    }

    Err(ParserError::new("S0217", node_position).with_token(Value::String(node_type)))
}

pub(super) fn has_focus(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.contains_key("focus"))
}

fn slot_index(slot: &Value) -> Option<usize> {
    slot.get("index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

pub(super) fn slot_level(slot: &Value) -> i64 {
    slot.get("level").and_then(Value::as_i64).unwrap_or(0)
}

fn set_slot_level(slot: &mut Value, level: i64) {
    if let Some(slot_map) = slot.as_object_mut() {
        slot_map.insert("level".to_string(), json!(level));
    }
}

fn set_slot_label(slot: &mut Value, label: &str) {
    if let Some(slot_map) = slot.as_object_mut() {
        slot_map.insert("label".to_string(), Value::String(label.to_string()));
    }
}

pub(super) fn apply_slot_aliases(node: &mut Value, state: &ParentReferenceState) {
    match node {
        Value::Array(items) => {
            for item in items {
                apply_slot_aliases(item, state);
            }
        }
        Value::Object(map) => {
            if let Some(slot) = map.get_mut("slot") {
                apply_slot_alias(slot, state);
            }
            if let Some(ancestor) = map.get_mut("ancestor") {
                apply_slot_alias(ancestor, state);
            }
            if let Some(seeking_parent) = map.get_mut("seekingParent").and_then(Value::as_array_mut)
            {
                for slot in seeking_parent {
                    apply_slot_alias(slot, state);
                }
            }
            for (key, value) in map.iter_mut() {
                if key == "slot" || key == "ancestor" || key == "seekingParent" {
                    continue;
                }
                apply_slot_aliases(value, state);
            }
        }
        _ => {}
    }
}

fn apply_slot_alias(slot: &mut Value, state: &ParentReferenceState) {
    let Some(index) = slot_index(slot) else {
        return;
    };
    let Some(label) = state.slot_label_aliases.get(&index) else {
        return;
    };
    set_slot_label(slot, label);
}
