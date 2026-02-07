use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_NGRAM_SIZE: usize = 3;
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

// ── Schema ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SchemaConfig {
    pub fields: Vec<FieldConfig>,
    #[serde(default = "default_ngram_size")]
    pub ngram_size: usize,
}

fn default_ngram_size() -> usize {
    DEFAULT_NGRAM_SIZE
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct FieldConfig {
    pub name: String,
    #[serde(default)]
    pub kind: FieldKind,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FieldKind {
    /// Full-text indexed and stored.
    Text,
    /// Stored only (for sorting, display, filtering). Not indexed.
    Stored,
}

impl Default for FieldKind {
    fn default() -> Self {
        FieldKind::Text
    }
}

// ── N-gram tokenizer ─────────────────────────────────────────────────────────

/// Extracts character-level n-grams from `text`.
///
/// - Lowercased for case-insensitive matching.
/// - Whitespace/control chars act as word boundaries.
/// - Words shorter than `n` are included as-is when delimited by boundaries.
pub(crate) fn ngrams(text: &str, n: usize) -> HashSet<String> {
    let mut grams = HashSet::new();
    let mut window: Vec<char> = Vec::with_capacity(n);
    let mut emitted = false;

    for ch in text.chars() {
        if ch.is_whitespace() || ch.is_control() {
            if !window.is_empty() && !emitted {
                grams.insert(window.iter().collect());
            }
            window.clear();
            emitted = false;
            continue;
        }

        for lower in ch.to_lowercase() {
            window.push(lower);
            if window.len() == n {
                grams.insert(window.iter().collect());
                window.remove(0);
                emitted = true;
            }
        }
    }

    if !window.is_empty() && !emitted {
        grams.insert(window.iter().collect());
    }

    grams
}

// ── Query AST ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub(crate) enum Query {
    Term(String),
    Phrase(String),
    Field(String, Box<Query>),
    And(Vec<Query>),
    Or(Vec<Query>),
    Not(Box<Query>),
}

// ── Query Parser ─────────────────────────────────────────────────────────────
//
// Grammar:
//   query      = or_expr
//   or_expr    = and_expr ("OR" and_expr)*
//   and_expr   = unary (unary)*
//   unary      = ("NOT" | "-") atom | atom
//   atom       = phrase | "(" query ")" | field_or_term
//   field_or_term = WORD ":" atom | WORD

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Phrase(String),
    Or,
    Not,
    LParen,
    RParen,
    Colon,
    Minus,
}

fn tokenize_query(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' {
            chars.next();
            let mut phrase = String::new();
            while let Some(&c) = chars.peek() {
                if c == '"' {
                    chars.next();
                    break;
                }
                phrase.push(c);
                chars.next();
            }
            tokens.push(Token::Phrase(phrase));
        } else if ch == '(' {
            tokens.push(Token::LParen);
            chars.next();
        } else if ch == ')' {
            tokens.push(Token::RParen);
            chars.next();
        } else if ch == ':' {
            tokens.push(Token::Colon);
            chars.next();
        } else if ch == '-' {
            tokens.push(Token::Minus);
            chars.next();
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '(' || c == ')' || c == ':' || c == '"' {
                    break;
                }
                word.push(c);
                chars.next();
            }

            match word.as_str() {
                "OR" => tokens.push(Token::Or),
                "NOT" => tokens.push(Token::Not),
                "AND" => {} // implicit AND, skip
                _ => tokens.push(Token::Word(word)),
            }
        }
    }

    tokens
}

pub(crate) fn parse_query(input: &str) -> Query {
    let tokens = tokenize_query(input);
    if tokens.is_empty() {
        return Query::Term(String::new());
    }
    let mut pos = 0;
    parse_or(&tokens, &mut pos)
}

fn parse_or(tokens: &[Token], pos: &mut usize) -> Query {
    let mut left = parse_and(tokens, pos);

    while *pos < tokens.len() && tokens[*pos] == Token::Or {
        *pos += 1;
        let right = parse_and(tokens, pos);
        left = match left {
            Query::Or(mut children) => {
                children.push(right);
                Query::Or(children)
            }
            _ => Query::Or(vec![left, right]),
        };
    }

    left
}

