//! Editor drafts survive selection and independent configuration changes.
use std::collections::HashMap;
#[derive(Default)]
pub struct Drafts {
    entries: HashMap<String, (String, String)>,
}
impl Drafts {
    pub fn sync(&mut self, key: &str, model: &str, previous: Option<(&str, &str)>) -> String {
        if let Some((old, text)) = previous {
            if let Some(entry) = self.entries.get_mut(old) {
                entry.1 = text.into();
            }
        }
        let entry = self
            .entries
            .entry(key.into())
            .or_insert_with(|| (model.into(), model.into()));
        if entry.1 == entry.0 || entry.1 == model {
            *entry = (model.into(), model.into());
        }
        entry.1.clone()
    }
    pub fn check_save(&self, key: &str, model: &str) -> Result<(), String> {
        if self
            .entries
            .get(key)
            .is_some_and(|(baseline, text)| baseline != model && text != model && text != baseline)
        {
            Err("The configuration changed while you were editing. Copy your draft and use Discard draft to load the current version before saving.".into())
        } else {
            Ok(())
        }
    }
    pub fn discard(&mut self, key: &str) {
        self.entries.remove(key);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsaved_configuration_survives_preset_changes_and_navigation() {
        let mut drafts = Drafts::default();
        drafts.sync("config", "old config", None);
        assert_eq!(
            drafts.sync(
                "config",
                "changed preset",
                Some(("config", "custom macro draft"))
            ),
            "custom macro draft"
        );
        assert!(drafts.check_save("config", "changed preset").is_err());
        assert_eq!(
            drafts.sync("config", "changed preset", None),
            "custom macro draft"
        );
        drafts.discard("config");
        assert_eq!(
            drafts.sync("config", "changed preset", None),
            "changed preset"
        );
        assert!(drafts.check_save("config", "changed preset").is_ok());
    }
}
