use std::collections::HashMap;

#[derive(Debug)]
pub(super) struct ControlSequenceScopes<M> {
    layers: Vec<HashMap<String, M>>,
}

impl<M> ControlSequenceScopes<M> {
    pub(super) fn new() -> Self {
        Self {
            layers: vec![HashMap::new()],
        }
    }

    pub(super) fn from_layers(layers: Vec<HashMap<String, M>>) -> Self {
        Self { layers }
    }

    pub(super) fn depth(&self) -> usize {
        self.layers.len()
    }

    pub(super) fn begin_group(&mut self) {
        self.layers.push(HashMap::new());
    }

    pub(super) fn end_group(&mut self) -> bool {
        if self.layers.len() <= 1 {
            return false;
        }
        self.layers.pop();
        true
    }

    pub(super) fn insert_root(&mut self, name: String, meaning: M) {
        for layer in self.layers.iter_mut().skip(1) {
            layer.remove(&name);
        }
        if let Some(root) = self.layers.first_mut() {
            root.insert(name, meaning);
        }
    }

    pub(super) fn insert_current(&mut self, name: String, meaning: M) {
        if let Some(current) = self.layers.last_mut() {
            current.insert(name, meaning);
        }
    }

    pub(super) fn get_visible(&self, name: &str) -> Option<&M> {
        self.layers.iter().rev().find_map(|scope| scope.get(name))
    }

    pub(super) fn visible_layer_index(&self, name: &str) -> Option<usize> {
        self.layers
            .iter()
            .rposition(|scope| scope.contains_key(name))
    }

    pub(super) fn get_mut_at(&mut self, layer: usize, name: &str) -> Option<&mut M> {
        self.layers.get_mut(layer)?.get_mut(name)
    }

    pub(super) fn insert_at(&mut self, layer: usize, name: String, meaning: M) {
        if let Some(layer) = self.layers.get_mut(layer) {
            layer.insert(name, meaning);
        }
    }

    pub(super) fn layers(&self) -> &[HashMap<String, M>] {
        &self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::ControlSequenceScopes;

    #[test]
    fn owner_preserves_root_and_group_shadowing() {
        let mut scopes = ControlSequenceScopes::new();
        scopes.insert_current("name".to_string(), "outer");

        assert_eq!(scopes.depth(), 1);
        assert_eq!(scopes.get_visible("name"), Some(&"outer"));
        assert!(!scopes.end_group());

        scopes.begin_group();
        scopes.insert_current("name".to_string(), "inner");

        assert_eq!(scopes.depth(), 2);
        assert_eq!(scopes.get_visible("name"), Some(&"inner"));
        assert!(scopes.end_group());
        assert_eq!(scopes.get_visible("name"), Some(&"outer"));
    }
}