fn parse_and(tokens: &[Token], pos: &mut usize) -> Query {
    let mut children = vec![parse_unary(tokens, pos)];

    while *pos < tokens.len() && tokens[*pos] != Token::Or && tokens[*pos] != Token::RParen {
        children.push(parse_unary(tokens, pos));
    }

    if children.len() == 1 {
        children.pop().unwrap()
    } else {
        Query::And(children)
    }
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Query {
    if *pos >= tokens.len() {
        return Query::Term(String::new());
    }

    match &tokens[*pos] {
        Token::Not | Token::Minus => {
            *pos += 1;
            let inner = parse_atom(tokens, pos);
            Query::Not(Box::new(inner))
        }
        _ => parse_atom(tokens, pos),
    }
}

fn parse_atom(tokens: &[Token], pos: &mut usize) -> Query {
    if *pos >= tokens.len() {
        return Query::Term(String::new());
    }

    match &tokens[*pos] {
        Token::Phrase(s) => {
            let phrase = s.clone();
            *pos += 1;
            Query::Phrase(phrase)
        }
        Token::LParen => {
            *pos += 1;
            let inner = parse_or(tokens, pos);
            if *pos < tokens.len() && tokens[*pos] == Token::RParen {
                *pos += 1;
            }
            inner
        }
        Token::Word(w) => {
            let word = w.clone();
            *pos += 1;
            // Check for field:value
            if *pos < tokens.len() && tokens[*pos] == Token::Colon {
                *pos += 1;
                let value = parse_atom(tokens, pos);
                Query::Field(word, Box::new(value))
            } else {
                Query::Term(word)
            }
        }
        _ => {
            *pos += 1;
            Query::Term(String::new())
        }
    }
}

// ── SearchIndex ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub(crate) struct SearchIndex {
    schema: SchemaConfig,

    // Document storage
    doc_ids: Vec<String>,
    #[serde(with = "hashmap_u32_serde")]
    id_to_idx: HashMap<String, u32>,
    doc_fields: Vec<HashMap<String, String>>,

    // Inverted index: field -> ngram -> sorted posting list
    postings: HashMap<String, HashMap<String, Vec<u32>>>,

    // BM25 metadata
    field_lengths: HashMap<String, Vec<u32>>,
    field_total_length: HashMap<String, u64>,

    // Deletion tracking
    deleted: HashSet<u32>,
}

/// serde helper: HashMap<String, u32> needs no special treatment but
/// we keep this module in case we switch to a binary format later.
mod hashmap_u32_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S: Serializer>(
        map: &HashMap<String, u32>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<HashMap<String, u32>, D::Error> {
        HashMap::deserialize(deserializer)
    }
}

impl SearchIndex {
    pub fn new(schema: SchemaConfig) -> Self {
        let mut postings = HashMap::new();
        let mut field_lengths = HashMap::new();
        let mut field_total_length = HashMap::new();

        for field in &schema.fields {
            if field.kind == FieldKind::Text {
                postings.insert(field.name.clone(), HashMap::new());
                field_lengths.insert(field.name.clone(), Vec::new());
                field_total_length.insert(field.name.clone(), 0);
            }
        }

        Self {
            schema,
            doc_ids: Vec::new(),
            id_to_idx: HashMap::new(),
            doc_fields: Vec::new(),
            postings,
            field_lengths,
            field_total_length,
            deleted: HashSet::new(),
        }
    }

    pub fn add(&mut self, docs: &[HashMap<String, String>]) {
        let n = self.schema.ngram_size;

        for doc in docs {
            let id = match doc.get("id") {
                Some(id) => id.clone(),
                None => continue,
            };

            let doc_idx = self.doc_ids.len() as u32;
            self.doc_ids.push(id.clone());
            self.id_to_idx.insert(id, doc_idx);
            self.doc_fields.push(doc.clone());

            for field_config in &self.schema.fields {
                if field_config.kind != FieldKind::Text {
                    continue;
                }

                let field_name = &field_config.name;
                let text = match doc.get(field_name) {
                    Some(t) => t,
                    None => {
                        if let Some(lengths) = self.field_lengths.get_mut(field_name) {
                            lengths.push(0);
                        }
                        continue;
                    }
                };

                let char_count = text.chars().count() as u32;
                if let Some(lengths) = self.field_lengths.get_mut(field_name) {
                    lengths.push(char_count);
                }
                if let Some(total) = self.field_total_length.get_mut(field_name) {
                    *total += char_count as u64;
                }

                let grams = ngrams(text, n);
                if let Some(field_postings) = self.postings.get_mut(field_name) {
                    for gram in grams {
                        let list = field_postings.entry(gram).or_default();
                        if list.last() != Some(&doc_idx) {
                            list.push(doc_idx);
                        }
                    }
                }
            }
        }
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(&idx) = self.id_to_idx.get(id) {
            self.deleted.insert(idx);
        }
    }

