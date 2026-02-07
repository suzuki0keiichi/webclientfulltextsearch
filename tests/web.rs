//! WASM integration tests for FullTextSearch.
//!
//! Run with: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

extern crate wasm_bindgen_test;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use webclientfulltextsearch::FullTextSearch;

wasm_bindgen_test_configure!(run_in_browser);

fn make_schema() -> JsValue {
    serde_wasm_bindgen::to_value(&serde_json::json!({
        "fields": [
            { "name": "title", "kind": "text", "weight": 2.0 },
            { "name": "body",  "kind": "text", "weight": 1.0 },
            { "name": "date",  "kind": "stored" }
        ],
        "ngram_size": 3
    }))
    .unwrap()
}

fn make_docs(docs: &[(&str, &str, &str, &str)]) -> JsValue {
    let items: Vec<serde_json::Value> = docs
        .iter()
        .map(|(id, title, body, date)| {
            serde_json::json!({
                "id": id,
                "title": title,
                "body": body,
                "date": date,
            })
        })
        .collect();
    serde_wasm_bindgen::to_value(&items).unwrap()
}

#[wasm_bindgen_test]
fn search_empty_index_returns_empty() {
    let index = FullTextSearch::new(make_schema()).unwrap();
    let results: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(index.search("hello", JsValue::NULL).unwrap()).unwrap();
    assert!(results.is_empty());
}

#[wasm_bindgen_test]
fn add_and_search_basic() {
    let mut index = FullTextSearch::new(make_schema()).unwrap();
    index
        .add(make_docs(&[
            ("1", "Rust programming", "systems language", "2024-01-01"),
            ("2", "JavaScript web", "frontend development", "2024-02-01"),
        ]))
        .unwrap();

    let results: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(index.search("Rust", JsValue::NULL).unwrap()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], "1");
}

#[wasm_bindgen_test]
fn search_japanese_text() {
    let mut index = FullTextSearch::new(make_schema()).unwrap();
    index
        .add(make_docs(&[
            ("v1", "情報検索システム", "設計と実装", "2024-01-01"),
            ("v2", "機械学習入門", "ニューラルネットワーク", "2024-02-01"),
        ]))
        .unwrap();

    let results: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(index.search("情報検索", JsValue::NULL).unwrap()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], "v1");
}

#[wasm_bindgen_test]
fn count_property() {
    let mut index = FullTextSearch::new(make_schema()).unwrap();
    assert_eq!(index.count(), 0);

    index
        .add(make_docs(&[
            ("1", "hello", "world", "2024-01-01"),
            ("2", "foo", "bar", "2024-02-01"),
        ]))
        .unwrap();
    assert_eq!(index.count(), 2);
}

#[wasm_bindgen_test]
fn multiple_independent_instances() {
    let mut a = FullTextSearch::new(make_schema()).unwrap();
    let mut b = FullTextSearch::new(make_schema()).unwrap();

    a.add(make_docs(&[("1", "only in alpha", "body", "2024-01-01")]))
        .unwrap();
    b.add(make_docs(&[("2", "only in beta", "body", "2024-01-01")]))
        .unwrap();

    let results_a: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(a.search("alpha", JsValue::NULL).unwrap()).unwrap();
    let results_b: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(b.search("alpha", JsValue::NULL).unwrap()).unwrap();

    assert_eq!(results_a.len(), 1);
    assert!(results_b.is_empty());
}

#[wasm_bindgen_test]
fn export_and_import() {
    let mut original = FullTextSearch::new(make_schema()).unwrap();
    original
        .add(make_docs(&[
            ("1", "Rust language", "systems", "2024-01-01"),
            ("2", "Python scripting", "easy", "2024-02-01"),
        ]))
        .unwrap();

    let exported = original.export_index().unwrap();
    let restored = FullTextSearch::import_index(exported).unwrap();

    assert_eq!(restored.count(), 2);
    let results: Vec<serde_json::Value> =
        serde_wasm_bindgen::from_value(restored.search("Rust", JsValue::NULL).unwrap()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], "1");
}
