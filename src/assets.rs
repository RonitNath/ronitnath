use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct AssetManifest {
    entries: HashMap<String, String>,
    css: HashMap<String, Vec<String>>,
    dev_mode: bool,
}

impl AssetManifest {
    pub(crate) fn load(path: &str, dev_mode: bool) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self {
                dev_mode,
                ..Self::default()
            };
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Self {
                dev_mode,
                ..Self::default()
            };
        };

        let mut entries = HashMap::new();
        let mut css: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(obj) = value.as_object() {
            for (key, item) in obj {
                if let Some(file) = item.get("file").and_then(serde_json::Value::as_str) {
                    entries.insert(key.clone(), format!("/ui/dist/{file}"));
                }
                let mut entry_css = Vec::new();
                if let Some(imports) = item.get("imports").and_then(serde_json::Value::as_array) {
                    for imp in imports {
                        if let Some(imp_key) = imp.as_str()
                            && let Some(imp_item) = obj.get(imp_key)
                            && let Some(css_arr) =
                                imp_item.get("css").and_then(serde_json::Value::as_array)
                        {
                            for css_file in css_arr {
                                if let Some(f) = css_file.as_str() {
                                    entry_css.push(format!("/ui/dist/{f}"));
                                }
                            }
                        }
                    }
                }
                if let Some(css_arr) = item.get("css").and_then(serde_json::Value::as_array) {
                    for css_file in css_arr {
                        if let Some(f) = css_file.as_str() {
                            entry_css.push(format!("/ui/dist/{f}"));
                        }
                    }
                }
                if !entry_css.is_empty() {
                    css.insert(key.clone(), entry_css);
                }
            }
        }
        Self {
            entries,
            css,
            dev_mode,
        }
    }

    pub(crate) fn entry(&self, entry_name: &str) -> Option<String> {
        let full_key = format!("entries/{entry_name}");
        self.entries
            .get(&full_key)
            .or_else(|| self.entries.get(entry_name))
            .cloned()
            .or_else(|| {
                self.dev_mode
                    .then(|| format!("/ui-dev/{}", entry_name.replace(".ts", ".js")))
            })
    }

    pub(crate) fn css_for_entry(&self, entry_name: &str) -> Vec<String> {
        let full_key = format!("entries/{entry_name}");
        self.css
            .get(&full_key)
            .or_else(|| self.css.get(entry_name))
            .cloned()
            .unwrap_or_default()
    }
}
