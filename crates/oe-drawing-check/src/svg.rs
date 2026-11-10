//! The data-entity identities of an SVG document and its data-member identities with their roots.
use anyhow::{Context, Result};
use quick_xml::events::Event;
use std::collections::BTreeSet;

pub struct Inventory {
    pub entities: BTreeSet<String>,
    /// (data-entity, data-member) of every element that names a native member; the root is empty when the
    /// element carries no data-entity.
    pub members: BTreeSet<(String, String)>,
}

pub fn inventory(bytes: &[u8]) -> Result<Inventory> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut entities = BTreeSet::new();
    let mut members = BTreeSet::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).context("svg_parse")? {
            Event::Start(e) | Event::Empty(e) => {
                let (mut root, mut member) = (String::new(), None);
                for attribute in e.attributes() {
                    let attribute = attribute.context("svg_attribute")?;
                    if attribute.key.as_ref() == b"data-entity" {
                        let value = attribute.unescape_value().context("svg_attribute_value")?;
                        if !value.is_empty() {
                            root = value.to_string();
                            entities.insert(value.into_owned());
                        }
                    }
                    if attribute.key.as_ref() == b"data-member" {
                        let value = attribute.unescape_value().context("svg_attribute_value")?;
                        member = Some(value.into_owned());
                    }
                }
                if let Some(member) = member {
                    members.insert((root, member));
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(Inventory { entities, members })
}

#[cfg(test)]
mod tests {
    use super::inventory;

    /// Roots and member identities are read with the root each group is drawn for; a member identity
    /// without a root keeps an empty root, so that the graph check refuses it.
    #[test]
    fn member_identities_are_read_with_their_roots() {
        let svg = br#"<svg><g data-entity="A1"><path d="M0 0Z"/></g><g data-entity="A1" data-member="A1/B2/C3"><path d="M0 0Z"/></g><g data-member="A1/B2"></g><g data-entity="D4" data-member="D4/E5"/></svg>"#;
        let found = inventory(svg).unwrap();
        assert_eq!(found.entities.into_iter().collect::<Vec<_>>(), ["A1", "D4"]);
        assert_eq!(
            found.members.into_iter().collect::<Vec<_>>(),
            [
                (String::new(), "A1/B2".to_owned()),
                ("A1".to_owned(), "A1/B2/C3".to_owned()),
                ("D4".to_owned(), "D4/E5".to_owned()),
            ]
        );
    }
}