    pub fn search(&self, query_str: &str, options: &SearchOptions) -> Vec<SearchResult> {
        let query = parse_query(query_str);

        let all_docs: HashSet<u32> = (0..self.doc_ids.len() as u32)
            .filter(|idx| !self.deleted.contains(idx))
            .collect();

        let matching = self.execute_query(&query, None, &all_docs);

        if matching.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = matching
            .iter()
            .map(|&doc_idx| {
                let score = self.score_bm25(doc_idx, &query, None);
                SearchResult {
                    id: self.doc_ids[doc_idx as usize].clone(),
                    score,
                    doc: self.doc_fields[doc_idx as usize].clone(),
                }
            })
            .collect();

        // Sort
        match &options.sort {
            Some(sort) => {
                let field = &sort.field;
                let desc = matches!(sort.order, SortOrder::Desc);
                results.sort_by(|a, b| {
                    let va = a.doc.get(field).map(|s| s.as_str()).unwrap_or("");
                    let vb = b.doc.get(field).map(|s| s.as_str()).unwrap_or("");
                    // Try numeric comparison first
                    let cmp = match (va.parse::<f64>(), vb.parse::<f64>()) {
                        (Ok(na), Ok(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
                        _ => va.cmp(vb),
                    };
                    if desc { cmp.reverse() } else { cmp }
                });
            }
            None => {
                results.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // Pagination
        let offset = options.offset.unwrap_or(0);
        let limit = options.limit.unwrap_or(results.len());
        results.into_iter().skip(offset).take(limit).collect()
    }

    fn execute_query(
        &self,
        query: &Query,
        field: Option<&str>,
        all_docs: &HashSet<u32>,
    ) -> HashSet<u32> {
        match query {
            Query::Term(term) => {
                if term.is_empty() {
                    return HashSet::new();
                }
                self.search_term(term, field)
            }
            Query::Phrase(phrase) => {
                if phrase.is_empty() {
                    return HashSet::new();
                }
                self.search_phrase(phrase, field)
            }
            Query::Field(field_name, inner) => {
                self.execute_query(inner, Some(field_name), all_docs)
            }
            Query::And(children) => {
                let mut result: Option<HashSet<u32>> = None;
                for child in children {
                    let child_result = self.execute_query(child, field, all_docs);
                    result = Some(match result {
                        Some(r) => r.intersection(&child_result).copied().collect(),
                        None => child_result,
                    });
                }
                result.unwrap_or_default()
            }
            Query::Or(children) => {
                let mut result = HashSet::new();
                for child in children {
                    result.extend(self.execute_query(child, field, all_docs));
                }
                result
            }
            Query::Not(inner) => {
                let excluded = self.execute_query(inner, field, all_docs);
                all_docs.difference(&excluded).copied().collect()
            }
        }
    }

    fn text_fields(&self) -> Vec<&str> {
        self.schema
            .fields
            .iter()
            .filter(|f| f.kind == FieldKind::Text)
            .map(|f| f.name.as_str())
            .collect()
    }

    fn search_term(&self, term: &str, field: Option<&str>) -> HashSet<u32> {
        let n = self.schema.ngram_size;
        let grams = ngrams(term, n);

        if grams.is_empty() {
            return HashSet::new();
        }

        let fields: Vec<&str> = match field {
            Some(f) => vec![f],
            None => self.text_fields(),
        };

        let mut result = HashSet::new();

        for field_name in &fields {
            if let Some(field_postings) = self.postings.get(*field_name) {
                let mut lists: Vec<&Vec<u32>> = Vec::new();
                let mut all_found = true;

                for gram in &grams {
                    match field_postings.get(gram) {
                        Some(list) => lists.push(list),
                        None => {
                            all_found = false;
                            break;
                        }
                    }
                }

                if !all_found || lists.is_empty() {
                    continue;
                }

                lists.sort_by_key(|l| l.len());

                let mut field_result: Vec<u32> = lists[0].clone();
                for list in &lists[1..] {
                    field_result = intersect_sorted(&field_result, list);
                    if field_result.is_empty() {
                        break;
                    }
                }

                for idx in field_result {
                    if !self.deleted.contains(&idx) {
                        result.insert(idx);
                    }
                }
            }
        }

        result
    }

    fn search_phrase(&self, phrase: &str, field: Option<&str>) -> HashSet<u32> {
        // Use n-gram intersection for candidate retrieval
        let candidates = self.search_term(phrase, field);
        if candidates.is_empty() {
            return candidates;
        }

        // Verify by substring match on stored text
        let phrase_lower = phrase.to_lowercase();
        let fields: Vec<&str> = match field {
            Some(f) => vec![f],
            None => self.text_fields(),
        };

        candidates
            .into_iter()
            .filter(|&doc_idx| {
                let doc = &self.doc_fields[doc_idx as usize];
                for field_name in &fields {
                    if let Some(text) = doc.get(*field_name) {
                        if text.to_lowercase().contains(&phrase_lower) {
                            return true;
                        }
                    }
                }
                false
            })
            .collect()
    }

    // ── BM25 scoring ─────────────────────────────────────────────────────

    fn score_bm25(&self, doc_idx: u32, query: &Query, field: Option<&str>) -> f64 {
        match query {
            Query::Term(term) => self.score_term_bm25(doc_idx, term, field),
            Query::Phrase(phrase) => self.score_term_bm25(doc_idx, phrase, field),
            Query::Field(field_name, inner) => {
                self.score_bm25(doc_idx, inner, Some(field_name))
            }
            Query::And(children) | Query::Or(children) => children
                .iter()
                .map(|c| self.score_bm25(doc_idx, c, field))
                .sum(),
            Query::Not(_) => 0.0,
        }
    }

    fn score_term_bm25(&self, doc_idx: u32, term: &str, field: Option<&str>) -> f64 {
        let fields: Vec<&FieldConfig> = match field {
            Some(f) => self
                .schema
                .fields
                .iter()
                .filter(|fc| fc.name == f && fc.kind == FieldKind::Text)
                .collect(),
            None => self
                .schema
                .fields
                .iter()
                .filter(|fc| fc.kind == FieldKind::Text)
                .collect(),
        };

        let n_docs = (self.doc_ids.len() - self.deleted.len()) as f64;
        if n_docs == 0.0 {
            return 0.0;
        }

        let term_lower = term.to_lowercase();
        let mut total_score = 0.0;

        for field_config in &fields {
            let field_name = &field_config.name;
            let weight = field_config.weight;

            let text = match self.doc_fields[doc_idx as usize].get(field_name) {
                Some(t) => t.to_lowercase(),
                None => continue,
            };

            let tf = count_occurrences(&text, &term_lower) as f64;
            if tf == 0.0 {
                continue;
            }

            let df = self.estimate_df(&term_lower, field_name) as f64;
            if df == 0.0 {
                continue;
            }

            let idf = ((n_docs - df + 0.5) / (df + 0.5) + 1.0).ln();

            let dl = self
                .field_lengths
                .get(field_name)
                .and_then(|lengths| lengths.get(doc_idx as usize))
                .copied()
                .unwrap_or(0) as f64;

            let n_total_docs = self
                .field_lengths
                .get(field_name)
                .map(|l| l.len())
                .unwrap_or(0) as f64;
            let total_length = self
                .field_total_length
                .get(field_name)
                .copied()
                .unwrap_or(0) as f64;
            let avgdl = if n_total_docs > 0.0 {
                total_length / n_total_docs
            } else {
                1.0
            };

            let score =
                idf * (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avgdl));

            total_score += score * weight;
        }

        total_score
    }

    fn estimate_df(&self, term: &str, field_name: &str) -> usize {
        let n = self.schema.ngram_size;
        let grams = ngrams(term, n);

        if let Some(field_postings) = self.postings.get(field_name) {
            grams
                .iter()
                .filter_map(|g| field_postings.get(g))
                .map(|list| list.len())
                .min()
                .unwrap_or(0)
        } else {
            0
        }
    }

    pub fn doc_count(&self) -> usize {
        self.doc_ids.len() - self.deleted.len()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn intersect_sorted(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                result.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    result
}

fn count_occurrences(text: &str, pattern: &str) -> usize {
    if pattern.is_empty() {
        return 0;
    }
    text.matches(pattern).count()
}

// ── Search options / results ─────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(crate) struct SearchOptions {
    #[serde(default)]
    pub sort: Option<SortOption>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct SortOption {
    pub field: String,
    #[serde(default)]
    pub order: SortOrder,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SortOrder {
    Asc,
    #[default]
    Desc,
}

#[derive(Serialize, Clone)]
pub(crate) struct SearchResult {
    pub id: String,
    pub score: f64,
    pub doc: HashMap<String, String>,
}

// ── WASM API ─────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct FullTextSearch {
    index: SearchIndex,
}

#[wasm_bindgen]
impl FullTextSearch {
    /// Creates a new search index with the given schema.
    ///
    /// ```js
    /// const index = new FullTextSearch({
    ///     fields: [
    ///         { name: "title", kind: "text", weight: 2.0 },
    ///         { name: "body",  kind: "text", weight: 1.0 },
    ///         { name: "date",  kind: "stored" },
    ///     ],
    ///     ngram_size: 3,
    /// });
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(schema: JsValue) -> Result<FullTextSearch, JsError> {
        let config: SchemaConfig = serde_wasm_bindgen::from_value(schema)?;
        Ok(Self {
            index: SearchIndex::new(config),
        })
    }

    /// Adds documents to the index.
    ///
    /// Each document must have an `id` field. Other fields are indexed or stored
    /// according to the schema.
    pub fn add(&mut self, docs: JsValue) -> Result<(), JsError> {
        let items: Vec<HashMap<String, String>> = serde_wasm_bindgen::from_value(docs)?;
        self.index.add(&items);
        Ok(())
    }

    /// Searches the index.
    ///
    /// Query syntax:
    /// - `hello world` — AND (both must match)
    /// - `hello OR world` — OR
    /// - `NOT hello` or `-hello` — exclude
    /// - `"exact phrase"` — phrase match
    /// - `title:hello` — field-specific
    /// - `(a OR b) c` — grouping
    pub fn search(&self, query: &str, options: JsValue) -> Result<JsValue, JsError> {
        let opts: SearchOptions = if options.is_undefined() || options.is_null() {
            SearchOptions::default()
        } else {
            serde_wasm_bindgen::from_value(options)?
        };

        let results = self.index.search(query, &opts);
        serde_wasm_bindgen::to_value(&results).map_err(JsError::from)
    }

    /// Removes a document by id (lazy deletion).
    pub fn remove(&mut self, id: &str) {
        self.index.remove(id);
    }

    /// Number of active (non-deleted) documents.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.index.doc_count()
    }

    /// Exports the full index for IndexedDB persistence.
    pub fn export_index(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.index).map_err(JsError::from)
    }

    /// Imports a previously exported index.
    pub fn import_index(data: JsValue) -> Result<FullTextSearch, JsError> {
        let index: SearchIndex = serde_wasm_bindgen::from_value(data)?;
        Ok(Self { index })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_schema() -> SchemaConfig {
        SchemaConfig {
            fields: vec![
                FieldConfig {
                    name: "title".into(),
                    kind: FieldKind::Text,
                    weight: 2.0,
                },
                FieldConfig {
                    name: "body".into(),
                    kind: FieldKind::Text,
                    weight: 1.0,
                },
                FieldConfig {
                    name: "date".into(),
                    kind: FieldKind::Stored,
                    weight: 1.0,
                },
            ],
            ngram_size: 3,
        }
    }

    fn make_doc(id: &str, title: &str, body: &str, date: &str) -> HashMap<String, String> {
        HashMap::from([
            ("id".into(), id.into()),
            ("title".into(), title.into()),
            ("body".into(), body.into()),
            ("date".into(), date.into()),
        ])
    }

    // ── ngrams ───────────────────────────────────────────────────────────

    #[test]
    fn ngrams_basic_ascii() {
        let r = ngrams("hello", 3);
        assert_eq!(r, HashSet::from(["hel".into(), "ell".into(), "llo".into()]));
    }

    #[test]
    fn ngrams_short_word() {
        assert_eq!(ngrams("hi", 3), HashSet::from(["hi".into()]));
    }

    #[test]
    fn ngrams_single_char() {
        assert_eq!(ngrams("a", 3), HashSet::from(["a".into()]));
    }

    #[test]
    fn ngrams_empty() {
        assert!(ngrams("", 3).is_empty());
    }

    #[test]
    fn ngrams_whitespace_boundary() {
        let r = ngrams("hi there", 3);
        assert!(r.contains("hi"));
        assert!(r.contains("the"));
        assert!(r.contains("her"));
        assert!(r.contains("ere"));
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn ngrams_case_insensitive() {
        assert_eq!(ngrams("Hello", 3), ngrams("hello", 3));
    }

    #[test]
    fn ngrams_japanese() {
        assert_eq!(
            ngrams("情報検索", 3),
            HashSet::from(["情報検".into(), "報検索".into()])
        );
    }

    #[test]
    fn ngrams_japanese_with_spaces() {
        assert_eq!(
            ngrams("東京 大学", 3),
            HashSet::from(["東京".into(), "大学".into()])
        );
    }

    #[test]
    fn ngrams_mixed_cjk_and_ascii() {
        let r = ngrams("Rust言語", 3);
        assert!(r.contains("rus"));
        assert!(r.contains("ust"));
        assert!(r.contains("st言"));
        assert!(r.contains("t言語"));
    }

    #[test]
    fn ngrams_word_boundary_clears_state() {
        let r = ngrams("abc xyz", 3);
        assert!(r.contains("abc"));
        assert!(r.contains("xyz"));
        assert!(!r.contains("bcx"));
    }

    // ── Query parser ─────────────────────────────────────────────────────

    #[test]
    fn parse_single_term() {
        assert_eq!(parse_query("hello"), Query::Term("hello".into()));
    }

    #[test]
    fn parse_implicit_and() {
        assert_eq!(
            parse_query("hello world"),
            Query::And(vec![
                Query::Term("hello".into()),
                Query::Term("world".into()),
            ])
        );
    }

    #[test]
    fn parse_or() {
        assert_eq!(
            parse_query("hello OR world"),
            Query::Or(vec![
                Query::Term("hello".into()),
                Query::Term("world".into()),
            ])
        );
    }

    #[test]
    fn parse_not() {
        assert_eq!(
            parse_query("NOT hello"),
            Query::Not(Box::new(Query::Term("hello".into())))
        );
    }

    #[test]
    fn parse_minus_not() {
        assert_eq!(
            parse_query("-hello"),
            Query::Not(Box::new(Query::Term("hello".into())))
        );
    }

    #[test]
    fn parse_phrase() {
        assert_eq!(
            parse_query("\"hello world\""),
            Query::Phrase("hello world".into())
        );
    }

    #[test]
    fn parse_field() {
        assert_eq!(
            parse_query("title:hello"),
            Query::Field("title".into(), Box::new(Query::Term("hello".into())))
        );
    }

    #[test]
    fn parse_field_phrase() {
        assert_eq!(
            parse_query("title:\"hello world\""),
            Query::Field(
                "title".into(),
                Box::new(Query::Phrase("hello world".into()))
            )
        );
    }

    #[test]
    fn parse_grouped_or_and() {
        assert_eq!(
            parse_query("(hello OR world) rust"),
            Query::And(vec![
                Query::Or(vec![
                    Query::Term("hello".into()),
                    Query::Term("world".into()),
                ]),
                Query::Term("rust".into()),
            ])
        );
    }

    #[test]
    fn parse_complex_query() {
        // "rust OR python" combined with NOT "java"
        let q = parse_query("(rust OR python) NOT java");
        assert_eq!(
            q,
            Query::And(vec![
                Query::Or(vec![
                    Query::Term("rust".into()),
                    Query::Term("python".into()),
                ]),
                Query::Not(Box::new(Query::Term("java".into()))),
            ])
        );
    }

    // ── intersect_sorted ─────────────────────────────────────────────────

    #[test]
    fn intersect_empty() {
        assert!(intersect_sorted(&[], &[]).is_empty());
        assert!(intersect_sorted(&[1, 2], &[]).is_empty());
    }

    #[test]
    fn intersect_overlapping() {
        assert_eq!(intersect_sorted(&[1, 2, 3, 5], &[2, 3, 4, 5]), vec![2, 3, 5]);
    }

    #[test]
    fn intersect_identical() {
        assert_eq!(intersect_sorted(&[1, 2, 3], &[1, 2, 3]), vec![1, 2, 3]);
    }

    // ── SearchIndex: basic search ────────────────────────────────────────

    #[test]
    fn search_basic() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust programming", "Systems language", "2024-01-01"),
            make_doc("2", "JavaScript web", "Frontend development", "2024-02-01"),
            make_doc("3", "Rust web assembly", "WASM tutorial", "2024-03-01"),
        ]);

        let results = index.search("Rust", &SearchOptions::default());
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"3"));
        assert!(!ids.contains(&"2"));
    }

