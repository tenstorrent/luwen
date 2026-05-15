use std::collections::HashMap;

use anyhow::Context as _;
use luwen_api::chip::Blackhole;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::Style;

pub fn get(
    bh: &Blackhole,
    table: Option<&crate::Table>,
    fmt: &crate::Fmt,
    fields: &[String],
) -> anyhow::Result<()> {
    let tags: &[&str] = match table {
        None | Some(crate::Table::FwTable) => &["cmfwcfg"],
        Some(crate::Table::ReadOnly) => &["boardcfg"],
        Some(crate::Table::FlashInfo) => &["flshinfo"],
    };
    for tag in tags {
        let map = bh
            .decode_boot_fs_table(tag)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("failed to decode table {tag:?}"))?;
        let mut rows: Vec<(String, String)> = Vec::new();
        flatten(&map, "", &mut rows);
        if !fields.is_empty() {
            rows.retain(|(k, _)| {
                fields
                    .iter()
                    .any(|f| k == f || k.starts_with(&format!("{f}.")))
            });
        }
        match fmt {
            crate::Fmt::Pretty => {
                let mut builder = Builder::default();
                builder.push_record(["Field", "Value"]);
                for (k, v) in &rows {
                    builder.push_record([k.as_str(), v.as_str()]);
                }
                let mut tbl = builder.build();
                tbl.with(Style::modern_rounded());
                println!("{tbl}");
            }
            crate::Fmt::Json => {
                if fields.is_empty() {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&map)
                            .context("failed to serialize table to JSON")?
                    );
                } else {
                    let obj: serde_json::Map<String, Value> = rows
                        .into_iter()
                        .map(|(k, v)| {
                            let val: Value = serde_json::from_str(&v).unwrap_or(Value::String(v));
                            (k, val)
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&obj)
                            .context("failed to serialize fields to JSON")?
                    );
                }
            }
        }
    }
    Ok(())
}

struct Diff {
    field: String,
    old: String,
    new: String,
}

fn print_diff_table(diffs: &[Diff]) {
    if !diffs.is_empty() {
        let mut builder = Builder::default();
        builder.push_record(["Field", "Old", "New"]);
        for Diff { field, old, new } in diffs {
            builder.push_record([field.as_str(), old.as_str(), new.as_str()]);
        }
        let mut tbl = builder.build();
        tbl.with(Style::modern_rounded());
        println!("{tbl}");
    }
    println!("{} change(s)", diffs.len());
}

fn flatten(map: &HashMap<String, Value>, prefix: &str, out: &mut Vec<(String, String)>) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    for key in keys {
        let val = &map[key];
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        push_value(&path, val, out);
    }
}

fn push_value(path: &str, val: &Value, out: &mut Vec<(String, String)>) {
    match val {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                push_value(&format!("{path}.{key}"), &map[key], out);
            }
        }
        _ => out.push((path.to_string(), val.to_string())),
    }
}

pub struct Set<'a> {
    bh: &'a Blackhole,
    fields: Vec<String>,
    dry_run: bool,
}

impl<'a> Set<'a> {
    pub fn new(bh: &'a Blackhole) -> Self {
        Self {
            bh,
            fields: Vec::new(),
            dry_run: false,
        }
    }

    pub fn field(mut self, f: impl Into<String>) -> Self {
        self.fields.push(f.into());
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn run(self) -> anyhow::Result<()> {
        let mut map = self
            .bh
            .decode_boot_fs_table("cmfwcfg")
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("failed to decode cmfwcfg")?;
        let mut diffs: Vec<Diff> = Vec::new();
        for spec in &self.fields {
            let (path, raw) = spec
                .split_once('=')
                .with_context(|| format!("invalid spec {spec:?}: expected field=value"))?;
            let before = get_value(&map, path).cloned();
            patch_field(&mut map, path, raw)?;
            let after = get_value(&map, path).cloned();
            if before != after {
                diffs.push(Diff {
                    field: path.to_string(),
                    old: fmt_val(before.as_ref()),
                    new: fmt_val(after.as_ref()),
                });
            }
        }
        print_diff_table(&diffs);
        if !self.dry_run {
            self.bh
                .encode_and_write_boot_fs_table(map, "cmfwcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("failed to write cmfwcfg")?;
        }
        Ok(())
    }
}

pub struct Reset<'a> {
    bh: &'a Blackhole,
    fields: Vec<String>,
    dry_run: bool,
}

impl<'a> Reset<'a> {
    pub fn new(bh: &'a Blackhole) -> Self {
        Self {
            bh,
            fields: Vec::new(),
            dry_run: false,
        }
    }

