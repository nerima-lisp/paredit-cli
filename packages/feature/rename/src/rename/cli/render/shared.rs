use serde_json::{Value, json};

use crate::rename::usecase::{
    RenameFunctionOccurrence, ReplaceFunctionCallSite, UnwrapFunctionCallSite, WrapFunctionCallSite,
};

pub fn rename_occurrences_json(occurrences: &[RenameFunctionOccurrence]) -> Vec<Value> {
    occurrences
        .iter()
        .map(|occurrence| {
            json!({
                "path": occurrence.path,
                "span": {
                    "start": occurrence.span.start().get(),
                    "end": occurrence.span.end().get(),
                },
                "text": occurrence.text,
                "replacement": occurrence.replacement,
            })
        })
        .collect()
}

pub fn wrap_call_sites_json(sites: &[WrapFunctionCallSite]) -> Vec<Value> {
    sites
        .iter()
        .map(|site| {
            json!({
                "path": site.path,
                "span": {
                    "start": site.span.start().get(),
                    "end": site.span.end().get(),
                },
                "text": site.text,
                "replacement": site.replacement,
            })
        })
        .collect()
}

pub fn replace_call_sites_json(sites: &[ReplaceFunctionCallSite]) -> Vec<Value> {
    sites
        .iter()
        .map(|site| {
            json!({
                "path": site.path,
                "span": {
                    "start": site.span.start().get(),
                    "end": site.span.end().get(),
                },
                "headSpan": {
                    "start": site.head_span.start().get(),
                    "end": site.head_span.end().get(),
                },
                "text": site.text,
                "replacement": site.replacement,
            })
        })
        .collect()
}

pub fn unwrap_call_sites_json(sites: &[UnwrapFunctionCallSite]) -> Vec<Value> {
    sites
        .iter()
        .map(|site| {
            json!({
                "path": site.path,
                "span": {
                    "start": site.span.start().get(),
                    "end": site.span.end().get(),
                },
                "text": site.text,
                "replacement": site.replacement,
            })
        })
        .collect()
}
