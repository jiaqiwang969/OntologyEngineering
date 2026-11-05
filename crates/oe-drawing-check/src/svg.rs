//! Element names, attribute names and data-entity identities of an SVG document.
use anyhow::{Context, Result};
use quick_xml::events::Event;
use std::collections::BTreeSet;

pub struct Inventory {
    pub entities: BTreeSet<String>,
}

pub fn inventory(bytes: &[u8]) -> Result<Inventory> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut entities = BTreeSet::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).context("svg_parse")? {
            Event::Start(e) | Event::Empty(e) => {
                for attribute in e.attributes() {
                    let attribute = attribute.context("svg_attribute")?;
                    if attribute.key.as_ref() == b"data-entity" {
                        let value = attribute.unescape_value().context("svg_attribute_value")?;
                        if !value.is_empty() {
                            entities.insert(value.into_owned());
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(Inventory { entities })
}
