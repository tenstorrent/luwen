use std::collections::HashMap;

use anyhow::Context as _;
use luwen_api::chip::spirom_tables::{self, fw_table_override::FwTableOverride};
use luwen_api::chip::Blackhole;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::Style;

type Chip<'a> = (usize, &'a Blackhole);
type Write<'a> = (usize, &'a Blackhole, HashMap<String, Value>, Vec<Diff>);
type FwView = (usize, HashMap<String, Value>, HashMap<String, Value>);
type FwRow = (String, String, String);
type ChipRows = (usize, Vec<FwRow>);

pub fn get(
    chips: &[Chip<'_>],
    table: Option<&crate::Table>,
    fmt: &crate::Fmt,
    delta: bool,
    fields: &[String],
) -> anyhow::Result<()> {
    match table {
        Some(crate::Table::ReadOnly) => get_simple(chips, "boardcfg", fmt, fields),
        Some(crate::Table::FlashInfo) => get_simple(chips, "flshinfo", fmt, fields),
        Some(crate::Table::FwTable) | None => get_fw_table(chips, fmt, delta, fields),
    }
}

/// Render a single-source table (boardcfg, flshinfo) as a two-column
/// `Field | Value`.
fn get_simple(
    chips: &[Chip<'_>],
    tag: &str,
    fmt: &crate::Fmt,
    fields: &[String],
) -> anyhow::Result<()> {
    let chip_maps: Vec<(usize, HashMap<String, Value>)> = chips
        .iter()
        .map(|(id, bh)| -> anyhow::Result<_> {
            let map = bh
                .decode_boot_fs_table(tag)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .with_context(|| format!("decoding {tag}"))?;
            Ok((*id, map))
        })
        .collect::<anyhow::Result<_>>()?;
    match fmt {
        crate::Fmt::Pretty => render_pretty(&chip_maps, fields),
        crate::Fmt::Json => render_json(&chip_maps, fields)?,
    }
    Ok(())
}

/// Render the fw-table view as a three-column `Field | Default | Override`,
/// with `Default` from `cmfwcfg` and `Override` from the active
/// `ccfgovr` bank. `delta` filters to rows whose override is set.
fn get_fw_table(
    chips: &[Chip<'_>],
    fmt: &crate::Fmt,
    delta: bool,
    fields: &[String],
) -> anyhow::Result<()> {
    let per_chip: Vec<FwView> = chips
        .iter()
        .map(|(id, bh)| -> anyhow::Result<_> {
            let cmfwcfg = bh
                .decode_boot_fs_table("cmfwcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("decoding cmfwcfg")?;
            let mut ovr = bh
                .ccfgovr_read()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("reading ccfgovr")?;
            prune_for_display(&mut ovr);
            Ok((*id, cmfwcfg, ovr))
        })
        .collect::<anyhow::Result<_>>()?;
    match fmt {
        crate::Fmt::Pretty => render_fw_table_pretty(&per_chip, delta, fields),
        crate::Fmt::Json => render_fw_table_json(&per_chip, delta, fields)?,
    }
    Ok(())
}

fn render_fw_table_pretty(per_chip: &[FwView], delta: bool, fields: &[String]) {
    let per_chip_rows: Vec<ChipRows> = per_chip
        .iter()
        .map(|(id, cmfwcfg, ovr)| (*id, fw_table_rows(cmfwcfg, ovr, delta, fields)))
        .collect();
    if per_chip_rows.iter().all(|(_, r)| r.is_empty()) {
        let msg = if !fields.is_empty() {
            "No matching fields."
        } else if delta {
            "No overrides set."
        } else {
            "No fields."
        };
        println!("{msg}");
        return;
    }

    // Group chips whose (default, override) rows are identical so their
    // columns can share a header. The multi-group case collapses to one
    // pair of columns labelled with a chip range.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, (_, rows)) in per_chip_rows.iter().enumerate() {
        if let Some(g) = groups.iter_mut().find(|g| per_chip_rows[g[0]].1 == *rows) {
            g.push(i);
        } else {
            groups.push(vec![i]);
        }
    }

    // Union of fields across all chips, preserving the per-chip ordering.
    let mut seen = std::collections::HashSet::new();
    let mut fields_in_order: Vec<String> = Vec::new();
    for (_, rows) in &per_chip_rows {
        for (f, _, _) in rows {
            if seen.insert(f.clone()) {
                fields_in_order.push(f.clone());
            }
        }
    }

    let single_group = groups.len() == 1;
    let mut builder = Builder::default();
    let mut header = vec!["Field".to_string()];
    for group in &groups {
        if single_group {
            header.push("Default".to_string());
            header.push("Override".to_string());
        } else {
            let label = compress_ids(group.iter().map(|&i| per_chip_rows[i].0));
            header.push(format!("Default ({label})"));
            header.push(format!("Override ({label})"));
        }
    }
    builder.push_record(header);
    for field in &fields_in_order {
        let mut row = vec![field.clone()];
        for group in &groups {
            let rep_rows = &per_chip_rows[group[0]].1;
            let (default, override_) = rep_rows
                .iter()
                .find(|(f, _, _)| f == field)
                .map(|(_, d, o)| (d.clone(), o.clone()))
                .unwrap_or_default();
            row.push(default);
            row.push(override_);
        }
        builder.push_record(row);
    }
    let mut tbl = builder.build();
    tbl.with(Style::modern_rounded());
    println!("{tbl}");
}

fn render_fw_table_json(per_chip: &[FwView], delta: bool, fields: &[String]) -> anyhow::Result<()> {
    let obj: serde_json::Map<String, Value> = per_chip
        .iter()
        .map(|(id, cmfwcfg, ovr)| {
            let rows = fw_table_rows(cmfwcfg, ovr, delta, fields);
            let entries: serde_json::Map<String, Value> = rows
                .into_iter()
                .map(|(field, default, override_)| {
                    let mut entry = serde_json::Map::new();
                    entry.insert("default".to_string(), parse_or_string(&default));
                    entry.insert("override".to_string(), parse_or_string(&override_));
                    (field, Value::Object(entry))
                })
                .collect();
            (id.to_string(), Value::Object(entries))
        })
        .collect();
    let out = if obj.len() == 1 {
        obj.into_iter().next().expect("len==1").1
    } else {
        Value::Object(obj)
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&out).context("rendering JSON")?
    );
    Ok(())
}

/// Build `(field, default, override)` rows for the fw-table view.
/// `delta` keeps only rows whose `override` is set.
fn fw_table_rows(
    cmfwcfg: &HashMap<String, Value>,
    ovr: &HashMap<String, Value>,
    delta: bool,
    fields: &[String],
) -> Vec<FwRow> {
    let mut cmfwcfg_rows = Vec::new();
    flatten(cmfwcfg, "", &mut cmfwcfg_rows);
    let cmfwcfg_lookup: HashMap<String, String> = cmfwcfg_rows.iter().cloned().collect();

    let mut ovr_rows = Vec::new();
    flatten(ovr, "", &mut ovr_rows);
    let ovr_lookup: HashMap<String, String> = ovr_rows.iter().cloned().collect();

    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (field, default) in &cmfwcfg_rows {
        let override_ = ovr_lookup.get(field).cloned().unwrap_or_default();
        if delta && override_.is_empty() {
            continue;
        }
        rows.push((field.clone(), default.clone(), override_));
    }
    // Surface override-only fields (shouldn't normally happen, but be safe).
    for (field, override_) in &ovr_rows {
        if !cmfwcfg_lookup.contains_key(field) {
            rows.push((field.clone(), String::new(), override_.clone()));
        }
    }
    if !fields.is_empty() {
        rows.retain(|(f, _, _)| {
            fields
                .iter()
                .any(|p| f == p || f.starts_with(&format!("{p}.")))
        });
    }
    rows
}

fn parse_or_string(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.to_string()))
    }
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
    if per_chip.iter().all(|(_, rows)| rows.is_empty()) {
        println!("No matching fields.");
        return;
    }

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
            header.push(compress_ids(group.iter().map(|&i| per_chip[i].0)));
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
                serde_json::to_string_pretty(map).context("rendering JSON")?
            );
        } else {
            let obj = filtered_obj(map, fields);
            println!(
                "{}",
                serde_json::to_string_pretty(&obj).context("rendering JSON")?
            );
        }
    } else {
        let outer: serde_json::Map<String, Value> = chip_maps
            .iter()
            .map(|(id, map)| -> anyhow::Result<(String, Value)> {
                let val = if fields.is_empty() {
                    serde_json::to_value(map).context("rendering JSON")?
                } else {
                    Value::Object(filtered_obj(map, fields))
                };
                Ok((id.to_string(), val))
            })
            .collect::<anyhow::Result<_>>()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&outer).context("rendering JSON")?
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
        print_single_chip_diff(chips[0]);
    } else {
        print_multi_chip_diff(chips);
    }
}

