use std::collections::{HashMap, HashSet};

use crate::config::sections::SrpConfig;

use super::union_find::UnionFind;
use super::{MethodFieldData, ResponsibilityCluster, SrpWarning, StructInfo};

/// Build SRP warnings for structs that exceed the smell threshold.
/// Operation: groups methods by parent type, computes LCOM4 and composite
/// score per struct via closures (filter_map), no own calls.
pub fn build_struct_warnings(
    structs: &[StructInfo],
    methods: &[MethodFieldData],
    config: &SrpConfig,
) -> Vec<SrpWarning> {
    // Group methods by parent type
    let mut methods_by_type: HashMap<&str, Vec<&MethodFieldData>> = HashMap::new();
    for m in methods {
        methods_by_type.entry(&m.parent_type).or_default().push(m);
    }

    structs
        .iter()
        .filter_map(|s| {
            let type_methods = methods_by_type.get(s.name.as_str());
            let method_list: Vec<&MethodFieldData> =
                type_methods.map(|v| v.to_vec()).unwrap_or_default();

            // Skip structs with fewer than 2 instance methods (LCOM4 undefined)
            if method_list.len() < 2 {
                return None;
            }

            let field_idx = build_field_method_index(&method_list, &s.fields);
            let (lcom4, clusters) = compute_lcom4(&method_list, &s.fields, &field_idx);
            let fan_out = compute_fan_out(&method_list);
            let composite =
                compute_composite_score(lcom4, s.fields.len(), method_list.len(), fan_out, config);

            if composite >= config.smell_threshold {
                Some(SrpWarning {
                    struct_name: s.name.clone(),
                    file: s.file.clone(),
                    line: s.line,
                    lcom4,
                    field_count: s.fields.len(),
                    method_count: method_list.len(),
                    fan_out,
                    composite_score: composite,
                    clusters,
                    suppressed: false,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Build the field-to-methods index, resolving self-method calls one level deep.
/// Operation: iterates methods, expands field accesses from self-call targets.
pub(crate) fn build_field_method_index<'a>(
    methods: &[&'a MethodFieldData],
    struct_fields: &'a [String],
) -> HashMap<&'a str, Vec<usize>> {
    let direct_fields: HashMap<&str, &HashSet<String>> = methods
        .iter()
        .map(|m| (m.method_name.as_str(), &m.field_accesses))
        .collect();
    // O(1) membership check; previously an O(N) linear scan inside
    // nested loops over methods × field accesses.
    let struct_field_set: HashSet<&str> = struct_fields.iter().map(String::as_str).collect();

    let mut field_to_methods: HashMap<&str, Vec<usize>> = HashMap::new();
    methods.iter().enumerate().for_each(|(i, m)| {
        // HashSet dedupes fields that show up in both the direct access
        // set and via one-or-more self-method calls — avoids pushing
        // the same method index multiple times for the same field.
        let mut fields_to_add: HashSet<&str> = if m.is_constructor {
            struct_fields.iter().map(String::as_str).collect()
        } else {
            m.field_accesses
                .iter()
                .map(String::as_str)
                .filter(|f| struct_field_set.contains(f))
                .collect()
        };
        // Resolve self-method calls: add callee's direct field accesses
        m.self_method_calls.iter().for_each(|callee| {
            if let Some(callee_fields) = direct_fields.get(callee.as_str()) {
                callee_fields
                    .iter()
                    .map(String::as_str)
                    .filter(|f| struct_field_set.contains(f))
                    .for_each(|f| {
                        fields_to_add.insert(f);
                    });
            }
        });
        fields_to_add.iter().for_each(|&field| {
            field_to_methods.entry(field).or_default().push(i);
        });
    });
    field_to_methods
}

/// Compute LCOM4: number of connected components in the method-field graph.
/// Operation: Union-Find on method indices connected by shared field accesses.
/// Uses closures to wrap UnionFind calls for IOSP lenient-mode compliance.
pub(crate) fn compute_lcom4(
    methods: &[&MethodFieldData],
    struct_fields: &[String],
    field_to_methods: &HashMap<&str, Vec<usize>>,
) -> (usize, Vec<ResponsibilityCluster>) {
    let n = methods.len();
    if n == 0 {
        return (0, vec![]);
    }

    let _ = struct_fields; // not needed directly; membership already honoured via field_to_methods
    let make_uf = |size| UnionFind::new(size);
    let mut uf = make_uf(n);
    let unite = |uf: &mut UnionFind, a, b| uf.union(a, b);
    let components = |uf: &mut UnionFind| uf.component_members();
    // Union methods that share fields
    field_to_methods.values().for_each(|indices| {
        indices.windows(2).for_each(|w| unite(&mut uf, w[0], w[1]));
    });
    // Invert `field_to_methods` into a per-method field set. This
    // captures the SAME expanded view (direct accesses PLUS fields
    // reached via one-level `self_method_calls`) used above for
    // unioning — so a method that only connects to the cluster via a
    // self-call still contributes its reached fields to the cluster.
    let method_to_fields = invert_field_to_methods(field_to_methods, n);
    // Build clusters from connected components. HashMap/HashSet
    // iteration is non-deterministic, so sort `methods` and `fields`
    // lexicographically inside each cluster and sort the clusters
    // themselves by their sorted method lists. Report/snapshot output
    // is stable across runs and platforms.
    let component_members = components(&mut uf);
    let mut clusters: Vec<ResponsibilityCluster> = component_members
        .values()
        .map(|member_indices| build_cluster(member_indices, methods, &method_to_fields))
        .collect();
    clusters.sort_by(|a, b| a.methods.cmp(&b.methods).then(a.fields.cmp(&b.fields)));
    (component_members.len(), clusters)
}

/// Invert `field_to_methods` into `method_index → list-of-fields`. A
/// single method can touch many fields (directly or via one-level
/// self-call expansion); the returned view matches what Union-Find
/// saw when forming clusters.
/// Operation: iterator-based grouping, no own calls.
fn invert_field_to_methods<'a>(
    field_to_methods: &HashMap<&'a str, Vec<usize>>,
    method_count: usize,
) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&'a str>> = vec![Vec::new(); method_count];
    field_to_methods.iter().for_each(|(field, indices)| {
        indices.iter().for_each(|&i| out[i].push(field));
    });
    out
}

/// Project one connected component into a sorted `ResponsibilityCluster`.
/// Fields come from the pre-expanded `method_to_fields` so cluster
/// membership and cluster contents stay in lock-step.
/// Operation: projection + sort, no own calls.
fn build_cluster(
    member_indices: &[usize],
    methods: &[&MethodFieldData],
    method_to_fields: &[Vec<&str>],
) -> ResponsibilityCluster {
    let mut cluster_methods: Vec<String> = member_indices
        .iter()
        .map(|&i| methods[i].method_name.clone())
        .collect();
    cluster_methods.sort();
    let cluster_fields_set: HashSet<String> = member_indices
        .iter()
        .flat_map(|&i| method_to_fields[i].iter().map(|f| (*f).to_string()))
        .collect();
    let mut cluster_fields: Vec<String> = cluster_fields_set.into_iter().collect();
    cluster_fields.sort();
    ResponsibilityCluster {
        methods: cluster_methods,
        fields: cluster_fields,
    }
}

/// Compute total fan-out: distinct external call targets across all methods.
/// Operation: set union.
pub(crate) fn compute_fan_out(methods: &[&MethodFieldData]) -> usize {
    let all_targets: HashSet<&str> = methods
        .iter()
        .flat_map(|m| m.call_targets.iter().map(|s| s.as_str()))
        .collect();
    all_targets.len()
}

/// Compute the composite SRP smell score from sub-metrics.
/// Operation: arithmetic normalization + weighted sum.
pub(crate) fn compute_composite_score(
    lcom4: usize,
    field_count: usize,
    method_count: usize,
    fan_out: usize,
    config: &SrpConfig,
) -> f64 {
    // Normalize LCOM4: 0 when <=1 (cohesive), scales linearly above threshold
    let lcom4_norm = if lcom4 <= 1 {
        0.0
    } else {
        let excess = (lcom4 - 1) as f64;
        let threshold_range = (config.lcom4_threshold.max(1) - 1) as f64;
        if threshold_range > 0.0 {
            (excess / threshold_range).min(1.0)
        } else {
            1.0
        }
    };

    // Guard each denominator: a misconfigured `0` would otherwise
    // produce inf / NaN and cascade into the composite score. Treat
    // any non-zero count as maximally penalised when the cap is 0.
    let field_norm = normalised_ratio(field_count, config.max_fields);
    let method_norm = normalised_ratio(method_count, config.max_methods);
    let fan_out_norm = normalised_ratio(fan_out, config.max_fan_out);

    let [w_lcom4, w_fields, w_methods, w_fan_out] = config.weights;

    w_lcom4 * lcom4_norm
        + w_fields * field_norm
        + w_methods * method_norm
        + w_fan_out * fan_out_norm
}

/// Return `(value / max).min(1.0)`, with a zero-guard: if `max == 0`,
/// treat any non-zero value as fully penalised (1.0) and zero as 0.0
/// so the score stays well-defined under a misconfigured threshold.
/// Operation: arithmetic with one branch.
fn normalised_ratio(value: usize, max: usize) -> f64 {
    if max == 0 {
        return if value == 0 { 0.0 } else { 1.0 };
    }
    (value as f64 / max as f64).min(1.0)
}
