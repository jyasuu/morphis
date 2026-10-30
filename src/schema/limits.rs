//! Query depth limiting that exempts introspection.
//!
//! async-graphql's built-in `limit_depth` counts introspection too, and the
//! standard introspection query (used by the admin UI and the MCP
//! `graphql_schema` tool) nests deeper than any sensible limit for data
//! queries. Depth is the main guard against response-size blowups through
//! relation cycles (`has_many` -> `belongs_to` -> `has_many` ...), so rather
//! than loosening it to let introspection through, we enforce it ourselves and
//! skip the `__schema` / `__type` roots.
//!
//! Depth counts nested fields: a root field is 1, so `{ materialsList { name } }`
//! has depth 2.

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery};
use async_graphql::parser::types::{
    DocumentOperations, ExecutableDocument, FragmentDefinition, Selection, SelectionSet,
};
use async_graphql::{Name, Positioned, ServerError, ServerResult, Variables};

/// Rejects operations nested deeper than `max_depth` (introspection exempt).
pub(crate) struct DepthLimit {
    max_depth: usize,
}

impl DepthLimit {
    pub(crate) fn new(max_depth: usize) -> Self {
        Self { max_depth }
    }
}

impl ExtensionFactory for DepthLimit {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(DepthLimitExtension {
            max_depth: self.max_depth,
        })
    }
}

struct DepthLimitExtension {
    max_depth: usize,
}

#[async_graphql::async_trait::async_trait]
impl Extension for DepthLimitExtension {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;
        let depth = max_depth(&doc);
        if depth > self.max_depth {
            return Err(ServerError::new(
                format!(
                    "Query is nested too deep ({depth} levels, maximum is {})",
                    self.max_depth
                ),
                None,
            ));
        }
        Ok(doc)
    }
}

/// Deepest nesting across every operation in the document, ignoring
/// introspection root fields.
pub(crate) fn max_depth(doc: &ExecutableDocument) -> usize {
    let mut memo = HashMap::new();
    let mut deepest = 0;
    let mut visit = |set: &SelectionSet| {
        let mut visiting = Vec::new();
        deepest = deepest.max(selection_depth(
            set,
            &doc.fragments,
            true,
            &mut visiting,
            &mut memo,
        ));
    };
    match &doc.operations {
        DocumentOperations::Single(op) => visit(&op.node.selection_set.node),
        DocumentOperations::Multiple(ops) => {
            for op in ops.values() {
                visit(&op.node.selection_set.node);
            }
        }
    }
    deepest
}

fn selection_depth(
    set: &SelectionSet,
    fragments: &HashMap<Name, Positioned<FragmentDefinition>>,
    at_root: bool,
    visiting: &mut Vec<Name>,
    memo: &mut HashMap<(Name, bool), usize>,
) -> usize {
    let mut deepest = 0;
    for item in &set.items {
        let depth = match &item.node {
            Selection::Field(field) => {
                if at_root && field.node.name.node.starts_with("__") {
                    0
                } else {
                    1 + selection_depth(
                        &field.node.selection_set.node,
                        fragments,
                        false,
                        visiting,
                        memo,
                    )
                }
            }
            Selection::InlineFragment(inline) => selection_depth(
                &inline.node.selection_set.node,
                fragments,
                at_root,
                visiting,
                memo,
            ),
            Selection::FragmentSpread(spread) => {
                let name = &spread.node.fragment_name.node;
                let key = (name.clone(), at_root);
                if let Some(depth) = memo.get(&key) {
                    *depth
                } else if visiting.contains(name) {
                    // A cycle; validation rejects it right after parsing.
                    0
                } else if let Some(def) = fragments.get(name) {
                    visiting.push(name.clone());
                    let depth = selection_depth(
                        &def.node.selection_set.node,
                        fragments,
                        at_root,
                        visiting,
                        memo,
                    );
                    visiting.pop();
                    memo.insert(key, depth);
                    depth
                } else {
                    0
                }
            }
        };
        deepest = deepest.max(depth);
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depth_of(query: &str) -> usize {
        max_depth(&async_graphql::parser::parse_query(query).unwrap())
    }

    #[test]
    fn counts_nested_fields_with_root_as_one() {
        assert_eq!(depth_of("{ a }"), 1);
        assert_eq!(depth_of("{ a { b } }"), 2);
        assert_eq!(depth_of("{ a { b { c { d } } } e { f } }"), 4);
    }

    #[test]
    fn follows_inline_and_named_fragments() {
        assert_eq!(depth_of("{ a { ... on T { b { c } } } }"), 3);
        assert_eq!(depth_of("{ a { ...F } } fragment F on T { b { c } }"), 3);
    }

    #[test]
    fn introspection_roots_are_exempt_but_data_fields_beside_them_count() {
        assert_eq!(depth_of("{ __schema { types { fields { type { ofType { name } } } } } }"), 0);
        assert_eq!(depth_of("{ __type(name: \"X\") { name } a { b } }"), 2);
        // Exempt even when reached through a fragment on the root.
        assert_eq!(depth_of("{ ...Q } fragment Q on Query { __schema { types { name } } }"), 0);
    }

    #[test]
    fn double_underscore_below_the_root_is_not_exempt() {
        assert_eq!(depth_of("{ a { __typename } }"), 2);
    }

    #[test]
    fn fragment_cycles_terminate() {
        let d = depth_of("{ a { ...F } } fragment F on T { b { ...G } } fragment G on T { c { ...F } }");
        assert!(d >= 3);
    }

    #[test]
    fn repeated_fragment_fan_out_stays_fast() {
        // 2^20 paths if expanded naively; memoised it is linear.
        let mut q = String::from("{ a { ...F0 } }");
        for i in 0..20 {
            q.push_str(&format!(" fragment F{i} on T {{ x {{ ...F{n} }} y {{ ...F{n} }} }}", n = i + 1));
        }
        q.push_str(" fragment F20 on T { leaf }");
        assert_eq!(depth_of(&q), 1 + 20 + 1);
    }

    #[test]
    fn takes_the_deepest_of_multiple_operations() {
        assert_eq!(depth_of("query A { a } query B { a { b { c } } }"), 3);
    }
}
