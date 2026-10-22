//! A source lexical observation of the SAME XML events, not another parser.
//! RDFLib 7.6.0 SAX literal_element_* behavior (BSD-3-Clause), see
//! docs/RDFLIB-SNAPSHOT-LICENSE.md in the containing workspace.
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct Name {
    pub namespace: String,
    pub local: String,
    pub prefix: String,
}
#[derive(Default)]
pub(crate) struct SourceXmlWriter {
    text: String,
    stack: Vec<(String, HashMap<String, String>)>,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn quote(value: &str) -> String {
    let value = escape(value)
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;");
    if value.contains('"') && !value.contains('\'') {
        format!("'{value}'")
    } else {
        format!("\"{}\"", value.replace('"', "&quot;"))
    }
}
impl SourceXmlWriter {
    pub(crate) fn start(&mut self, name: Name, attributes: Vec<(Name, String)>) {
        let mut declared = self.stack.last().map(|v| v.1.clone()).unwrap_or_else(|| {
            HashMap::from([("http://www.w3.org/XML/1998/namespace".into(), "xml".into())])
        });
        let tag = if name.prefix.is_empty() {
            name.local
        } else {
            format!("{}:{}", name.prefix, name.local)
        };
        self.text.push('<');
        self.text.push_str(&tag);
        if !name.namespace.is_empty() && !declared.contains_key(&name.namespace) {
            declared.insert(name.namespace.clone(), name.prefix.clone());
            let declaration = if name.prefix.is_empty() {
                "xmlns".into()
            } else {
                format!("xmlns:{}", name.prefix)
            };
            self.text
                .push_str(&format!(" {declaration}=\"{}\"", name.namespace));
        }
        for (a, value) in attributes {
            let key = if a.namespace.is_empty() {
                a.local
            } else {
                let prefix = declared.entry(a.namespace).or_insert(a.prefix);
                format!("{prefix}:{}", a.local)
            };
            self.text.push_str(&format!(" {key}={}", quote(&value)));
        }
        self.text.push('>');
        self.stack.push((tag, declared));
    }
    pub(crate) fn text(&mut self, value: &str) {
        self.text.push_str(&escape(value));
    }
    pub(crate) fn end(&mut self) {
        let (tag, _) = self.stack.pop().expect("source XML event nesting");
        self.text.push_str(&format!("</{tag}>"));
    }
    pub(crate) fn finish(self) -> String {
        self.text
    }
}
