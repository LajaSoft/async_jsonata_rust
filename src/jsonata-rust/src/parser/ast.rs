use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct AstNode {
    pub id: String,
    pub token_type: String,
    pub node_type: String,
    pub value: Value,
    pub position: usize,
    pub fields: Map<String, Value>,
}

impl AstNode {
    pub fn new(id: String, token_type: String, value: Value, position: usize) -> Self {
        let mut fields = Map::new();
        fields.insert("id".to_string(), Value::String(id.clone()));
        fields.insert("type".to_string(), Value::String(token_type.clone()));
        fields.insert("value".to_string(), value.clone());
        fields.insert(
            "position".to_string(),
            json!(position as u64),
        );

        Self {
            id,
            token_type: token_type.clone(),
            node_type: token_type,
            value,
            position,
            fields,
        }
    }

    pub fn set_type<S: Into<String>>(&mut self, new_type: S) {
        let new_type = new_type.into();
        self.node_type = new_type.clone();
        self.fields
            .insert("type".to_string(), Value::String(new_type));
    }

    pub fn set_value(&mut self, value: Value) {
        self.value = value.clone();
        self.fields.insert("value".to_string(), value);
    }

    pub fn set_field(&mut self, key: &str, value: Value) {
        self.fields.insert(key.to_string(), value);
    }

    pub fn push_node(&mut self, key: &str, node: AstNode) {
        let entry = self
            .fields
            .entry(key.to_string())
            .or_insert_with(|| Value::Array(vec![]));
        let array = entry.as_array_mut().expect("field should be array");
        array.push(node.into());
    }

    pub fn set_node(&mut self, key: &str, node: AstNode) {
        self.fields.insert(key.to_string(), node.into());
    }
}

impl From<AstNode> for Value {
    fn from(mut node: AstNode) -> Self {
        node.fields
            .insert("id".to_string(), Value::String(node.id.clone()));
        node.fields.insert(
            "value".to_string(),
            node.value.clone(),
        );
        node.fields.insert(
            "type".to_string(),
            Value::String(node.node_type.clone()),
        );
        node.fields.insert(
            "position".to_string(),
            json!(node.position as u64),
        );
        Value::Object(node.fields)
    }
}
