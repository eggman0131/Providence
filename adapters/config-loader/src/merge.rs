//! Deep merge of TOML layers (docs/40-parameterisation.md §4): later layers
//! override earlier ones **by key** — tables merge recursively, scalars and
//! arrays replace wholesale.

/// Merge `overlay` into `base`, overriding by key.
pub fn deep_merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, overlay_value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(base_value) => deep_merge(base_value, overlay_value),
                    None => {
                        base_table.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

#[cfg(test)]
mod tests {
    use super::deep_merge;

    fn parse(text: &str) -> toml::Value {
        toml::from_str(text).expect("test TOML must parse")
    }

    #[test]
    fn tables_merge_recursively_scalars_replace() {
        let mut base = parse("[a]\nx = 1\ny = 2\n[b]\nz = 3\n");
        let overlay = parse("[a]\ny = 20\n");
        deep_merge(&mut base, overlay);
        assert_eq!(base, parse("[a]\nx = 1\ny = 20\n[b]\nz = 3\n"));
    }

    #[test]
    fn new_keys_are_added() {
        let mut base = parse("[a]\nx = 1\n");
        let overlay = parse("[c]\nw = 4\n");
        deep_merge(&mut base, overlay);
        assert_eq!(base, parse("[a]\nx = 1\n[c]\nw = 4\n"));
    }
}
