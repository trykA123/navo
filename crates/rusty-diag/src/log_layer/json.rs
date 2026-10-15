use serde_json::Value;

pub(super) struct Line<'a> {
    pub(super) millis: u64,
    pub(super) level: &'a str,
    pub(super) target: &'a str,
    pub(super) msg: &'a str,
    pub(super) fields: &'a [(String, Value)],
}

impl Line<'_> {
    pub(super) fn render(&self) -> String {
        let mut out = format!(
            "{{\"t\":{},\"level\":{},\"target\":{},\"msg\":{},\"fields\":{{",
            self.millis,
            quote(self.level),
            quote(self.target),
            quote(self.msg),
        );
        for (index, (name, value)) in self.fields.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&quote(name));
            out.push(':');
            out.push_str(&value.to_string());
        }
        out.push_str("}}");
        out
    }
}

fn quote(text: &str) -> String {
    Value::from(text).to_string()
}
