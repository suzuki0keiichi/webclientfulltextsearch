use wasm_bindgen::prelude::*;

use bloomfilter::Bloom;
use wasm_bindgen::__rt::std::collections::{HashSet};
use wasm_bindgen::__rt::std::sync::Mutex;
use serde::{Deserialize, Serialize};

#[macro_use]
extern crate lazy_static;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const BLOOM_ITEMS_COUNT: usize = 1000;
const BLOOM_FP_RATE: f64 = 0.05;

struct Filter {
    id: String,
    bloom: Bloom<String>,
}

lazy_static! {
static ref FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static ref BIT_INDEXES: Mutex<Vec<Vec<u16>>> = Mutex::new(Vec::new());
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[derive(Serialize, Deserialize)]
struct Content {
    id: String,
    text: String,
}

#[wasm_bindgen]
pub fn sandbox() {
}

#[wasm_bindgen]
pub fn generate(contents: JsValue) {
    let _contents: Vec<Content> = contents.into_serde().unwrap();
    let mut filters = FILTERS.lock().unwrap();
    let mut bit_indexes = BIT_INDEXES.lock().unwrap();

    filters.clear();
    bit_indexes.clear();

    let bit_len = Bloom::<String>::compute_bitmap_size(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE) * 8;

    for _i in 0..bit_len {
        bit_indexes.push(Vec::new());
    }

    let parent = Bloom::<String>::new_for_fp_rate(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE);

    for content in _contents.iter() {
        let filter = generate_filter(&content.text, &parent);
        let bitmap = &filter.bitmap();

        for i in 0..bit_len {
            if bitmap[i / 8] & (1 << (i % 8)) != 0 {
                bit_indexes.get_mut(i).unwrap().push(filters.len() as u16);
            }
        }

        filters.push(Filter {
            id: content.id.clone(),
            bloom: filter,
        });
    }
}

#[wasm_bindgen]
pub fn search2(text: String) -> JsValue {
    // あっさり版
    let mut found: Vec<String> = Vec::new();
    let filters = FILTERS.lock().unwrap();
    let grams = ngrams(&text);
    let grams_len: usize = grams.len();

    for filter in filters.iter() {
        if contains(&filter.bloom, &grams, grams_len) {
            found.push(filter.id.clone());
        }
    }

    JsValue::from_serde(&found).unwrap()
}

#[wasm_bindgen]
pub fn search(text: String) -> JsValue {
    // あっさりしない版
    let filters = FILTERS.lock().unwrap();
    let filter = generate_filter(&text, &filters[0].bloom);
    let bitmap = filter.bitmap();
    let mut found: HashSet<String> = HashSet::new();
    let bit_indexes = BIT_INDEXES.lock().unwrap();
    let grams = ngrams(&text);
    let grams_len: usize = grams.len();
    let bit_len = Bloom::<String>::compute_bitmap_size(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE) * 8;

    for i in 0..bit_len {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            for filter_index in bit_indexes[i].iter() {
                let filter = filters.get(filter_index.clone() as usize).unwrap();

                if !found.contains(&filter.id) && contains(&filter.bloom, &grams, grams_len) {
                    found.insert(filter.id.clone());
                }
            }
        }
    }

    JsValue::from_serde(&found).unwrap()
}

fn contains(bloom: &Bloom<String>, grams: &HashSet<String>, grams_len: usize) -> bool {
    let count: usize = grams.iter().map(|gram| {
        if bloom.check(&gram) {
            1
        } else {
            0
        }
    }).sum();

    count == grams_len
}

fn generate_filter(text: &String, parent: &Bloom<String>) -> Bloom<String> {
    let words: HashSet<String> = ngrams(text);

    let mut bloom = Bloom::<String>::from_existing(
        parent.bitmap().as_ref(),
        parent.number_of_bits(),
        parent.number_of_hash_functions(),
        parent.sip_keys());

    bloom.clear();

    for word in words.iter() {
        // log(format!("set {}", &word).as_str());
        bloom.set(&word);
    }

    bloom
}

const N: usize = 3;

fn ngrams(text: &String) -> HashSet<String> {
    let mut grams: HashSet<String> = HashSet::new();
    let mut word: String = String::new();

    for c in text.chars() {
        match c {
            c if c.is_whitespace() || c.is_control() => {
                // スペースなどで区切られていたらその前後は別の単語として扱う
                let count = word.chars().count();
                if count > 0 && count < N {
                    grams.insert(word.clone());
                    word.clear();
                }
            }
            _ => {
                // 全角→半角とかもあるが、実験の実装なのでやらない
                if c.is_uppercase() {
                    word.push_str(c.to_lowercase().to_string().as_str());
                } else {
                    word.push(c);
                }

                let count = word.chars().count();
                if count >= N {
                    if count > N {
                        word.remove(0);
                    }

                    grams.insert(word.clone());
                }
            }
        }
    }

    grams
}
