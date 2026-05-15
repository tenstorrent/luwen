use std::collections::HashMap;

use anyhow::Context as _;
use luwen_api::chip::Blackhole;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::Style;

type Chip<'a> = (usize, &'a Blackhole);
type Write<'a> = (usize, &'a Blackhole, HashMap<String, Value>, Vec<Diff>);

pub fn get(
    chips: &[Chip<'_>],
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
        let chip_maps: Vec<(usize, HashMap<String, Value>)> = chips
            .iter()
            .map(|(id, bh)| {
                let map = bh
                    .decode_boot_fs_table(tag)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .with_context(|| format!("failed to decode table {tag:?}"))?;
                Ok((*id, map))
            })
            .collect::<anyhow::Result<_>>()?;
        match fmt {
            crate::Fmt::Pretty => render_pretty(&chip_maps, fields),
            crate::Fmt::Json => render_json(&chip_maps, fields)?,
        }
    }
    Ok(())
}

fn render_pretty(chip_maps: &[(usize, HashMap<String, Value>)], fields: &[String]) {
    let per_chip: Vec<(usize, Vec<(String, String)>)> = chip_maps
        .iter()
        .map(|(id, map)| {
            let mut rows = Vec::new();
            flatten(map, "", &mut rows);
            if !fields.is_empty() {
                rows.retain(|(k, _)| {
                    fields
                        .iter()
                        .any(|f| k == f || k.starts_with(&format!("{f}.")))
                });
            }
            (*id, rows)
        })
        .collect();

    let Some((_, first_rows)) = per_chip.first() else {
        return;
    };

    let multi = per_chip.len() > 1 && per_chip[1..].iter().any(|(_, rows)| rows != first_rows);

    let mut builder = Builder::default();
    if multi {
        // Build per-chip value vectors in field order, then group identical patterns.
        let field_names: Vec<&str> = first_rows.iter().map(|(k, _)| k.as_str()).collect();
        let chip_vals: Vec<Vec<String>> = per_chip
            .iter()
            .map(|(_, rows)| {
                field_names
                    .iter()
                    .map(|name| {
                        rows.iter()
                            .find(|(k, _)| k == name)
                            .map_or(String::new(), |(_, v)| v.clone())
                    })
                    .collect()
            })
            .collect();
        let groups = group_by_pattern(&chip_vals);

        let mut header = vec!["Field".to_string()];
        for group in &groups {
            let ids = group
                .iter()
                .map(|&i| per_chip[i].0.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            header.push(ids);
        }
        builder.push_record(header);
        for (fi, (name, _)) in first_rows.iter().enumerate() {
            let mut row: Vec<String> = vec![name.clone()];
            for group in &groups {
                row.push(chip_vals[group[0]][fi].clone());
            }
            builder.push_record(row);
        }
    } else {
        builder.push_record(["Field", "Value"]);
        for (k, v) in first_rows {
            builder.push_record([k.as_str(), v.as_str()]);
        }
    }
    let mut tbl = builder.build();
    tbl.with(Style::modern_rounded());
    println!("{tbl}");
}

fn render_json(
    chip_maps: &[(usize, HashMap<String, Value>)],
    fields: &[String],
) -> anyhow::Result<()> {
    if chip_maps.len() == 1 {
        let (_, map) = &chip_maps[0];
        if fields.is_empty() {
            println!(
                "{}",
                serde_json::to_string_pretty(map).context("failed to serialize table to JSON")?
            );
        } else {
            let obj = filtered_obj(map, fields);
            println!(
                "{}",
                serde_json::to_string_pretty(&obj).context("failed to serialize fields to JSON")?
            );
        }
    } else {
        let outer: serde_json::Map<String, Value> = chip_maps
            .iter()
            .map(|(id, map)| -> anyhow::Result<(String, Value)> {
                let val = if fields.is_empty() {
                    serde_json::to_value(map).context("failed to serialize table")?
                } else {
                    Value::Object(filtered_obj(map, fields))
                };
                Ok((id.to_string(), val))
            })
            .collect::<anyhow::Result<_>>()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&outer).context("failed to serialize tables to JSON")?
        );
    }
    Ok(())
}

fn filtered_obj(map: &HashMap<String, Value>, fields: &[String]) -> serde_json::Map<String, Value> {
    let mut rows = Vec::new();
    flatten(map, "", &mut rows);
    rows.retain(|(k, _)| {
        fields
            .iter()
            .any(|f| k == f || k.starts_with(&format!("{f}.")))
    });
    rows.into_iter()
        .map(|(k, v)| {
            let val = serde_json::from_str(&v).unwrap_or(Value::String(v));
            (k, val)
        })
        .collect()
}

struct Diff {
    field: String,
    old: String,
    new: String,
}