fn print_single_chip_diff((id, diffs): (usize, &[Diff])) {
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
}

fn print_multi_chip_diff(chips: &[(usize, &[Diff])]) {
    // Collect changed fields across chips, preserving first-seen order
    let mut seen = std::collections::HashSet::new();
    let mut fields: Vec<String> = Vec::new();
    for (_, diffs) in chips {
        for d in *diffs {
            if seen.insert(d.field.clone()) {
                fields.push(d.field.clone());
            }
        }
    }
    if !fields.is_empty() {
        print_multi_chip_table(chips, &fields);
    }
    println!("{}", multi_chip_summary(chips).join("\n"));
}

fn print_multi_chip_table(chips: &[(usize, &[Diff])], fields: &[String]) {
    // Per-chip old-value vectors, grouped by identical pattern
    let old_vals: Vec<Vec<String>> = chips
        .iter()
        .map(|(_, diffs)| {
            fields
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
        header.push(format!(
            "Old ({})",
            compress_ids(group.iter().map(|&i| chips[i].0))
        ));
    }
    header.push("New".to_string());
    builder.push_record(header);
    for (fi, field) in fields.iter().enumerate() {
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

fn multi_chip_summary(chips: &[(usize, &[Diff])]) -> Vec<String> {
    // Group chips by identical change count so equal counts collapse to a
    // single line ("N change(s) on chips X, Y")
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for (id, diffs) in chips {
        let n = diffs.len();
        if let Some(g) = groups.iter_mut().find(|(c, _)| *c == n) {
            g.1.push(*id);
        } else {
            groups.push((n, vec![*id]));
        }
    }
    groups
        .iter()
        .map(|(n, ids)| {
            let label = if ids.len() == 1 {
                format!("chip {}", ids[0])
            } else {
                format!("chips {}", compress_ids(ids.iter().copied()))
            };
            format!("{n} change(s) on {label}")
        })
        .collect()
}

fn push_diffs(path: &str, before: Option<&Value>, after: Option<&Value>, out: &mut Vec<Diff>) {
    match (before, after) {
        (Some(Value::Object(b)), Some(Value::Object(a))) => {
            let mut keys: std::collections::BTreeSet<&String> = b.keys().collect();
            keys.extend(a.keys());
            for key in keys {
                push_diffs(&format!("{path}.{key}"), b.get(key), a.get(key), out);
            }
        }
        (Some(Value::Object(b)), None) => {
            for (key, val) in b {
                push_diffs(&format!("{path}.{key}"), Some(val), None, out);
            }
        }
        (None, Some(Value::Object(a))) => {
            for (key, val) in a {
                push_diffs(&format!("{path}.{key}"), None, Some(val), out);
            }
        }
        _ => {
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
}

/// Render a list of chip IDs as compact contiguous ranges:
/// `[0,1,2,3] -> "0-3"`, `[0,1,3,4] -> "0-1, 3-4"`, `[5] -> "5"`.
fn compress_ids(ids: impl IntoIterator<Item = usize>) -> String {
    let mut sorted: Vec<usize> = ids.into_iter().collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut iter = sorted.into_iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let (mut start, mut end) = (first, first);
    let push = |parts: &mut Vec<String>, s: usize, e: usize| {
        parts.push(if s == e {
            s.to_string()
        } else {
            format!("{s}-{e}")
        });
    };
    for id in iter {
        if id == end + 1 {
            end = id;
        } else {
            push(&mut parts, start, end);
            start = id;
            end = id;
        }
    }
    push(&mut parts, start, end);
    parts.join(", ")
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
            let cmfwcfg = bh
                .decode_boot_fs_table("cmfwcfg")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("decoding cmfwcfg")?;
            let before_map = bh
                .ccfgovr_read()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("reading ccfgovr")?;
            let mut new_map = before_map.clone();
            let mut paths: Vec<&str> = Vec::new();
            for spec in &self.fields {
                let (path, raw) = spec
                    .split_once('=')
                    .with_context(|| format!("invalid spec {spec:?}: expected field=value"))?;
                // Look up the field's type from cmfwcfg (the FwTable schema
                // covers a superset of FwTableOverride and uses the same
                // types for matching fields, so it's a safe type source).
                let mut typed = get_value(&cmfwcfg, path)
                    .with_context(|| format!("unknown field path: {path}"))?
                    .clone();
                set_value(&mut typed, path, raw)?;
                insert_at_path(&mut new_map, path, typed);
                paths.push(path);
            }
            // Round-trip through FwTableOverride: any path not in the
            // override proto is silently dropped by serde.
            let round_tripped = override_round_trip(&new_map);
            for path in &paths {
                anyhow::ensure!(
                    get_value(&new_map, path) == get_value(&round_tripped, path),
                    "cannot override {path}",
                );
            }
            let mut diffs: Vec<Diff> = Vec::new();
            collect_diffs(&before_map, &new_map, &mut diffs);
            writes.push((*id, bh, new_map, diffs));
        }
        let chip_diffs: Vec<(usize, &[Diff])> = writes
            .iter()
            .map(|(id, _, _, diffs)| (*id, diffs.as_slice()))
            .collect();
        print_diff_table(&chip_diffs);
        if !self.dry_run {
            for (_, bh, map, _) in writes {
                bh.ccfgovr_write(map)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("writing ccfgovr")?;
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
            let before_map = bh
                .ccfgovr_read()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("reading ccfgovr")?;
            let new_map: HashMap<String, Value> = if self.fields.is_empty() {
                HashMap::new()
            } else {
                let mut m = before_map.clone();
                for path in &self.fields {
                    remove_at_path(&mut m, path);
                }
                m
            };
            let mut diffs: Vec<Diff> = Vec::new();
            collect_diffs(&before_map, &new_map, &mut diffs);
            writes.push((*id, bh, new_map, diffs));
        }
        let chip_diffs: Vec<(usize, &[Diff])> = writes
            .iter()
            .map(|(id, _, _, diffs)| (*id, diffs.as_slice()))
            .collect();
        print_diff_table(&chip_diffs);
        if !self.dry_run {
            for (_, bh, map, _) in writes {
                bh.ccfgovr_write(map)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("writing ccfgovr")?;
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

/// Insert `value` at `path`, creating intermediate `Value::Object`s as
/// needed and overwriting any non-object intermediates.
fn insert_at_path(map: &mut HashMap<String, Value>, path: &str, value: Value) {
    let (head, tail) = split(path);
    match tail {
        None => {
            map.insert(head.to_string(), value);
        }
        Some(rest) => {
            let entry = map
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if !matches!(entry, Value::Object(_)) {
                *entry = Value::Object(serde_json::Map::new());
            }
            let Value::Object(obj) = entry else {
                unreachable!()
            };
            insert_in_object(obj, rest, value);
        }
    }
}

fn insert_in_object(obj: &mut serde_json::Map<String, Value>, path: &str, value: Value) {
    let (head, tail) = split(path);
    match tail {
        None => {
            obj.insert(head.to_string(), value);
        }
        Some(rest) => {
            let entry = obj
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if !matches!(entry, Value::Object(_)) {
                *entry = Value::Object(serde_json::Map::new());
            }
            let Value::Object(child) = entry else {
                unreachable!()
            };
            insert_in_object(child, rest, value);
        }
    }
}

/// Remove the leaf at `path`. Empty intermediate `Value::Object`s left
/// behind are pruned.
fn remove_at_path(map: &mut HashMap<String, Value>, path: &str) {
    let (head, tail) = split(path);
    match tail {
        None => {
            map.remove(head);
        }
        Some(rest) => {
            let Some(Value::Object(child)) = map.get_mut(head) else {
                return;
            };
            remove_in_object(child, rest);
            if child.is_empty() {
                map.remove(head);
            }
        }
    }
}

fn remove_in_object(obj: &mut serde_json::Map<String, Value>, path: &str) {
    let (head, tail) = split(path);
    match tail {
        None => {
            obj.remove(head);
        }
        Some(rest) => {
            let Some(Value::Object(child)) = obj.get_mut(head) else {
                return;
            };
            remove_in_object(child, rest);
            if child.is_empty() {
                obj.remove(head);
            }
        }
    }
}

/// Serialize a `HashMap` through `FwTableOverride` and back. Fields not in
/// the override schema get silently dropped by serde, so this is the
/// allow-list filter used by `Set::run` to reject unsupported paths.
fn override_round_trip(map: &HashMap<String, Value>) -> HashMap<String, Value> {
    let ovr: FwTableOverride = spirom_tables::from_hash_map(map.clone());
    spirom_tables::to_hash_map(ovr)
}

/// Collect leaf-level diffs between two override maps. Keys present in one
/// and missing in the other map to `before=Some/after=None` (or vice versa)
/// and recurse into matching `Value::Object` pairs.
fn collect_diffs(
    before: &HashMap<String, Value>,
    after: &HashMap<String, Value>,
    out: &mut Vec<Diff>,
) {
    let mut keys: std::collections::BTreeSet<&String> = before.keys().collect();
    keys.extend(after.keys());
    for key in keys {
        push_diffs(key, before.get(key), after.get(key), out);
    }
}

/// Returns true if a value is the proto3 default (and would be omitted
/// from the encoded wire form).
fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => {
            n.as_u64() == Some(0) || n.as_i64() == Some(0) || n.as_f64() == Some(0.0)
        }
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty() || m.values().all(is_empty_value),
    }
}

/// Strip empty/default values from a `HashMap` so the override view shows
/// only fields that are actually overridden on the wire.
fn prune_for_display(map: &mut HashMap<String, Value>) {
    for v in map.values_mut() {
        if let Value::Object(obj) = v {
            prune_object(obj);
        }
    }
    map.retain(|_, v| !is_empty_value(v));
}

fn prune_object(obj: &mut serde_json::Map<String, Value>) {
    for v in obj.values_mut() {
        if let Value::Object(child) = v {
            prune_object(child);
        }
    }
    obj.retain(|_, v| !is_empty_value(v));
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