    pub fn field(mut self, f: impl Into<String>) -> Self {
        self.fields.push(f.into());
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn run(self) -> anyhow::Result<()> {
        let orig = self
            .bh
            .decode_boot_fs_table("origcfg")
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("failed to decode origcfg")?;
        let mut current = self
            .bh
            .decode_boot_fs_table("cmfwcfg")
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("failed to decode cmfwcfg")?;
        let mut diffs: Vec<Diff> = Vec::new();
        if self.fields.is_empty() {
            for (key, val) in &orig {
                let before = current.get(key).cloned();
                if before.as_ref() != Some(val) {
                    diffs.push(Diff {
                        field: key.clone(),
                        old: fmt_val(before.as_ref()),
                        new: val.to_string(),
                    });
                    current.insert(key.clone(), val.clone());
                }
            }
        } else {
            for path in &self.fields {
                let orig_val = get_value(&orig, path)
                    .with_context(|| format!("unknown field path: {path}"))?
                    .clone();
                let before = get_value(&current, path).cloned();
                if before.as_ref() != Some(&orig_val) {
                    diffs.push(Diff {
                        field: path.clone(),
                        old: fmt_val(before.as_ref()),
                        new: orig_val.to_string(),
                    });
                    put_field(&mut current, path, orig_val)?;
                }
            }
        }
        print_diff_table(&diffs);
        if !self.dry_run {
            self.bh
                .encode_and_write_boot_fs_table(current, "cmfwcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("failed to write cmfwcfg")?;
        }
        Ok(())
    }
}

fn fmt_val(v: Option<&Value>) -> String {
    v.map(Value::to_string).unwrap_or_default()
}

fn get_value<'a>(map: &'a HashMap<String, Value>, path: &str) -> Option<&'a Value> {
    let (head, tail) = split(path);
    let val = map.get(head)?;
    match tail {
        None => Some(val),
        Some(rest) => get_nested(val, rest),
    }
}

fn get_nested<'a>(val: &'a Value, path: &str) -> Option<&'a Value> {
    let (head, tail) = split(path);
    let Value::Object(map) = val else {
        return None;
    };
    let val = map.get(head)?;
    match tail {
        None => Some(val),
        Some(rest) => get_nested(val, rest),
    }
}

fn patch_field(map: &mut HashMap<String, Value>, path: &str, raw: &str) -> anyhow::Result<()> {
    let (head, tail) = split(path);
    let val = map
        .get_mut(head)
        .with_context(|| format!("unknown field path: {path}"))?;
    match tail {
        None => set_value(val, path, raw),
        Some(rest) => patch_nested(val, path, rest, raw),
    }
}

fn patch_nested(val: &mut Value, full: &str, rest: &str, raw: &str) -> anyhow::Result<()> {
    let (head, tail) = split(rest);
    let Value::Object(map) = val else {
        anyhow::bail!("unknown field path: {full}");
    };
    let val = map
        .get_mut(head)
        .with_context(|| format!("unknown field path: {full}"))?;
    match tail {
        None => set_value(val, full, raw),
        Some(rest) => patch_nested(val, full, rest, raw),
    }
}

fn put_field(map: &mut HashMap<String, Value>, path: &str, new: Value) -> anyhow::Result<()> {
    let (head, tail) = split(path);
    match tail {
        None => {
            map.insert(head.to_string(), new);
            Ok(())
        }
        Some(rest) => {
            let val = map
                .get_mut(head)
                .with_context(|| format!("unknown field path: {path}"))?;
            put_nested(val, path, rest, new)
        }
    }
}

fn put_nested(val: &mut Value, full: &str, rest: &str, new: Value) -> anyhow::Result<()> {
    let (head, tail) = split(rest);
    let Value::Object(map) = val else {
        anyhow::bail!("unknown field path: {full}");
    };
    match tail {
        None => {
            map.insert(head.to_string(), new);
            Ok(())
        }
        Some(rest) => {
            let val = map
                .get_mut(head)
                .with_context(|| format!("unknown field path: {full}"))?;
            put_nested(val, full, rest, new)
        }
    }
}

fn set_value(val: &mut Value, path: &str, raw: &str) -> anyhow::Result<()> {
    *val = match val {
        Value::Number(_) => serde_json::from_str::<serde_json::Number>(raw)
            .map(Value::Number)
            .with_context(|| format!("cannot parse {raw:?} for field {path}"))?,
        Value::Bool(_) => raw
            .parse::<bool>()
            .map(Value::Bool)
            .with_context(|| format!("cannot parse {raw:?} for field {path}"))?,
        Value::String(_) => Value::String(raw.to_string()),
        _ => anyhow::bail!("unknown field path: {path}"),
    };
    Ok(())
}

fn split(path: &str) -> (&str, Option<&str>) {
    match path.find('.') {
        Some(i) => (&path[..i], Some(&path[i + 1..])),
        None => (path, None),
    }
}