fn print_diff_table(chips: &[(usize, &[Diff])]) {
    if chips.len() == 1 {
        let (id, diffs) = chips[0];
        if !diffs.is_empty() {
            let old_header = format!("Old ({id})");
            let mut builder = Builder::default();
            builder.push_record(["Field", &old_header, "New"]);
            for Diff { field, old, new } in diffs {
                builder.push_record([field.as_str(), old.as_str(), new.as_str()]);
            }
            let mut tbl = builder.build();
            tbl.with(Style::modern_rounded());
            println!("{tbl}");
        }
        println!("{} change(s) on chip {id}", diffs.len());
    } else {
        // Collect all changed fields across chips, preserving first-seen order.
        let mut seen = std::collections::HashSet::new();
        let mut all_fields: Vec<String> = Vec::new();
        for (_, diffs) in chips {
            for d in *diffs {
                if seen.insert(d.field.clone()) {
                    all_fields.push(d.field.clone());
                }
            }
        }
        if !all_fields.is_empty() {
            // Build per-chip old-value vectors across all changed fields, then group.
            let old_vals: Vec<Vec<String>> = chips
                .iter()
                .map(|(_, diffs)| {
                    all_fields
                        .iter()
                        .map(|f| {
                            diffs
                                .iter()
                                .find(|d| &d.field == f)
                                .map_or_else(|| "\u{2014}".to_string(), |d| d.old.clone())
                        })
                        .collect()
                })
                .collect();
            let groups = group_by_pattern(&old_vals);

            let mut builder = Builder::default();
            let mut header = vec!["Field".to_string()];
            for group in &groups {
                let ids = group
                    .iter()
                    .map(|&i| chips[i].0.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                header.push(format!("Old ({ids})"));
            }
            header.push("New".to_string());
            builder.push_record(header);
            for (fi, field) in all_fields.iter().enumerate() {
                let new_val = chips
                    .iter()
                    .find_map(|(_, diffs)| {
                        diffs
                            .iter()
                            .find(|d| &d.field == field)
                            .map(|d| d.new.clone())
                    })
                    .unwrap_or_default();
                let mut row = vec![field.clone()];
                for group in &groups {
                    row.push(old_vals[group[0]][fi].clone());
                }
                row.push(new_val);
                builder.push_record(row);
            }
            let mut tbl = builder.build();
            tbl.with(Style::modern_rounded());
            println!("{tbl}");
        }
        // Group chips by identical change count so "0 change(s) on chip 0, 0 change(s) on chip 1"
        // collapses to "0 change(s) on chips 0, 1".
        let mut count_groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for (id, diffs) in chips {
            let n = diffs.len();
            if let Some(g) = count_groups.iter_mut().find(|(c, _)| *c == n) {
                g.1.push(*id);
            } else {
                count_groups.push((n, vec![*id]));
            }
        }
        let summary: Vec<String> = count_groups
            .iter()
            .map(|(n, ids)| {
                let label = if ids.len() == 1 {
                    format!("chip {}", ids[0])
                } else {
                    format!(
                        "chips {}",
                        ids.iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!("{n} change(s) on {label}")
            })
            .collect();
        println!("{}", summary.join("\n"));
    }
}

fn push_diffs(path: &str, before: Option<&Value>, after: Option<&Value>, out: &mut Vec<Diff>) {
    if let (Some(Value::Object(b_map)), Some(Value::Object(a_map))) = (before, after) {
        let mut keys: std::collections::BTreeSet<&String> = b_map.keys().collect();
        keys.extend(a_map.keys());
        for key in keys {
            push_diffs(
                &format!("{path}.{key}"),
                b_map.get(key),
                a_map.get(key),
                out,
            );
        }
    } else {
        let old = fmt_val(before);
        let new = fmt_val(after);
        if old != new {
            out.push(Diff {
                field: path.to_string(),
                old,
                new,
            });
        }
    }
}

fn group_by_pattern(patterns: &[Vec<String>]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, vals) in patterns.iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|g| &patterns[g[0]] == vals) {
            group.push(i);
        } else {
            groups.push(vec![i]);
        }
    }
    groups
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
    chips: &'a [Chip<'a>],
    fields: Vec<String>,
    dry_run: bool,
}

impl<'a> Set<'a> {
    pub fn new(chips: &'a [Chip<'a>]) -> Self {
        Self {
            chips,
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
        let mut writes: Vec<Write<'_>> = Vec::new();
        for (id, bh) in self.chips {
            let mut map = bh
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
                push_diffs(path, before.as_ref(), after.as_ref(), &mut diffs);
            }
            writes.push((*id, bh, map, diffs));
        }
        let chip_diffs: Vec<(usize, &[Diff])> = writes
            .iter()
            .map(|(id, _, _, diffs)| (*id, diffs.as_slice()))
            .collect();
        print_diff_table(&chip_diffs);
        if !self.dry_run {
            for (_, bh, map, _) in writes {
                bh.encode_and_write_boot_fs_table(map, "cmfwcfg")
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("failed to write cmfwcfg")?;
            }
        }
        Ok(())
    }
}

pub struct Reset<'a> {
    chips: &'a [Chip<'a>],
    fields: Vec<String>,
    dry_run: bool,
}

impl<'a> Reset<'a> {
    pub fn new(chips: &'a [Chip<'a>]) -> Self {
        Self {
            chips,
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
        let mut writes: Vec<Write<'_>> = Vec::new();
        for (id, bh) in self.chips {
            let orig = bh
                .decode_boot_fs_table("origcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("failed to decode origcfg")?;
            let mut current = bh
                .decode_boot_fs_table("cmfwcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("failed to decode cmfwcfg")?;
            let mut diffs: Vec<Diff> = Vec::new();
            if self.fields.is_empty() {
                for (key, val) in &orig {
                    let before = current.get(key);
                    if before != Some(val) {
                        push_diffs(key, before, Some(val), &mut diffs);
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
                        push_diffs(path, before.as_ref(), Some(&orig_val), &mut diffs);
                        put_field(&mut current, path, orig_val)?;
                    }
                }
            }
            writes.push((*id, bh, current, diffs));
        }
        let chip_diffs: Vec<(usize, &[Diff])> = writes
            .iter()
            .map(|(id, _, _, diffs)| (*id, diffs.as_slice()))
            .collect();
        print_diff_table(&chip_diffs);
        if !self.dry_run {
            for (_, bh, map, _) in writes {
                bh.encode_and_write_boot_fs_table(map, "cmfwcfg")
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("failed to write cmfwcfg")?;
            }
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