    #[test]
    fn search_empty_index() {
        let index = SearchIndex::new(test_schema());
        assert!(index.search("hello", &SearchOptions::default()).is_empty());
    }

    #[test]
    fn search_empty_query() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[make_doc("1", "hello", "world", "2024-01-01")]);
        assert!(index.search("", &SearchOptions::default()).is_empty());
    }

    #[test]
    fn search_japanese() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("v1", "情報検索システム", "設計と実装について", "2024-01-01"),
            make_doc("v2", "機械学習入門", "ニューラルネットワーク", "2024-02-01"),
        ]);

        let results = index.search("情報検索", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "v1");
    }

    #[test]
    fn search_case_insensitive() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[make_doc("1", "Hello World", "body", "2024-01-01")]);

        assert_eq!(index.search("hello", &SearchOptions::default()).len(), 1);
        assert_eq!(index.search("HELLO", &SearchOptions::default()).len(), 1);
    }

    // ── AND / OR / NOT ───────────────────────────────────────────────────

    #[test]
    fn search_implicit_and() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust programming", "fast language", "2024-01-01"),
            make_doc("2", "Python programming", "easy language", "2024-02-01"),
        ]);

        let results = index.search("Rust programming", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn search_or() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust language", "systems", "2024-01-01"),
            make_doc("2", "Python language", "scripting", "2024-02-01"),
            make_doc("3", "Java language", "enterprise", "2024-03-01"),
        ]);

        let results = index.search("Rust OR Python", &SearchOptions::default());
        let ids: HashSet<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains("1"));
        assert!(ids.contains("2"));
        assert!(!ids.contains("3"));
    }

    #[test]
    fn search_not() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust language", "systems", "2024-01-01"),
            make_doc("2", "Python language", "scripting", "2024-02-01"),
            make_doc("3", "Java language", "enterprise", "2024-03-01"),
        ]);

        let results = index.search("language NOT Java", &SearchOptions::default());
        let ids: HashSet<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains("1"));
        assert!(ids.contains("2"));
        assert!(!ids.contains("3"));
    }

    // ── Phrase search ────────────────────────────────────────────────────

    #[test]
    fn search_phrase() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust web assembly", "tutorial", "2024-01-01"),
            make_doc("2", "web Rust assembly", "guide", "2024-02-01"),
        ]);

        let results = index.search("\"Rust web\"", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn search_phrase_japanese() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "情報検索システム", "body", "2024-01-01"),
            make_doc("2", "検索情報システム", "body", "2024-02-01"),
        ]);

        let results = index.search("\"情報検索\"", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    // ── Field-specific search ────────────────────────────────────────────

    #[test]
    fn search_field_specific() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust", "Python scripting", "2024-01-01"),
            make_doc("2", "Python", "Rust systems", "2024-02-01"),
        ]);

        let results = index.search("title:Rust", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");

        let results = index.search("body:Rust", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "2");
    }

    // ── BM25 scoring ─────────────────────────────────────────────────────

    #[test]
    fn scoring_title_weight_higher() {
        let mut index = SearchIndex::new(test_schema());
        // "Rust" appears in title for doc1, in body for doc2
        index.add(&[
            make_doc("1", "Rust language", "programming guide", "2024-01-01"),
            make_doc("2", "programming guide", "Rust language", "2024-02-01"),
        ]);

        let results = index.search("Rust", &SearchOptions::default());
        assert_eq!(results.len(), 2);
        // Doc with "Rust" in title (weight=2.0) should score higher
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn scoring_more_occurrences_rank_higher() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust", "introduction", "2024-01-01"),
            make_doc("2", "Rust Rust Rust", "Rust programming in Rust", "2024-02-01"),
        ]);

        let results = index.search("Rust", &SearchOptions::default());
        assert_eq!(results[0].id, "2");
    }

    // ── Sort ─────────────────────────────────────────────────────────────

    #[test]
    fn sort_by_date_asc() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust", "lang", "2024-03-01"),
            make_doc("2", "Rust", "lang", "2024-01-01"),
            make_doc("3", "Rust", "lang", "2024-02-01"),
        ]);

        let opts = SearchOptions {
            sort: Some(SortOption {
                field: "date".into(),
                order: SortOrder::Asc,
            }),
            ..Default::default()
        };

        let results = index.search("Rust", &opts);
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["2", "3", "1"]);
    }

    #[test]
    fn sort_by_date_desc() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust", "lang", "2024-03-01"),
            make_doc("2", "Rust", "lang", "2024-01-01"),
            make_doc("3", "Rust", "lang", "2024-02-01"),
        ]);

        let opts = SearchOptions {
            sort: Some(SortOption {
                field: "date".into(),
                order: SortOrder::Desc,
            }),
            ..Default::default()
        };

        let results = index.search("Rust", &opts);
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "3", "2"]);
    }

    #[test]
    fn sort_numeric() {
        let schema = SchemaConfig {
            fields: vec![
                FieldConfig { name: "title".into(), kind: FieldKind::Text, weight: 1.0 },
                FieldConfig { name: "score".into(), kind: FieldKind::Stored, weight: 1.0 },
            ],
            ngram_size: 3,
        };
        let mut index = SearchIndex::new(schema);
        index.add(&[
            HashMap::from([("id".into(), "a".into()), ("title".into(), "hello world".into()), ("score".into(), "100".into())]),
            HashMap::from([("id".into(), "b".into()), ("title".into(), "hello world".into()), ("score".into(), "9".into())]),
            HashMap::from([("id".into(), "c".into()), ("title".into(), "hello world".into()), ("score".into(), "50".into())]),
        ]);

        let opts = SearchOptions {
            sort: Some(SortOption { field: "score".into(), order: SortOrder::Desc }),
            ..Default::default()
        };

        let results = index.search("hello", &opts);
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
    }

    // ── Pagination ───────────────────────────────────────────────────────

    #[test]
    fn pagination() {
        let mut index = SearchIndex::new(test_schema());
        for i in 0..10 {
            index.add(&[make_doc(
                &format!("doc{i}"),
                "common keyword here",
                "body text",
                &format!("2024-{:02}-01", i + 1),
            )]);
        }

        let opts = SearchOptions {
            sort: Some(SortOption { field: "date".into(), order: SortOrder::Asc }),
            limit: Some(3),
            offset: Some(2),
        };

        let results = index.search("keyword", &opts);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "doc2");
        assert_eq!(results[1].id, "doc3");
        assert_eq!(results[2].id, "doc4");
    }

    // ── Remove ───────────────────────────────────────────────────────────

    #[test]
    fn remove_document() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust language", "body", "2024-01-01"),
            make_doc("2", "Rust systems", "body", "2024-02-01"),
        ]);

        assert_eq!(index.search("Rust", &SearchOptions::default()).len(), 2);

        index.remove("1");
        assert_eq!(index.doc_count(), 1);

        let results = index.search("Rust", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "2");
    }

    // ── Export / Import ──────────────────────────────────────────────────

    #[test]
    fn export_import_roundtrip() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "Rust programming", "fast language", "2024-01-01"),
            make_doc("2", "Python scripting", "easy language", "2024-02-01"),
        ]);

        // Serialize via serde_json (proxy for serde_wasm_bindgen in non-WASM tests)
        let json = serde_json::to_string(&index).unwrap();
        let restored: SearchIndex = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.doc_count(), 2);

        let results = restored.search("Rust", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    // ── Incremental add ──────────────────────────────────────────────────

    #[test]
    fn incremental_add() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[make_doc("1", "first document", "body", "2024-01-01")]);
        index.add(&[make_doc("2", "second document", "body", "2024-02-01")]);
        assert_eq!(index.doc_count(), 2);

        assert_eq!(index.search("first", &SearchOptions::default()).len(), 1);
        assert_eq!(index.search("second", &SearchOptions::default()).len(), 1);
    }

    // ── Many documents ───────────────────────────────────────────────────

    #[test]
    fn many_documents() {
        let mut index = SearchIndex::new(test_schema());
        let docs: Vec<HashMap<String, String>> = (0..200)
            .map(|i| make_doc(&format!("doc{i}"), &format!("document number {i}"), "content", "2024-01-01"))
            .collect();
        index.add(&docs);
        assert_eq!(index.doc_count(), 200);

        let results = index.search("document", &SearchOptions::default());
        assert_eq!(results.len(), 200);
    }

    // ── Cross-field search ───────────────────────────────────────────────

    #[test]
    fn search_finds_across_fields() {
        let mut index = SearchIndex::new(test_schema());
        index.add(&[
            make_doc("1", "only in title here", "nothing relevant", "2024-01-01"),
            make_doc("2", "nothing relevant", "only in body here", "2024-02-01"),
        ]);

        // "title" as term should match doc1 (title field) but not doc2
        let results = index.search("title", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");

        // "body" as term matches doc2
        let results = index.search("body", &SearchOptions::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "2");
    }
}
